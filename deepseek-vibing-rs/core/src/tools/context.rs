use crate::context_manager::truncate_function_output_payload;
use crate::original_image_detail::sanitize_original_image_detail;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::tools::TELEMETRY_PREVIEW_MAX_BYTES;
use crate::tools::TELEMETRY_PREVIEW_MAX_LINES;
use crate::tools::TELEMETRY_PREVIEW_TRUNCATION_NOTICE;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::unified_exec::resolve_max_tokens;
use deepseek_vibing_protocol::exec_output::ExecToolCallOutput;
use deepseek_vibing_protocol::mcp::CallToolResult;
use deepseek_vibing_protocol::models::FunctionCallOutputBody;
use deepseek_vibing_protocol::models::FunctionCallOutputContentItem;
use deepseek_vibing_protocol::models::FunctionCallOutputPayload;
use deepseek_vibing_protocol::models::ResponseInputItem;
use deepseek_vibing_protocol::models::function_call_output_content_items_to_text;
use deepseek_vibing_tools::LoadableToolSpec;
use deepseek_vibing_tools::ToolName;
use deepseek_vibing_utils_output_truncation::TruncationPolicy;
use deepseek_vibing_utils_output_truncation::formatted_truncate_text;
use deepseek_vibing_utils_string::take_bytes_at_char_boundary;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub use deepseek_vibing_tools::ToolOutput;
pub use deepseek_vibing_tools::ToolPayload;

pub(crate) fn boxed_tool_output<T>(output: T) -> Box<dyn ToolOutput>
where
    T: ToolOutput + 'static,
{
    Box::new(output)
}

pub(crate) fn structured_tool_error_text(tool: &str, kind: &str, message: &str) -> String {
    json!({
        "ok": false,
        "kind": kind,
        "message": message,
        "tool": tool,
    })
    .to_string()
}

pub(crate) fn is_structured_tool_error_text(message: &str) -> bool {
    serde_json::from_str::<JsonValue>(message)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|object| {
            object.get("ok") == Some(&JsonValue::Bool(false))
                && object.contains_key("kind")
                && object.contains_key("message")
                && object.contains_key("tool")
        })
}

pub(crate) fn structured_exec_error_text(
    tool: &str,
    kind: &str,
    message: &str,
    output: &ExecToolCallOutput,
    cwd: Option<&str>,
    truncation_policy: TruncationPolicy,
) -> String {
    let stdout = formatted_truncate_text(&output.stdout.text, truncation_policy);
    let stderr = formatted_truncate_text(&output.stderr.text, truncation_policy);
    let truncated = output.stdout.truncated_after_lines.is_some()
        || output.stderr.truncated_after_lines.is_some()
        || stdout != output.stdout.text
        || stderr != output.stderr.text;

    json!({
        "ok": false,
        "kind": kind,
        "message": message,
        "tool": tool,
        "exit_code": output.exit_code,
        "cwd": cwd.unwrap_or(""),
        "stdout": stdout,
        "stderr": stderr,
        "duration_ms": output.duration.as_millis(),
        "truncated": truncated,
    })
    .to_string()
}

pub type SharedTurnDiffTracker = Arc<Mutex<TurnDiffTracker>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolCallSource {
    Direct,
}

#[derive(Clone)]
pub struct ToolInvocation {
    pub session: Arc<Session>,
    pub turn: Arc<TurnContext>,
    pub cancellation_token: CancellationToken,
    pub tracker: SharedTurnDiffTracker,
    pub call_id: String,
    pub tool_name: ToolName,
    pub source: ToolCallSource,
    pub payload: ToolPayload,
}

#[derive(Clone, Debug)]
pub struct McpToolOutput {
    pub result: CallToolResult,
    pub tool_input: JsonValue,
    pub wall_time: Duration,
    pub original_image_detail_supported: bool,
    pub truncation_policy: TruncationPolicy,
}

impl ToolOutput for McpToolOutput {
    fn log_preview(&self) -> String {
        let payload = self.response_payload();
        let preview = payload.body.to_text().unwrap_or_else(|| {
            serde_json::to_string(&self.result.content)
                .unwrap_or_else(|err| format!("failed to serialize mcp result: {err}"))
        });
        telemetry_preview(&preview)
    }

    fn success_for_logging(&self) -> bool {
        self.result.success()
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output: self.response_payload(),
        }
    }

    fn post_tool_use_input(&self, _payload: &ToolPayload) -> Option<JsonValue> {
        Some(self.tool_input.clone())
    }

    fn post_tool_use_response(&self, _call_id: &str, _payload: &ToolPayload) -> Option<JsonValue> {
        serde_json::to_value(&self.result).ok()
    }
}

impl McpToolOutput {
    fn response_payload(&self) -> FunctionCallOutputPayload {
        let mut payload = self.result.as_function_call_output_payload();
        if let Some(items) = payload.content_items_mut() {
            sanitize_original_image_detail(self.original_image_detail_supported, items);
        }

        let wall_time_seconds = self.wall_time.as_secs_f64();
        let header = format!("Wall time: {wall_time_seconds:.4} seconds\nOutput:");

        match &mut payload.body {
            FunctionCallOutputBody::Text(text) => {
                if text.is_empty() {
                    *text = header;
                } else {
                    *text = format!("{header}\n{text}");
                }
            }
            FunctionCallOutputBody::ContentItems(items) => {
                items.insert(0, FunctionCallOutputContentItem::InputText { text: header });
            }
        }

        // This is the context-injection form, so keep it aligned with the
        // function-call output truncation that conversation history already
        // applies.
        // The text is serialized again inside the Responses payload, so allow
        // a small buffer for JSON escaping and wrapper overhead.
        truncate_function_output_payload(&payload, self.truncation_policy * 1.2)
    }
}

#[derive(Clone)]
pub struct ToolSearchOutput {
    pub tools: Vec<LoadableToolSpec>,
}

impl ToolOutput for ToolSearchOutput {
    fn log_preview(&self) -> String {
        let tools = self
            .tools
            .iter()
            .map(|tool| {
                serde_json::to_value(tool).unwrap_or_else(|err| {
                    JsonValue::String(format!("failed to serialize tool_search output: {err}"))
                })
            })
            .collect();
        telemetry_preview(&JsonValue::Array(tools).to_string())
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        ResponseInputItem::ToolSearchOutput {
            call_id: call_id.to_string(),
            status: "completed".to_string(),
            execution: "client".to_string(),
            tools: self
                .tools
                .iter()
                .map(|tool| {
                    serde_json::to_value(tool).unwrap_or_else(|err| {
                        JsonValue::String(format!("failed to serialize tool_search output: {err}"))
                    })
                })
                .collect(),
        }
    }
}

pub struct FunctionToolOutput {
    pub body: Vec<FunctionCallOutputContentItem>,
    pub success: Option<bool>,
    pub post_tool_use_response: Option<JsonValue>,
}

impl FunctionToolOutput {
    pub fn from_text(text: String, success: Option<bool>) -> Self {
        Self {
            body: vec![FunctionCallOutputContentItem::InputText { text }],
            success,
            post_tool_use_response: None,
        }
    }

    pub fn from_content(
        content: Vec<FunctionCallOutputContentItem>,
        success: Option<bool>,
    ) -> Self {
        Self {
            body: content,
            success,
            post_tool_use_response: None,
        }
    }

    pub fn into_text(self) -> String {
        function_call_output_content_items_to_text(&self.body).unwrap_or_default()
    }
}

impl ToolOutput for FunctionToolOutput {
    fn log_preview(&self) -> String {
        telemetry_preview(
            &function_call_output_content_items_to_text(&self.body).unwrap_or_default(),
        )
    }

    fn success_for_logging(&self) -> bool {
        self.success.unwrap_or(true)
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        function_tool_response(call_id, payload, self.body.clone(), self.success)
    }

    fn post_tool_use_response(&self, _call_id: &str, _payload: &ToolPayload) -> Option<JsonValue> {
        self.post_tool_use_response.clone()
    }
}

pub struct ApplyPatchToolOutput {
    pub text: String,
}

impl ApplyPatchToolOutput {
    pub fn from_text(text: String) -> Self {
        Self { text }
    }
}

impl ToolOutput for ApplyPatchToolOutput {
    fn log_preview(&self) -> String {
        telemetry_preview(&self.text)
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        function_tool_response(
            call_id,
            payload,
            vec![FunctionCallOutputContentItem::InputText {
                text: self.text.clone(),
            }],
            Some(true),
        )
    }

    fn post_tool_use_response(&self, _call_id: &str, _payload: &ToolPayload) -> Option<JsonValue> {
        Some(JsonValue::String(self.text.clone()))
    }
}

pub struct AbortedToolOutput {
    pub message: String,
}

impl ToolOutput for AbortedToolOutput {
    fn log_preview(&self) -> String {
        telemetry_preview(&self.message)
    }

    fn success_for_logging(&self) -> bool {
        false
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        match payload {
            ToolPayload::ToolSearch { .. } => ResponseInputItem::ToolSearchOutput {
                call_id: call_id.to_string(),
                status: "completed".to_string(),
                execution: "client".to_string(),
                tools: Vec::new(),
            },
            _ => function_tool_response(
                call_id,
                payload,
                vec![FunctionCallOutputContentItem::InputText {
                    text: self.message.clone(),
                }],
                /*success*/ None,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecCommandToolOutput {
    pub event_call_id: String,
    pub chunk_id: String,
    pub wall_time: Duration,
    /// Raw bytes returned for this unified exec call before any truncation.
    pub raw_output: Vec<u8>,
    pub truncation_policy: TruncationPolicy,
    pub max_output_tokens: Option<usize>,
    pub process_id: Option<i32>,
    pub exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub original_token_count: Option<usize>,
    pub hook_command: Option<String>,
}

impl ToolOutput for ExecCommandToolOutput {
    fn log_preview(&self) -> String {
        telemetry_preview(&self.response_text())
    }

    fn success_for_logging(&self) -> bool {
        self.exit_code.unwrap_or(0) == 0 || self.process_id.is_some()
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        let success = self.exit_code.unwrap_or(0) == 0 || self.process_id.is_some();
        let text = if success {
            self.response_text()
        } else {
            self.structured_error_text()
        };
        function_tool_response(
            call_id,
            payload,
            vec![FunctionCallOutputContentItem::InputText { text }],
            Some(success),
        )
    }

    fn post_tool_use_id(&self, call_id: &str) -> String {
        if self.event_call_id.is_empty() {
            call_id.to_string()
        } else {
            self.event_call_id.clone()
        }
    }

    fn post_tool_use_input(&self, _payload: &ToolPayload) -> Option<JsonValue> {
        self.hook_command
            .as_ref()
            .map(|command| serde_json::json!({ "command": command }))
    }

    fn post_tool_use_response(&self, _call_id: &str, _payload: &ToolPayload) -> Option<JsonValue> {
        if self.process_id.is_some() || self.hook_command.is_none() {
            return None;
        }

        Some(JsonValue::String(
            self.truncated_output(self.model_output_max_tokens()),
        ))
    }
}

impl ExecCommandToolOutput {
    fn model_output_max_tokens(&self) -> usize {
        resolve_max_tokens(self.max_output_tokens).min(self.truncation_policy.token_budget())
    }

    pub(crate) fn truncated_output(&self, max_tokens: usize) -> String {
        let text = String::from_utf8_lossy(&self.raw_output).to_string();
        formatted_truncate_text(&text, TruncationPolicy::Tokens(max_tokens))
    }

    fn response_text(&self) -> String {
        let mut sections = Vec::new();

        if !self.chunk_id.is_empty() {
            sections.push(format!("Chunk ID: {}", self.chunk_id));
        }

        let wall_time_seconds = self.wall_time.as_secs_f64();
        sections.push(format!("Wall time: {wall_time_seconds:.4} seconds"));

        if let Some(exit_code) = self.exit_code {
            sections.push(format!("Process exited with code {exit_code}"));
        }

        if let Some(process_id) = &self.process_id {
            sections.push(format!("Process running with session ID {process_id}"));
        }

        if let Some(original_token_count) = self.original_token_count {
            sections.push(format!("Original token count: {original_token_count}"));
        }

        sections.push("Output:".to_string());
        sections.push(self.truncated_output(self.model_output_max_tokens()));

        sections.join("\n")
    }

    fn structured_error_text(&self) -> String {
        let output = self.truncated_output(self.model_output_max_tokens());
        let raw_text = String::from_utf8_lossy(&self.raw_output).to_string();
        let truncated = output != raw_text;
        serde_json::json!({
            "ok": false,
            "kind": "command_failed",
            "message": match self.exit_code {
                Some(code) => format!("command exited with code {code}"),
                None => "command failed".to_string(),
            },
            "tool": "exec_command",
            "exit_code": self.exit_code,
            "cwd": self.cwd.as_deref().unwrap_or(""),
            "stdout": output,
            "stderr": "",
            "duration_ms": self.wall_time.as_millis(),
            "truncated": truncated,
        })
        .to_string()
    }
}

fn function_tool_response(
    call_id: &str,
    payload: &ToolPayload,
    body: Vec<FunctionCallOutputContentItem>,
    success: Option<bool>,
) -> ResponseInputItem {
    let body = match body.as_slice() {
        [FunctionCallOutputContentItem::InputText { text }] => {
            FunctionCallOutputBody::Text(text.clone())
        }
        _ => FunctionCallOutputBody::ContentItems(body),
    };

    if matches!(payload, ToolPayload::Custom { .. }) {
        return ResponseInputItem::CustomToolCallOutput {
            call_id: call_id.to_string(),
            name: None,
            output: FunctionCallOutputPayload { body, success },
        };
    }

    ResponseInputItem::FunctionCallOutput {
        call_id: call_id.to_string(),
        output: FunctionCallOutputPayload { body, success },
    }
}

fn telemetry_preview(content: &str) -> String {
    let truncated_slice = take_bytes_at_char_boundary(content, TELEMETRY_PREVIEW_MAX_BYTES);
    let truncated_by_bytes = truncated_slice.len() < content.len();

    let mut preview = String::new();
    let mut lines_iter = truncated_slice.lines();
    for idx in 0..TELEMETRY_PREVIEW_MAX_LINES {
        match lines_iter.next() {
            Some(line) => {
                if idx > 0 {
                    preview.push('\n');
                }
                preview.push_str(line);
            }
            None => break,
        }
    }
    let truncated_by_lines = lines_iter.next().is_some();

    if !truncated_by_bytes && !truncated_by_lines {
        return content.to_string();
    }

    if preview.len() < truncated_slice.len()
        && truncated_slice
            .as_bytes()
            .get(preview.len())
            .is_some_and(|byte| *byte == b'\n')
    {
        preview.push('\n');
    }

    if !preview.is_empty() && !preview.ends_with('\n') {
        preview.push('\n');
    }
    preview.push_str(TELEMETRY_PREVIEW_TRUNCATION_NOTICE);

    preview
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
