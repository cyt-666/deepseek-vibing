use crate::config::TrajectoryConfig;
use crate::config::TrajectoryDetail;
use chrono::Datelike;
use chrono::Utc;
use deepseek_vibing_protocol::protocol::EventMsg;
use deepseek_vibing_protocol::protocol::ExecCommandStatus;
use deepseek_vibing_protocol::protocol::PatchApplyStatus;
use deepseek_vibing_protocol::protocol::SandboxPolicy;
use deepseek_vibing_protocol::protocol::TokenUsage;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;
use tracing::warn;

const TRAJECTORY_FILE_PREFIX: &str = "trajectory-";
const TRAJECTORY_FILE_EXT: &str = "jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectorySandboxTier {
    Function,
    Container,
    MicroVm,
    FullVm,
    External,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryNetworkStatus {
    Restricted,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryTextSummary {
    pub text: String,
    pub original_bytes: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub non_cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryToolError {
    pub kind: String,
    pub message: String,
    pub tool: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryToolCall {
    pub call_id: String,
    pub turn_id: String,
    pub tool: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<TrajectoryTextSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<TrajectoryTextSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregated_output: Option<TrajectoryTextSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_changed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TrajectoryToolError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryTurn {
    pub turn_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TrajectoryUsage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<TrajectoryToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<TrajectoryTextSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectorySummary {
    pub trajectory_id: String,
    pub path: PathBuf,
    pub thread_id: String,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    pub started_at_ms: i64,
    pub cwd: PathBuf,
    pub sandbox_tier: TrajectorySandboxTier,
    pub network: TrajectoryNetworkStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<TrajectoryTurn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_trace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryReplayReport {
    pub trajectory_id: String,
    pub path: PathBuf,
    pub turn_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failure_turns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_zero_exits: Vec<TrajectoryToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patch_failures: Vec<TrajectoryToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approval_denials: Vec<TrajectoryToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_diff: Option<TrajectoryTextSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_trace_root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_reduced_state: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct TrajectorySessionMetadata {
    pub thread_id: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub cwd: PathBuf,
    pub sandbox_tier: TrajectorySandboxTier,
    pub network: TrajectoryNetworkStatus,
}

#[derive(Clone, Debug)]
pub struct TrajectoryRecorder {
    inner: Option<Arc<TrajectoryRecorderInner>>,
}

#[derive(Debug)]
struct TrajectoryRecorderInner {
    path: PathBuf,
    output_max_bytes: usize,
    diff_max_bytes: usize,
    raw_trace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TrajectorySessionLine {
    trajectory_id: String,
    thread_id: String,
    provider: String,
    model: String,
    reasoning_effort: Option<String>,
    started_at_ms: i64,
    cwd: PathBuf,
    sandbox_tier: TrajectorySandboxTier,
    network: TrajectoryNetworkStatus,
    raw_trace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TrajectoryLine {
    SessionStarted(TrajectorySessionLine),
    TurnStarted {
        turn_id: String,
        started_at_ms: Option<i64>,
    },
    TurnEnded {
        turn_id: String,
        status: String,
        completed_at_ms: Option<i64>,
        duration_ms: Option<i64>,
    },
    TokenUsage {
        turn_id: String,
        usage: TrajectoryUsage,
    },
    ToolCall(TrajectoryToolCall),
    TurnDiff {
        turn_id: String,
        diff: TrajectoryTextSummary,
    },
}

impl TrajectoryRecorder {
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn start(
        config: &TrajectoryConfig,
        deepseek_vibing_home: &Path,
        provider_id: &str,
        metadata: TrajectorySessionMetadata,
    ) -> Self {
        if !config.is_enabled_for_provider(provider_id) {
            return Self::disabled();
        }

        let root_dir = config
            .root_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| default_trajectory_root(deepseek_vibing_home));
        let raw_trace_root =
            matches!(config.detail, TrajectoryDetail::Raw).then(|| root_dir.join("raw"));
        let now = Utc::now();
        let day_dir = root_dir.join(format!(
            "{:04}/{:02}/{:02}",
            now.year(),
            now.month(),
            now.day()
        ));
        if let Err(err) = std::fs::create_dir_all(&day_dir) {
            warn!("failed to create trajectory directory: {err:#}");
            return Self::disabled();
        }
        if let Some(raw_root) = raw_trace_root.as_ref()
            && let Err(err) = std::fs::create_dir_all(raw_root)
        {
            warn!("failed to create raw trace directory for trajectory: {err:#}");
            return Self::disabled();
        }

        let trajectory_id = format!(
            "{}{}-{}",
            TRAJECTORY_FILE_PREFIX,
            now.format("%Y%m%dT%H%M%S%.3fZ"),
            sanitize_path_component(&metadata.thread_id)
        );
        let path = day_dir.join(format!("{trajectory_id}.{TRAJECTORY_FILE_EXT}"));
        let session_line = TrajectoryLine::SessionStarted(TrajectorySessionLine {
            trajectory_id,
            thread_id: metadata.thread_id,
            provider: metadata.provider,
            model: metadata.model,
            reasoning_effort: metadata.reasoning_effort,
            started_at_ms: now.timestamp_millis(),
            cwd: metadata.cwd,
            sandbox_tier: metadata.sandbox_tier,
            network: metadata.network,
            raw_trace_root: raw_trace_root.clone(),
        });
        let inner = Arc::new(TrajectoryRecorderInner {
            path,
            output_max_bytes: config.output_max_bytes,
            diff_max_bytes: config.diff_max_bytes,
            raw_trace_root,
        });
        if let Err(err) = append_line(&inner.path, &session_line) {
            warn!("failed to initialize trajectory summary: {err:#}");
            return Self::disabled();
        }
        debug!("recording trajectory summary at {}", inner.path.display());
        Self { inner: Some(inner) }
    }

    pub fn raw_trace_root(&self) -> Option<PathBuf> {
        self.inner.as_ref()?.raw_trace_root.clone()
    }

    pub fn record_event(&self, default_turn_id: &str, event: &EventMsg) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Some(line) = line_for_event(
            default_turn_id,
            event,
            inner.output_max_bytes,
            inner.diff_max_bytes,
        ) else {
            return;
        };
        if let Err(err) = append_line(&inner.path, &line) {
            warn!("failed to append trajectory summary event: {err:#}");
        }
    }
}

pub fn default_trajectory_root(deepseek_vibing_home: &Path) -> PathBuf {
    deepseek_vibing_home.join("trajectories")
}

pub fn trajectory_sandbox_tier(policy: &SandboxPolicy) -> TrajectorySandboxTier {
    match policy {
        SandboxPolicy::ExternalSandbox { .. } => TrajectorySandboxTier::External,
        SandboxPolicy::DangerFullAccess
        | SandboxPolicy::ReadOnly { .. }
        | SandboxPolicy::WorkspaceWrite { .. } => TrajectorySandboxTier::Function,
    }
}

pub fn trajectory_network_status(policy: &SandboxPolicy) -> TrajectoryNetworkStatus {
    if policy.has_full_network_access() {
        TrajectoryNetworkStatus::Enabled
    } else {
        TrajectoryNetworkStatus::Restricted
    }
}

pub fn list_trajectory_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_trajectory_files(root, &mut files)?;
    files.sort();
    files.reverse();
    Ok(files)
}

pub fn resolve_trajectory_path(root: &Path, selector: &str) -> anyhow::Result<PathBuf> {
    let candidate = PathBuf::from(selector);
    if candidate.exists() {
        return Ok(candidate);
    }
    let mut matches = list_trajectory_files(root)?
        .into_iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == selector || name.contains(selector))
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.reverse();
    match matches.as_slice() {
        [path] => Ok(path.clone()),
        [] => anyhow::bail!(
            "trajectory `{selector}` was not found under {}",
            root.display()
        ),
        _ => anyhow::bail!("trajectory selector `{selector}` is ambiguous"),
    }
}

pub fn load_trajectory_summary(path: &Path) -> anyhow::Result<TrajectorySummary> {
    let lines = read_lines(path)?;
    reduce_lines(path.to_path_buf(), lines)
}

pub fn replay_trajectory(path: &Path) -> anyhow::Result<TrajectoryReplayReport> {
    let summary = load_trajectory_summary(path)?;
    let mut failure_turns = Vec::new();
    let mut non_zero_exits = Vec::new();
    let mut patch_failures = Vec::new();
    let mut approval_denials = Vec::new();
    let mut final_diff = None;

    for turn in &summary.turns {
        if turn.status != "completed" {
            failure_turns.push(turn.turn_id.clone());
        }
        if let Some(diff) = turn.diff.clone() {
            final_diff = Some(diff);
        }
        for tool_call in &turn.tool_calls {
            if tool_call.exit_code.is_some_and(|code| code != 0) {
                non_zero_exits.push(tool_call.clone());
            }
            if tool_call.tool == "apply_patch" && tool_call.success == Some(false) {
                patch_failures.push(tool_call.clone());
            }
            if tool_call.status == "declined" {
                approval_denials.push(tool_call.clone());
            }
        }
    }

    let raw_reduced_state = summary.raw_trace_root.as_ref().and_then(find_reduced_state);

    Ok(TrajectoryReplayReport {
        trajectory_id: summary.trajectory_id,
        path: summary.path,
        turn_count: summary.turns.len(),
        failure_turns,
        non_zero_exits,
        patch_failures,
        approval_denials,
        final_diff,
        raw_trace_root: summary.raw_trace_root,
        raw_reduced_state,
    })
}

fn line_for_event(
    default_turn_id: &str,
    event: &EventMsg,
    output_max_bytes: usize,
    diff_max_bytes: usize,
) -> Option<TrajectoryLine> {
    match event {
        EventMsg::TurnStarted(event) => Some(TrajectoryLine::TurnStarted {
            turn_id: event.turn_id.clone(),
            started_at_ms: event.started_at.map(|seconds| seconds * 1000),
        }),
        EventMsg::TurnComplete(event) => Some(TrajectoryLine::TurnEnded {
            turn_id: event.turn_id.clone(),
            status: "completed".to_string(),
            completed_at_ms: event.completed_at.map(|seconds| seconds * 1000),
            duration_ms: event.duration_ms,
        }),
        EventMsg::TurnAborted(event) => Some(TrajectoryLine::TurnEnded {
            turn_id: event
                .turn_id
                .clone()
                .unwrap_or_else(|| default_turn_id.to_string()),
            status: format!("aborted:{:?}", event.reason).to_ascii_lowercase(),
            completed_at_ms: event.completed_at.map(|seconds| seconds * 1000),
            duration_ms: event.duration_ms,
        }),
        EventMsg::TokenCount(event) => event.info.as_ref().map(|info| TrajectoryLine::TokenUsage {
            turn_id: default_turn_id.to_string(),
            usage: usage_summary(&info.last_token_usage),
        }),
        EventMsg::ExecCommandEnd(event) => {
            let status = exec_status_label(&event.status);
            let error = exec_error(
                &event.status,
                event.exit_code,
                &event.stderr,
                &event.formatted_output,
            );
            Some(TrajectoryLine::ToolCall(TrajectoryToolCall {
                call_id: event.call_id.clone(),
                turn_id: event.turn_id.clone(),
                tool: "exec_command".to_string(),
                status,
                command: Some(event.command.clone()),
                cwd: Some(event.cwd.clone().into()),
                duration_ms: Some(event.duration.as_millis() as u64),
                exit_code: Some(event.exit_code),
                stdout: Some(summarize_text(&event.stdout, output_max_bytes)),
                stderr: Some(summarize_text(&event.stderr, output_max_bytes)),
                aggregated_output: Some(summarize_text(&event.aggregated_output, output_max_bytes)),
                files_changed: Vec::new(),
                success: Some(
                    matches!(event.status, ExecCommandStatus::Completed) && event.exit_code == 0,
                ),
                error,
            }))
        }
        EventMsg::PatchApplyEnd(event) => {
            let mut files_changed = event
                .changes
                .keys()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            files_changed.sort();
            let status = patch_status_label(&event.status);
            let error = patch_error(&event.status, &event.stderr, event.success);
            Some(TrajectoryLine::ToolCall(TrajectoryToolCall {
                call_id: event.call_id.clone(),
                turn_id: if event.turn_id.is_empty() {
                    default_turn_id.to_string()
                } else {
                    event.turn_id.clone()
                },
                tool: "apply_patch".to_string(),
                status,
                command: None,
                cwd: None,
                duration_ms: None,
                exit_code: None,
                stdout: Some(summarize_text(&event.stdout, output_max_bytes)),
                stderr: Some(summarize_text(&event.stderr, output_max_bytes)),
                aggregated_output: None,
                files_changed,
                success: Some(event.success),
                error,
            }))
        }
        EventMsg::TurnDiff(event) => Some(TrajectoryLine::TurnDiff {
            turn_id: default_turn_id.to_string(),
            diff: summarize_text(&event.unified_diff, diff_max_bytes),
        }),
        _ => None,
    }
}

fn append_line(path: &Path, line: &TrajectoryLine) -> anyhow::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(line)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn read_lines(path: &Path) -> anyhow::Result<Vec<TrajectoryLine>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed = serde_json::from_str(&line)
            .map_err(|err| anyhow::anyhow!("invalid trajectory JSONL line {}: {err}", index + 1))?;
        lines.push(parsed);
    }
    Ok(lines)
}

fn reduce_lines(path: PathBuf, lines: Vec<TrajectoryLine>) -> anyhow::Result<TrajectorySummary> {
    let mut session = None;
    let mut turns = Vec::<TrajectoryTurn>::new();

    for line in lines {
        match line {
            TrajectoryLine::SessionStarted(line) => session = Some(line),
            TrajectoryLine::TurnStarted {
                turn_id,
                started_at_ms,
            } => {
                let turn = turn_mut(&mut turns, &turn_id);
                turn.started_at_ms = started_at_ms;
                turn.status = "running".to_string();
            }
            TrajectoryLine::TurnEnded {
                turn_id,
                status,
                completed_at_ms,
                duration_ms,
            } => {
                let turn = turn_mut(&mut turns, &turn_id);
                turn.status = status;
                turn.completed_at_ms = completed_at_ms;
                turn.duration_ms = duration_ms;
            }
            TrajectoryLine::TokenUsage { turn_id, usage } => {
                turn_mut(&mut turns, &turn_id).token_usage = Some(usage);
            }
            TrajectoryLine::ToolCall(tool_call) => {
                let turn_id = tool_call.turn_id.clone();
                turn_mut(&mut turns, &turn_id).tool_calls.push(tool_call);
            }
            TrajectoryLine::TurnDiff { turn_id, diff } => {
                turn_mut(&mut turns, &turn_id).diff = Some(diff);
            }
        }
    }

    let session = session.ok_or_else(|| anyhow::anyhow!("trajectory has no session header"))?;
    Ok(TrajectorySummary {
        trajectory_id: session.trajectory_id,
        path,
        thread_id: session.thread_id,
        provider: session.provider,
        model: session.model,
        reasoning_effort: session.reasoning_effort,
        started_at_ms: session.started_at_ms,
        cwd: session.cwd,
        sandbox_tier: session.sandbox_tier,
        network: session.network,
        turns,
        raw_trace_root: session.raw_trace_root,
    })
}

fn turn_mut<'a>(turns: &'a mut Vec<TrajectoryTurn>, turn_id: &str) -> &'a mut TrajectoryTurn {
    if let Some(index) = turns.iter().position(|turn| turn.turn_id == turn_id) {
        return &mut turns[index];
    }
    turns.push(TrajectoryTurn {
        turn_id: turn_id.to_string(),
        status: "unknown".to_string(),
        started_at_ms: None,
        completed_at_ms: None,
        duration_ms: None,
        token_usage: None,
        tool_calls: Vec::new(),
        diff: None,
    });
    turns.last_mut().expect("turn was just pushed")
}

fn usage_summary(usage: &TokenUsage) -> TrajectoryUsage {
    TrajectoryUsage {
        input_tokens: usage.input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        non_cached_input_tokens: (usage.input_tokens - usage.cached_input_tokens).max(0),
        output_tokens: usage.output_tokens,
        reasoning_output_tokens: usage.reasoning_output_tokens,
        total_tokens: usage.total_tokens,
    }
}

fn summarize_text(text: &str, max_bytes: usize) -> TrajectoryTextSummary {
    let original_bytes = text.len();
    if original_bytes <= max_bytes {
        return TrajectoryTextSummary {
            text: text.to_string(),
            original_bytes,
            truncated: false,
        };
    }

    let mut end = 0;
    for (index, ch) in text.char_indices() {
        let next = index + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        end = next;
    }
    TrajectoryTextSummary {
        text: text[..end].to_string(),
        original_bytes,
        truncated: true,
    }
}

fn exec_status_label(status: &ExecCommandStatus) -> String {
    match status {
        ExecCommandStatus::Completed => "completed",
        ExecCommandStatus::Failed => "failed",
        ExecCommandStatus::Declined => "declined",
    }
    .to_string()
}

fn patch_status_label(status: &PatchApplyStatus) -> String {
    match status {
        PatchApplyStatus::Completed => "completed",
        PatchApplyStatus::Failed => "failed",
        PatchApplyStatus::Declined => "declined",
    }
    .to_string()
}

fn exec_error(
    status: &ExecCommandStatus,
    exit_code: i32,
    stderr: &str,
    formatted_output: &str,
) -> Option<TrajectoryToolError> {
    if matches!(status, ExecCommandStatus::Completed) && exit_code == 0 {
        return None;
    }
    let kind = if matches!(status, ExecCommandStatus::Declined) {
        "approval_denied"
    } else if exit_code != 0 {
        "non_zero_exit"
    } else {
        "exec_failed"
    };
    let message = if !stderr.trim().is_empty() {
        stderr.trim()
    } else {
        formatted_output.trim()
    };
    Some(TrajectoryToolError {
        kind: kind.to_string(),
        message: message.to_string(),
        tool: "exec_command".to_string(),
    })
}

fn patch_error(
    status: &PatchApplyStatus,
    stderr: &str,
    success: bool,
) -> Option<TrajectoryToolError> {
    if matches!(status, PatchApplyStatus::Completed) && success {
        return None;
    }
    let kind = if matches!(status, PatchApplyStatus::Declined) {
        "approval_denied"
    } else {
        "patch_failed"
    };
    Some(TrajectoryToolError {
        kind: kind.to_string(),
        message: stderr.trim().to_string(),
        tool: "apply_patch".to_string(),
    })
}

fn is_trajectory_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == TRAJECTORY_FILE_EXT)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(TRAJECTORY_FILE_PREFIX))
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn find_reduced_state(raw_root: &PathBuf) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    collect_reduced_state_files(raw_root, &mut candidates).ok()?;
    candidates.sort();
    candidates.pop()
}

fn collect_trajectory_files(dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if entry.file_name() == "raw" {
                continue;
            }
            collect_trajectory_files(&path, files)?;
        } else if file_type.is_file() && is_trajectory_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_reduced_state_files(dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_reduced_state_files(&path, files)?;
        } else if file_type.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == deepseek_vibing_rollout_trace::REDUCED_STATE_FILE_NAME)
        {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TrajectoryEnabled;
    use deepseek_vibing_protocol::config_types::ModeKind;
    use deepseek_vibing_protocol::parse_command::ParsedCommand;
    use deepseek_vibing_protocol::protocol::ExecCommandEndEvent;
    use deepseek_vibing_protocol::protocol::NetworkAccess;
    use deepseek_vibing_protocol::protocol::ReasoningRawContentDeltaEvent;
    use deepseek_vibing_protocol::protocol::TurnCompleteEvent;
    use deepseek_vibing_protocol::protocol::TurnDiffEvent;
    use deepseek_vibing_protocol::protocol::TurnStartedEvent;
    use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;
    use std::time::Duration;

    #[test]
    fn summarize_text_obeys_byte_limit() {
        let summary = summarize_text("hello world", 5);
        assert_eq!(summary.text, "hello");
        assert_eq!(summary.original_bytes, 11);
        assert!(summary.truncated);
    }

    #[test]
    fn sandbox_tier_maps_existing_policies() {
        assert_eq!(
            trajectory_sandbox_tier(&SandboxPolicy::new_read_only_policy()),
            TrajectorySandboxTier::Function
        );
        assert_eq!(
            trajectory_sandbox_tier(&SandboxPolicy::ExternalSandbox {
                network_access: NetworkAccess::Restricted,
            }),
            TrajectorySandboxTier::External
        );
    }

    #[test]
    fn network_status_uses_policy_network_access() {
        assert_eq!(
            trajectory_network_status(&SandboxPolicy::ReadOnly {
                network_access: false,
            }),
            TrajectoryNetworkStatus::Restricted
        );
        assert_eq!(
            trajectory_network_status(&SandboxPolicy::DangerFullAccess),
            TrajectoryNetworkStatus::Enabled
        );
    }

    #[test]
    fn recorder_writes_summary_and_replay_without_raw_reasoning() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = AbsolutePathBuf::try_from(temp.path().join("trajectories"))
            .expect("absolute trajectory root");
        let config = TrajectoryConfig {
            enabled: TrajectoryEnabled::Always,
            detail: TrajectoryDetail::Summary,
            root_dir: Some(root),
            output_max_bytes: 8,
            diff_max_bytes: 16,
        };
        let cwd =
            AbsolutePathBuf::try_from(temp.path().join("repo")).expect("absolute cwd for test");
        let recorder = TrajectoryRecorder::start(
            &config,
            temp.path(),
            "deepseek",
            TrajectorySessionMetadata {
                thread_id: "thread-1".to_string(),
                provider: "deepseek".to_string(),
                model: "deepseek-v4-pro".to_string(),
                reasoning_effort: Some("high".to_string()),
                cwd: cwd.clone().into(),
                sandbox_tier: TrajectorySandboxTier::Function,
                network: TrajectoryNetworkStatus::Restricted,
            },
        );

        recorder.record_event(
            "turn-1",
            &EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: "turn-1".to_string(),
                trace_id: None,
                started_at: Some(1),
                model_context_window: None,
                collaboration_mode_kind: ModeKind::Default,
            }),
        );
        recorder.record_event(
            "turn-1",
            &EventMsg::ReasoningRawContentDelta(ReasoningRawContentDeltaEvent {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "reasoning-1".to_string(),
                delta: "SECRET_REASONING_TEXT".to_string(),
                content_index: 0,
            }),
        );
        recorder.record_event(
            "turn-1",
            &EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: "call-1".to_string(),
                process_id: None,
                turn_id: "turn-1".to_string(),
                completed_at_ms: 1_500,
                command: vec!["false".to_string()],
                cwd: cwd.clone(),
                parsed_cmd: vec![ParsedCommand::Unknown {
                    cmd: "false".to_string(),
                }],
                source: Default::default(),
                interaction_input: None,
                stdout: "0123456789".to_string(),
                stderr: "failed hard".to_string(),
                aggregated_output: "0123456789".to_string(),
                exit_code: 1,
                duration: Duration::from_millis(12),
                formatted_output: "failed hard".to_string(),
                status: ExecCommandStatus::Completed,
            }),
        );
        recorder.record_event(
            "turn-1",
            &EventMsg::TurnDiff(TurnDiffEvent {
                unified_diff: "diff --git a/file b/file\n+changed\n".to_string(),
            }),
        );
        recorder.record_event(
            "turn-1",
            &EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: "turn-1".to_string(),
                last_agent_message: None,
                completed_at: Some(2),
                duration_ms: Some(1_000),
                time_to_first_token_ms: None,
            }),
        );

        let files = list_trajectory_files(&default_trajectory_root(temp.path()))
            .expect("list trajectory files");
        assert_eq!(files.len(), 1);
        let jsonl = std::fs::read_to_string(&files[0]).expect("read trajectory jsonl");
        assert!(!jsonl.contains("SECRET_REASONING_TEXT"));

        let summary = load_trajectory_summary(&files[0]).expect("load summary");
        assert_eq!(summary.turns.len(), 1);
        assert_eq!(summary.turns[0].tool_calls.len(), 1);
        let stdout = summary.turns[0].tool_calls[0]
            .stdout
            .as_ref()
            .expect("stdout summary");
        assert_eq!(stdout.text, "01234567");
        assert!(stdout.truncated);

        let report = replay_trajectory(&files[0]).expect("replay trajectory");
        assert_eq!(report.turn_count, 1);
        assert_eq!(report.non_zero_exits.len(), 1);
        assert!(report.failure_turns.is_empty());
    }
}
