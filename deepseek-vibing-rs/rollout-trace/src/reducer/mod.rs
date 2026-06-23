//! Deterministic replay from raw trace events to `RolloutTrace`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde_json::Value;

use crate::bundle::MANIFEST_FILE_NAME;
use crate::bundle::RAW_EVENT_LOG_FILE_NAME;
use crate::bundle::REDUCED_TRACE_SCHEMA_VERSION;
use crate::bundle::TraceBundleManifest;
use crate::model::ExecutionStatus;
use crate::model::RolloutTrace;
use crate::payload::RawPayloadRef;
use crate::raw_event::RawTraceEvent;
use crate::raw_event::RawTraceEventPayload;

mod compaction;
mod conversation;
mod inference;
#[cfg(test)]
pub(crate) mod test_support;
mod thread;
mod tool;

use self::compaction::StartedCompactionRequest;
use self::inference::StartedInferenceCall;
use self::tool::ObservedAgentResultEdge;
use self::tool::PendingAgentInteractionEdge;
use self::tool::ToolCallStarted;

/// Replays a local trace bundle into a reduced rollout graph.
pub fn replay_bundle(bundle_dir: impl AsRef<Path>) -> Result<RolloutTrace> {
    let bundle_dir = bundle_dir.as_ref();
    let manifest: TraceBundleManifest =
        serde_json::from_reader(File::open(bundle_dir.join(MANIFEST_FILE_NAME))?)
            .with_context(|| format!("read {}", bundle_dir.join(MANIFEST_FILE_NAME).display()))?;
    let mut reducer = TraceReducer {
        rollout: RolloutTrace::new(
            REDUCED_TRACE_SCHEMA_VERSION,
            manifest.trace_id,
            manifest.rollout_id,
            manifest.root_thread_id,
            manifest.started_at_unix_ms,
        ),
        bundle_dir: bundle_dir.to_path_buf(),
        next_conversation_item_ordinal: 1,
        next_terminal_operation_ordinal: 1,
        thread_conversation_snapshots: BTreeMap::new(),
        pending_compaction_replacement_item_ids: BTreeMap::new(),
        pending_agent_interaction_edges: Vec::new(),
    };

    let event_log_path = bundle_dir.join(RAW_EVENT_LOG_FILE_NAME);
    let event_log = File::open(&event_log_path)
        .with_context(|| format!("open trace event log {}", event_log_path.display()))?;
    for (line_index, line) in BufReader::new(event_log).lines().enumerate() {
        let line = line.with_context(|| format!("read trace event line {}", line_index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: RawTraceEvent = serde_json::from_str(&line)
            .with_context(|| format!("parse trace event line {}", line_index + 1))?;
        reducer.apply_event(event)?;
    }
    // Spawn edges prefer the child task message as their target, but a child can
    // fail before that message is ever reduced. Only after replaying the whole
    // bundle do we know which spawn deliveries need the child-thread fallback.
    reducer.resolve_pending_spawn_edge_fallbacks()?;

    Ok(reducer.rollout)
}

struct TraceReducer {
    rollout: RolloutTrace,
    bundle_dir: PathBuf,
    next_conversation_item_ordinal: u64,
    next_terminal_operation_ordinal: u64,
    /// Last model-visible conversation snapshot per thread.
    ///
    /// Requests and responses both advance this sequence because both are
    /// model-facing payloads. Repeated request snapshots reuse item IDs only
    /// when the same normalized item appears at the same position; identical
    /// content at a new position must remain a distinct conversation item.
    thread_conversation_snapshots: BTreeMap<String, Vec<String>>,
    /// Replacement snapshot installed by compaction but not yet seen in a sampling request.
    ///
    /// The first full request after compaction should compare against the installed replacement
    /// history, not against the pre-compaction request. That keeps repeated prefix/context messages
    /// as fresh post-compaction conversation items while still reusing the summary/replacement
    /// items that actually became live history.
    pending_compaction_replacement_item_ids: BTreeMap<String, Vec<String>>,
    /// Multi-agent deliveries whose recipient-side transcript item has not been observed yet.
    ///
    /// V2 agent tools enqueue mailbox messages in the target thread. The trace event for the
    /// sending tool arrives before the recipient inference request materializes that mailbox item
    /// as a `ConversationItem`, so the reducer keeps the delivery edge pending until it can point
    /// at the exact model-visible item instead of a coarse thread.
    pending_agent_interaction_edges: Vec<PendingAgentInteractionEdge>,
}

impl TraceReducer {
    fn read_payload_json(&self, payload: &RawPayloadRef) -> Result<Value> {
        // Reducers keep raw bodies out of the graph, but typed replay sometimes
        // needs a small subset of fields to build semantic objects.
        let payload_path = self.bundle_dir.join(&payload.path);
        let file = File::open(&payload_path)
            .with_context(|| format!("open payload {}", payload.raw_payload_id))?;
        serde_json::from_reader(file)
            .with_context(|| format!("parse payload {}", payload.raw_payload_id))
    }

    fn apply_event(&mut self, event: RawTraceEvent) -> Result<()> {
        // Raw payload refs are reducer-wide evidence, not owned by a single
        // semantic arm. Keep this bookkeeping separate so typed reduction can
        // stay strict without duplicating payload insertion in every case.
        for payload in event.payload.raw_payload_refs() {
            self.insert_raw_payload(payload);
        }

        match event.payload {
            RawTraceEventPayload::RolloutStarted {
                trace_id,
                root_thread_id,
            } => {
                self.rollout.trace_id = trace_id;
                self.rollout.root_thread_id = root_thread_id;
            }
            RawTraceEventPayload::RolloutEnded { status } => {
                self.rollout.status = status;
                self.rollout.ended_at_unix_ms = Some(event.wall_time_unix_ms);
            }
            RawTraceEventPayload::ThreadStarted {
                thread_id,
                agent_path,
                metadata_payload,
            } => {
                self.start_thread(
                    event.seq,
                    event.wall_time_unix_ms,
                    thread_id,
                    agent_path,
                    metadata_payload,
                )?;
            }
            RawTraceEventPayload::ThreadEnded { thread_id, status } => {
                self.end_thread(event.seq, event.wall_time_unix_ms, thread_id, status)?;
            }
            RawTraceEventPayload::DeepSeekVibingTurnStarted {
                deepseek_vibing_turn_id,
                thread_id,
            } => {
                self.start_deepseek_vibing_turn(
                    event.seq,
                    event.wall_time_unix_ms,
                    deepseek_vibing_turn_id,
                    thread_id,
                )?;
            }
            RawTraceEventPayload::DeepSeekVibingTurnEnded {
                deepseek_vibing_turn_id,
                status,
            } => {
                self.end_deepseek_vibing_turn(
                    event.seq,
                    event.wall_time_unix_ms,
                    event.thread_id,
                    deepseek_vibing_turn_id,
                    status,
                )?;
            }
            RawTraceEventPayload::InferenceStarted {
                inference_call_id,
                thread_id,
                deepseek_vibing_turn_id,
                model,
                provider_name,
                request_payload,
            } => {
                self.start_inference_call(
                    event.seq,
                    event.wall_time_unix_ms,
                    StartedInferenceCall {
                        inference_call_id,
                        thread_id,
                        deepseek_vibing_turn_id,
                        model,
                        provider_name,
                        request_payload,
                    },
                )?;
            }
            payload @ (RawTraceEventPayload::InferenceCompleted { .. }
            | RawTraceEventPayload::InferenceFailed { .. }
            | RawTraceEventPayload::InferenceCancelled { .. }) => {
                self.complete_inference_call(event.seq, event.wall_time_unix_ms, payload)?;
            }
            RawTraceEventPayload::ProtocolEventObserved { .. } => {
                // Protocol wrappers are raw debug breadcrumbs. Typed hooks own
                // the reduced graph, so these payload refs are retained without
                // creating semantic objects.
            }
            RawTraceEventPayload::ToolCallStarted {
                tool_call_id,
                model_visible_call_id,
                requester,
                kind,
                summary,
                invocation_payload,
            } => {
                self.start_tool_call(
                    event.seq,
                    event.wall_time_unix_ms,
                    event.thread_id,
                    event.deepseek_vibing_turn_id,
                    ToolCallStarted {
                        tool_call_id,
                        model_visible_call_id,
                        requester,
                        kind,
                        summary,
                        invocation_payload,
                    },
                )?;
            }
            RawTraceEventPayload::McpToolCallCorrelationAssigned {
                tool_call_id,
                mcp_call_id,
            } => {
                self.assign_mcp_tool_call_correlation(tool_call_id, mcp_call_id)?;
            }
            RawTraceEventPayload::ToolCallRuntimeStarted {
                tool_call_id,
                runtime_payload,
            } => {
                self.start_tool_runtime_observation(
                    event.seq,
                    event.wall_time_unix_ms,
                    tool_call_id,
                    runtime_payload,
                )?;
            }
            RawTraceEventPayload::ToolCallRuntimeEnded {
                tool_call_id,
                status,
                runtime_payload,
            } => {
                self.end_tool_runtime_observation(
                    event.seq,
                    event.wall_time_unix_ms,
                    tool_call_id,
                    status,
                    runtime_payload,
                )?;
            }
            RawTraceEventPayload::ToolCallEnded {
                tool_call_id,
                status,
                result_payload,
            } => {
                self.end_tool_call(
                    event.seq,
                    event.wall_time_unix_ms,
                    tool_call_id,
                    status,
                    result_payload,
                )?;
            }
            RawTraceEventPayload::CompactionRequestStarted {
                compaction_id,
                compaction_request_id,
                thread_id,
                deepseek_vibing_turn_id,
                model,
                provider_name,
                request_payload,
            } => {
                self.start_compaction_request(
                    event.seq,
                    event.wall_time_unix_ms,
                    StartedCompactionRequest {
                        compaction_id,
                        compaction_request_id,
                        thread_id,
                        deepseek_vibing_turn_id,
                        model,
                        provider_name,
                        request_payload,
                    },
                )?;
            }
            RawTraceEventPayload::CompactionRequestCompleted {
                compaction_id,
                compaction_request_id,
                response_payload,
            } => {
                self.complete_compaction_request(
                    event.seq,
                    event.wall_time_unix_ms,
                    compaction_id,
                    compaction_request_id,
                    ExecutionStatus::Completed,
                    Some(response_payload),
                )?;
            }
            RawTraceEventPayload::CompactionRequestFailed {
                compaction_id,
                compaction_request_id,
                ..
            } => {
                self.complete_compaction_request(
                    event.seq,
                    event.wall_time_unix_ms,
                    compaction_id,
                    compaction_request_id,
                    ExecutionStatus::Failed,
                    /*response_payload*/ None,
                )?;
            }
            RawTraceEventPayload::CompactionInstalled {
                compaction_id,
                checkpoint_payload,
            } => {
                let Some(thread_id) = event.thread_id else {
                    bail!("compaction installed event {compaction_id} did not include a thread id");
                };
                let Some(deepseek_vibing_turn_id) = event.deepseek_vibing_turn_id else {
                    bail!(
                        "compaction installed event {compaction_id} did not include a codex turn id"
                    );
                };
                self.reduce_compaction_installed_event(
                    event.wall_time_unix_ms,
                    thread_id,
                    deepseek_vibing_turn_id,
                    compaction_id,
                    checkpoint_payload,
                )?;
            }
            RawTraceEventPayload::AgentResultObserved {
                edge_id,
                child_thread_id,
                child_deepseek_vibing_turn_id,
                parent_thread_id,
                message,
                carried_payload,
            } => {
                self.queue_agent_result_interaction_edge(ObservedAgentResultEdge {
                    wall_time_unix_ms: event.wall_time_unix_ms,
                    edge_id,
                    child_thread_id,
                    child_deepseek_vibing_turn_id,
                    parent_thread_id,
                    message,
                    carried_payload,
                })?;
            }
            RawTraceEventPayload::Other { .. } => {
                bail!("raw trace event has no reducer implementation");
            }
        }

        Ok(())
    }

    fn insert_raw_payload(&mut self, payload: &RawPayloadRef) {
        self.rollout
            .raw_payloads
            .insert(payload.raw_payload_id.clone(), payload.clone());
    }
}
