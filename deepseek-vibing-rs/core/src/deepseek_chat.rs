use std::time::Duration;

use deepseek_vibing_api::Provider as ApiProvider;
use deepseek_vibing_api::SharedAuthProvider;
use deepseek_vibing_auth::default_client::build_reqwest_client;
use deepseek_vibing_protocol::error::ConnectionFailedError;
use deepseek_vibing_protocol::error::DeepSeekVibingErr;
use deepseek_vibing_protocol::error::Result;
use deepseek_vibing_protocol::error::UnexpectedResponseError;
use deepseek_vibing_protocol::models::ContentItem;
use deepseek_vibing_protocol::models::FunctionCallOutputPayload;
use deepseek_vibing_protocol::models::MessagePhase;
use deepseek_vibing_protocol::models::ReasoningItemContent;
use deepseek_vibing_protocol::models::ReasoningItemReasoningSummary;
use deepseek_vibing_protocol::models::ResponseItem;
use deepseek_vibing_protocol::openai_models::ModelInfo;
use deepseek_vibing_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use deepseek_vibing_protocol::protocol::TokenUsage;
use deepseek_vibing_tools::create_tools_json_for_deepseek_chat;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;

const CHAT_COMPLETIONS_PATH: &str = "chat/completions";
const DEEPSEEK_TOOL_CALL_PREFIX: &str = "deepseek_tool_call_";
const DEEPSEEK_MESSAGE_ID: &str = "deepseek_message";
const DEEPSEEK_REASONING_ID: &str = "deepseek_reasoning";

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct DeepSeekChatRequest {
    model: String,
    messages: Vec<DeepSeekChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<ReasoningEffortConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<DeepSeekResponseFormat>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DeepSeekChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<DeepSeekChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct DeepSeekChatToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: DeepSeekChatFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct DeepSeekChatFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize, PartialEq)]
struct DeepSeekResponseFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChatChunk {
    id: Option<String>,
    #[serde(default)]
    choices: Vec<DeepSeekChatChoice>,
    #[serde(default)]
    usage: Option<DeepSeekUsage>,
    #[serde(default)]
    error: Option<DeepSeekError>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChatChoice {
    #[serde(default)]
    delta: DeepSeekChatDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DeepSeekChatDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<DeepSeekToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekToolCallDelta {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    tool_type: Option<String>,
    function: Option<DeepSeekFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekError {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct DeepSeekUsage {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    prompt_cache_hit_tokens: Option<i64>,
    completion_tokens_details: Option<DeepSeekCompletionTokensDetails>,
}

#[derive(Debug, Default, Deserialize)]
struct DeepSeekCompletionTokensDetails {
    reasoning_tokens: Option<i64>,
}

#[derive(Debug, Default)]
struct PendingToolCall {
    id: Option<String>,
    tool_type: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Default)]
struct StreamAccumulator {
    response_id: Option<String>,
    content: String,
    reasoning: String,
    text_started: bool,
    reasoning_started: bool,
    tool_calls: Vec<PendingToolCall>,
    usage: Option<TokenUsage>,
    finish_reason: Option<String>,
}

pub(crate) fn build_deepseek_chat_request(
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
    strict_tools: bool,
) -> Result<DeepSeekChatRequest> {
    let tools = create_tools_json_for_deepseek_chat(&prompt.tools, strict_tools)?;
    Ok(DeepSeekChatRequest {
        model: model_info.slug.clone(),
        messages: build_deepseek_messages(prompt),
        stream: true,
        tool_choice: (!tools.is_empty()).then(|| "auto".to_string()),
        parallel_tool_calls: (!tools.is_empty()).then_some(prompt.parallel_tool_calls),
        tools,
        reasoning_effort: deepseek_reasoning_effort(effort),
        response_format: prompt
            .output_schema
            .as_ref()
            .map(|_| DeepSeekResponseFormat {
                format_type: "json_object",
            }),
    })
}

pub(crate) async fn stream_deepseek_chat(
    api_provider: ApiProvider,
    api_auth: SharedAuthProvider,
    prompt: &Prompt,
    model_info: &ModelInfo,
    effort: Option<ReasoningEffortConfig>,
    strict_tools: bool,
    idle_timeout: Duration,
) -> Result<ResponseStream> {
    let request = build_deepseek_chat_request(prompt, model_info, effort, strict_tools)?;
    let mut headers = api_provider.headers.clone();
    api_auth.add_auth_headers(&mut headers);
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );

    let url = api_provider.url_for_path(CHAT_COMPLETIONS_PATH);
    let response = build_reqwest_client()
        .post(url.clone())
        .headers(headers)
        .json(&request)
        .send()
        .await
        .map_err(|source| DeepSeekVibingErr::ConnectionFailed(ConnectionFailedError { source }))?;

    if !response.status().is_success() {
        return Err(unexpected_status(response, Some(url)).await);
    }

    Ok(spawn_deepseek_stream(response, idle_timeout))
}

fn build_deepseek_messages(prompt: &Prompt) -> Vec<DeepSeekChatMessage> {
    let mut messages = Vec::new();
    let instructions = prompt.base_instructions.text.trim();
    if !instructions.is_empty() {
        messages.push(DeepSeekChatMessage {
            role: "system".to_string(),
            content: Some(instructions.to_string()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    let mut pending_reasoning: Option<String> = None;
    for item in prompt.get_formatted_input() {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let content = content_items_to_text(&content);
                let reasoning_content = if role == "assistant" {
                    pending_reasoning.take()
                } else {
                    None
                };
                if !content.is_empty() || reasoning_content.is_some() {
                    messages.push(DeepSeekChatMessage {
                        role: deepseek_message_role(&role).to_string(),
                        content: Some(content),
                        reasoning_content,
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
            ResponseItem::Reasoning { content, .. } => {
                pending_reasoning = reasoning_content_to_text(content.as_deref());
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                messages.push(DeepSeekChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    reasoning_content: pending_reasoning.take(),
                    tool_calls: Some(vec![DeepSeekChatToolCall {
                        id: call_id,
                        tool_type: "function".to_string(),
                        function: DeepSeekChatFunctionCall { name, arguments },
                    }]),
                    tool_call_id: None,
                });
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                messages.push(tool_message(call_id, &output));
            }
            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                ..
            } => {
                messages.push(DeepSeekChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    reasoning_content: pending_reasoning.take(),
                    tool_calls: Some(vec![DeepSeekChatToolCall {
                        id: call_id,
                        tool_type: "function".to_string(),
                        function: DeepSeekChatFunctionCall {
                            name,
                            arguments: input,
                        },
                    }]),
                    tool_call_id: None,
                });
            }
            ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => {
                messages.push(tool_message(call_id, &output));
            }
            ResponseItem::LocalShellCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::CompactionTrigger
            | ResponseItem::ContextCompaction { .. }
            | ResponseItem::Other => {}
        }
    }

    messages
}

fn deepseek_message_role(role: &str) -> &str {
    match role {
        "system" | "user" | "assistant" | "tool" | "latest_reminder" => role,
        // DeepSeek's chat endpoint rejects OpenAI/DeepSeekVibing `developer` messages.
        // Keep their ordering and content by folding them into system messages.
        "developer" => "system",
        _ => "system",
    }
}

fn tool_message(call_id: String, output: &FunctionCallOutputPayload) -> DeepSeekChatMessage {
    DeepSeekChatMessage {
        role: "tool".to_string(),
        content: Some(output.to_string()),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: Some(call_id),
    }
}

fn content_items_to_text(items: &[ContentItem]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.to_string())
            }
            ContentItem::InputImage { .. } => Some(
                "[Image input omitted; use Vision Bridge image_observation instead]".to_string(),
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn reasoning_content_to_text(items: Option<&[ReasoningItemContent]>) -> Option<String> {
    let text = items?
        .iter()
        .map(|item| match item {
            ReasoningItemContent::ReasoningText { text } | ReasoningItemContent::Text { text } => {
                text.as_str()
            }
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn deepseek_reasoning_effort(
    effort: Option<ReasoningEffortConfig>,
) -> Option<ReasoningEffortConfig> {
    match effort {
        Some(ReasoningEffortConfig::None) => None,
        Some(ReasoningEffortConfig::Minimal) => Some(ReasoningEffortConfig::Low),
        Some(ReasoningEffortConfig::XHigh) => Some(ReasoningEffortConfig::High),
        other => other,
    }
}

fn spawn_deepseek_stream(response: reqwest::Response, idle_timeout: Duration) -> ResponseStream {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
    let consumer_dropped = CancellationToken::new();
    let stream_cancel = consumer_dropped.clone();

    tokio::spawn(async move {
        let _ = tx_event.send(Ok(ResponseEvent::Created)).await;
        let mut accumulator = StreamAccumulator::default();
        let mut stream = response.bytes_stream().eventsource();
        loop {
            if stream_cancel.is_cancelled() {
                break;
            }
            let next = match timeout(idle_timeout, stream.next()).await {
                Ok(next) => next,
                Err(_) => {
                    let _ = tx_event
                        .send(Err(DeepSeekVibingErr::Stream(
                            "DeepSeek stream idle timeout".to_string(),
                            None,
                        )))
                        .await;
                    break;
                }
            };

            let Some(event) = next else {
                break;
            };

            match event {
                Ok(event) => {
                    if event.data.trim() == "[DONE]" {
                        break;
                    }
                    match serde_json::from_str::<DeepSeekChatChunk>(&event.data) {
                        Ok(chunk) => {
                            if let Err(err) = handle_chunk(chunk, &mut accumulator, &tx_event).await
                            {
                                let _ = tx_event.send(Err(err)).await;
                                return;
                            }
                        }
                        Err(err) => {
                            let _ = tx_event
                                .send(Err(DeepSeekVibingErr::Stream(
                                    format!("failed to parse DeepSeek stream event: {err}"),
                                    None,
                                )))
                                .await;
                            return;
                        }
                    }
                }
                Err(err) => {
                    let _ = tx_event
                        .send(Err(DeepSeekVibingErr::Stream(
                            format!("DeepSeek stream error: {err}"),
                            None,
                        )))
                        .await;
                    return;
                }
            }
        }

        finish_stream(accumulator, tx_event).await;
    });

    ResponseStream {
        rx_event,
        consumer_dropped,
    }
}

async fn handle_chunk(
    chunk: DeepSeekChatChunk,
    accumulator: &mut StreamAccumulator,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
) -> Result<()> {
    if let Some(error) = chunk.error {
        return Err(DeepSeekVibingErr::InvalidRequest(error.to_string()));
    }
    if accumulator.response_id.is_none() {
        accumulator.response_id = chunk.id;
    }
    if let Some(usage) = chunk.usage {
        accumulator.usage = Some(usage.into());
    }

    for choice in chunk.choices {
        if let Some(finish_reason) = choice.finish_reason {
            accumulator.finish_reason = Some(finish_reason);
        }

        if let Some(reasoning_delta) = choice.delta.reasoning_content
            && !reasoning_delta.is_empty()
        {
            if !accumulator.reasoning_started {
                accumulator.reasoning_started = true;
                tx_event
                    .send(Ok(ResponseEvent::OutputItemAdded(reasoning_item(
                        String::new(),
                    ))))
                    .await
                    .ok();
            }
            accumulator.reasoning.push_str(&reasoning_delta);
            tx_event
                .send(Ok(ResponseEvent::ReasoningContentDelta {
                    delta: reasoning_delta,
                    content_index: 0,
                }))
                .await
                .ok();
        }

        if let Some(content_delta) = choice.delta.content
            && !content_delta.is_empty()
        {
            if !accumulator.text_started {
                accumulator.text_started = true;
                tx_event
                    .send(Ok(ResponseEvent::OutputItemAdded(message_item(
                        String::new(),
                    ))))
                    .await
                    .ok();
            }
            accumulator.content.push_str(&content_delta);
            tx_event
                .send(Ok(ResponseEvent::OutputTextDelta(content_delta)))
                .await
                .ok();
        }

        if let Some(tool_call_deltas) = choice.delta.tool_calls {
            for delta in tool_call_deltas {
                let pending = ensure_tool_call(accumulator, delta.index);
                if let Some(id) = delta.id {
                    pending.id = Some(id);
                }
                if let Some(tool_type) = delta.tool_type {
                    pending.tool_type = Some(tool_type);
                }
                if let Some(function) = delta.function {
                    if let Some(name) = function.name {
                        pending.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        pending.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn finish_stream(
    accumulator: StreamAccumulator,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
) {
    if accumulator.reasoning_started {
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemDone(reasoning_item(
                accumulator.reasoning,
            ))))
            .await;
    }
    if accumulator.text_started {
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemDone(message_item(
                accumulator.content,
            ))))
            .await;
    }

    for (index, tool_call) in accumulator.tool_calls.into_iter().enumerate() {
        let item = ResponseItem::FunctionCall {
            id: None,
            name: tool_call.name.unwrap_or_else(|| "unknown_tool".to_string()),
            namespace: None,
            arguments: tool_call.arguments,
            call_id: tool_call
                .id
                .unwrap_or_else(|| format!("{DEEPSEEK_TOOL_CALL_PREFIX}{index}")),
        };
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item.clone())))
            .await;
        let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
    }

    let _ = tx_event
        .send(Ok(ResponseEvent::Completed {
            response_id: accumulator
                .response_id
                .unwrap_or_else(|| "deepseek-chat".to_string()),
            token_usage: accumulator.usage,
            end_turn: Some(!matches!(
                accumulator.finish_reason.as_deref(),
                Some("tool_calls")
            )),
        }))
        .await;
}

fn ensure_tool_call(accumulator: &mut StreamAccumulator, index: usize) -> &mut PendingToolCall {
    while accumulator.tool_calls.len() <= index {
        accumulator.tool_calls.push(PendingToolCall::default());
    }
    &mut accumulator.tool_calls[index]
}

fn reasoning_item(text: String) -> ResponseItem {
    ResponseItem::Reasoning {
        id: DEEPSEEK_REASONING_ID.to_string(),
        summary: Vec::<ReasoningItemReasoningSummary>::new(),
        content: Some(vec![ReasoningItemContent::ReasoningText { text }]),
        encrypted_content: None,
    }
}

fn message_item(text: String) -> ResponseItem {
    ResponseItem::Message {
        id: Some(DEEPSEEK_MESSAGE_ID.to_string()),
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        phase: Some(MessagePhase::FinalAnswer),
    }
}

async fn unexpected_status(response: reqwest::Response, url: Option<String>) -> DeepSeekVibingErr {
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.text().await.unwrap_or_default();
    DeepSeekVibingErr::UnexpectedStatus(UnexpectedResponseError {
        status,
        body,
        url,
        cf_ray: header_to_string(&headers, "cf-ray"),
        request_id: header_to_string(&headers, "x-request-id"),
        identity_authorization_error: None,
        identity_error_code: None,
    })
}

fn header_to_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}

impl std::fmt::Display for DeepSeekError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = self.message.as_deref().unwrap_or("DeepSeek API error");
        match (&self.error_type, &self.code) {
            (Some(error_type), Some(code)) => write!(f, "{message} ({error_type}, code: {code})"),
            (Some(error_type), None) => write!(f, "{message} ({error_type})"),
            (None, Some(code)) => write!(f, "{message} (code: {code})"),
            (None, None) => f.write_str(message),
        }
    }
}

impl From<DeepSeekUsage> for TokenUsage {
    fn from(value: DeepSeekUsage) -> Self {
        let input_tokens = value.prompt_tokens.unwrap_or(0);
        let output_tokens = value.completion_tokens.unwrap_or(0);
        TokenUsage {
            input_tokens,
            cached_input_tokens: value.prompt_cache_hit_tokens.unwrap_or(0),
            output_tokens,
            reasoning_output_tokens: value
                .completion_tokens_details
                .and_then(|details| details.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: value
                .total_tokens
                .unwrap_or_else(|| input_tokens.saturating_add(output_tokens)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tools::handlers::apply_patch_spec::create_apply_patch_freeform_tool;
    use crate::tools::handlers::apply_patch_spec::create_apply_patch_function_tool;
    use deepseek_vibing_protocol::models::BaseInstructions;
    use deepseek_vibing_protocol::models::FunctionCallOutputPayload;
    use deepseek_vibing_protocol::openai_models::ReasoningEffort;
    use deepseek_vibing_tools::ReadFileToolOptions;
    use deepseek_vibing_tools::ResponsesApiTool;
    use deepseek_vibing_tools::ToolSpec;
    use deepseek_vibing_tools::create_read_file_tool;
    use tokio::sync::mpsc;

    use super::*;

    fn model_info() -> ModelInfo {
        serde_json::from_value(serde_json::json!({
            "slug": "deepseek-v4-flash",
            "display_name": "DeepSeek V4 Flash",
            "description": null,
            "default_reasoning_level": "medium",
            "supported_reasoning_levels": [],
            "shell_type": "shell_command",
            "visibility": "list",
            "supported_in_api": true,
            "priority": 0,
            "upgrade": null,
            "base_instructions": "base",
            "supports_reasoning_summaries": false,
            "support_verbosity": false,
            "default_verbosity": null,
            "apply_patch_tool_type": null,
            "truncation_policy": {"mode": "bytes", "limit": 1000000},
            "supports_parallel_tool_calls": true,
            "supports_image_detail_original": false,
            "context_window": 1000000,
            "max_context_window": 1000000,
            "experimental_supported_tools": [],
            "input_modalities": ["text"]
        }))
        .expect("valid model metadata")
    }

    #[test]
    fn request_maps_history_reasoning_tool_call_and_tool_output() {
        let prompt = Prompt {
            base_instructions: BaseInstructions {
                text: "You are DeepSeek Vibing.".to_string(),
            },
            input: vec![
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "run a command".to_string(),
                    }],
                    phase: None,
                },
                ResponseItem::Reasoning {
                    id: "rs".to_string(),
                    summary: vec![],
                    content: Some(vec![ReasoningItemContent::ReasoningText {
                        text: "Need shell.".to_string(),
                    }]),
                    encrypted_content: None,
                },
                ResponseItem::FunctionCall {
                    id: None,
                    name: "exec_command".to_string(),
                    namespace: None,
                    arguments: "{\"cmd\":\"pwd\"}".to_string(),
                    call_id: "call_1".to_string(),
                },
                ResponseItem::FunctionCallOutput {
                    call_id: "call_1".to_string(),
                    output: FunctionCallOutputPayload::from_text("/tmp".to_string()),
                },
            ],
            tools: vec![],
            parallel_tool_calls: false,
            personality: None,
            output_schema: None,
            output_schema_strict: true,
        };

        let request = build_deepseek_chat_request(
            &prompt,
            &model_info(),
            Some(ReasoningEffort::Medium),
            false,
        )
        .expect("request should build");

        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[1].role, "user");
        let assistant = &request.messages[2];
        assert_eq!(assistant.role, "assistant");
        assert_eq!(assistant.reasoning_content.as_deref(), Some("Need shell."));
        assert_eq!(
            assistant.tool_calls.as_ref().unwrap()[0].function.name,
            "exec_command"
        );
        assert_eq!(request.messages[3].role, "tool");
        assert_eq!(request.reasoning_effort, Some(ReasoningEffort::Medium));
    }

    #[test]
    fn request_maps_developer_messages_to_system_for_deepseek() {
        let prompt = Prompt {
            base_instructions: BaseInstructions {
                text: "base".to_string(),
            },
            input: vec![ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "repo context".to_string(),
                }],
                phase: None,
            }],
            ..Prompt::default()
        };

        let request =
            build_deepseek_chat_request(&prompt, &model_info(), None, false).expect("request");
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[1].role, "system");
        assert_eq!(request.messages[1].content.as_deref(), Some("repo context"));
    }

    #[test]
    fn request_filters_hosted_tools_and_serializes_function_tools() {
        let prompt = Prompt {
            tools: vec![
                ToolSpec::Function(ResponsesApiTool {
                    name: "exec_command".to_string(),
                    description: "Run a command".to_string(),
                    strict: false,
                    defer_loading: None,
                    parameters: serde_json::from_value(serde_json::json!({
                        "type": "object",
                        "properties": {}
                    }))
                    .expect("schema"),
                    output_schema: None,
                }),
                ToolSpec::WebSearch {
                    external_web_access: None,
                    filters: None,
                    user_location: None,
                    search_context_size: None,
                    search_content_types: None,
                },
            ],
            ..Prompt::default()
        };

        let request = build_deepseek_chat_request(&prompt, &model_info(), None, true)
            .expect("request should build");
        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0]["type"], "function");
        assert_eq!(request.tools[0]["function"]["strict"], true);
    }

    #[test]
    fn request_serializes_deepseek_apply_patch_function_tool() {
        let prompt = Prompt {
            tools: vec![
                create_apply_patch_function_tool(/*include_environment_id*/ false),
                create_apply_patch_freeform_tool(/*include_environment_id*/ false),
            ],
            ..Prompt::default()
        };

        let request = build_deepseek_chat_request(&prompt, &model_info(), None, true)
            .expect("request should build");

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0]["type"], "function");
        assert_eq!(request.tools[0]["function"]["name"], "apply_patch");
        assert_eq!(request.tools[0]["function"]["strict"], true);
        let parameters = &request.tools[0]["function"]["parameters"];
        assert_eq!(parameters["required"], serde_json::json!(["patch"]));
        assert_eq!(parameters["additionalProperties"], false);
        assert!(parameters.pointer("/properties/patch").is_some());
    }

    #[test]
    fn request_serializes_deepseek_read_file_function_tool() {
        let prompt = Prompt {
            tools: vec![create_read_file_tool(ReadFileToolOptions {
                include_environment_id: false,
            })],
            ..Prompt::default()
        };

        let request = build_deepseek_chat_request(&prompt, &model_info(), None, true)
            .expect("request should build");

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0]["type"], "function");
        assert_eq!(request.tools[0]["function"]["name"], "read_file");
        assert_eq!(request.tools[0]["function"]["strict"], true);
        let parameters = &request.tools[0]["function"]["parameters"];
        assert_eq!(parameters["required"], serde_json::json!(["path"]));
        assert_eq!(parameters["additionalProperties"], false);
        assert!(parameters.pointer("/properties/path").is_some());
        assert!(parameters.pointer("/properties/start_line").is_some());
        assert!(parameters.pointer("/properties/limit").is_some());
    }

    #[test]
    fn request_image_fallback_does_not_include_raw_image_url() {
        let raw_image_url = "data:image/png;base64,RAW_IMAGE_BYTES";
        let prompt = Prompt {
            input: vec![ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputImage {
                    image_url: raw_image_url.to_string(),
                    detail: None,
                }],
                phase: None,
            }],
            ..Prompt::default()
        };

        let request =
            build_deepseek_chat_request(&prompt, &model_info(), None, false).expect("request");
        let serialized = serde_json::to_string(&request).expect("serialize request");
        assert!(!serialized.contains(raw_image_url));
        assert!(serialized.contains("Image input omitted"));
    }

    #[test]
    fn usage_maps_cache_and_reasoning_tokens() {
        let usage: TokenUsage = serde_json::from_value::<DeepSeekUsage>(serde_json::json!({
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30,
            "prompt_cache_hit_tokens": 7,
            "completion_tokens_details": {"reasoning_tokens": 11}
        }))
        .expect("usage")
        .into();

        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.cached_input_tokens, 7);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.reasoning_output_tokens, 11);
        assert_eq!(usage.total_tokens, 30);
    }

    fn chunk(value: serde_json::Value) -> DeepSeekChatChunk {
        serde_json::from_value(value).expect("valid DeepSeek stream chunk")
    }

    #[tokio::test]
    async fn stream_maps_reasoning_text_tool_call_usage_and_replays_ledger() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut accumulator = StreamAccumulator::default();

        handle_chunk(
            chunk(serde_json::json!({
                "id": "chatcmpl_fixture",
                "choices": [{
                    "delta": {"reasoning_content": "Need to inspect. "}
                }]
            })),
            &mut accumulator,
            &tx,
        )
        .await
        .expect("reasoning chunk");
        handle_chunk(
            chunk(serde_json::json!({
                "choices": [{
                    "delta": {
                        "reasoning_content": "Call a tool.",
                        "content": "I will inspect it."
                    }
                }]
            })),
            &mut accumulator,
            &tx,
        )
        .await
        .expect("mixed chunk");
        handle_chunk(
            chunk(serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "exec_command",
                                "arguments": "{\"cmd\":"
                            }
                        }]
                    }
                }]
            })),
            &mut accumulator,
            &tx,
        )
        .await
        .expect("tool call start");
        handle_chunk(
            chunk(serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "function": {
                                "arguments": "\"pwd\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 4,
                    "prompt_cache_hit_tokens": 6,
                    "completion_tokens_details": {"reasoning_tokens": 3}
                }
            })),
            &mut accumulator,
            &tx,
        )
        .await
        .expect("tool call finish");

        finish_stream(accumulator, tx).await;

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event.expect("stream event"));
        }

        assert!(matches!(
            events.first(),
            Some(ResponseEvent::OutputItemAdded(
                ResponseItem::Reasoning { .. }
            ))
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            ResponseEvent::ReasoningContentDelta { delta, .. }
                if delta == "Need to inspect. "
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ResponseEvent::OutputTextDelta(delta) if delta == "I will inspect it."
        )));

        let reasoning = events
            .iter()
            .find_map(|event| match event {
                ResponseEvent::OutputItemDone(ResponseItem::Reasoning { content, .. }) => {
                    content.clone()
                }
                _ => None,
            })
            .expect("reasoning done item");
        assert_eq!(
            reasoning,
            vec![ReasoningItemContent::ReasoningText {
                text: "Need to inspect. Call a tool.".to_string()
            }]
        );

        let tool_call = events
            .iter()
            .find_map(|event| match event {
                ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    ..
                }) => Some((name.clone(), arguments.clone(), call_id.clone())),
                _ => None,
            })
            .expect("function call item");
        assert_eq!(tool_call.0, "exec_command");
        assert_eq!(tool_call.1, "{\"cmd\":\"pwd\"}");
        assert_eq!(tool_call.2, "call_1");

        let completed = events
            .iter()
            .find_map(|event| match event {
                ResponseEvent::Completed {
                    response_id,
                    token_usage,
                    end_turn,
                } => Some((response_id, token_usage.as_ref(), end_turn)),
                _ => None,
            })
            .expect("completed event");
        assert_eq!(completed.0, "chatcmpl_fixture");
        assert_eq!(completed.2, &Some(false));
        let usage = completed.1.expect("usage");
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.cached_input_tokens, 6);
        assert_eq!(usage.reasoning_output_tokens, 3);

        let request = build_deepseek_chat_request(
            &Prompt {
                base_instructions: BaseInstructions {
                    text: String::new(),
                },
                input: vec![
                    ResponseItem::Reasoning {
                        id: "rs".to_string(),
                        summary: vec![],
                        content: Some(reasoning),
                        encrypted_content: None,
                    },
                    ResponseItem::FunctionCall {
                        id: None,
                        name: tool_call.0,
                        namespace: None,
                        arguments: tool_call.1,
                        call_id: tool_call.2.clone(),
                    },
                    ResponseItem::FunctionCallOutput {
                        call_id: tool_call.2,
                        output: FunctionCallOutputPayload::from_text("/repo".to_string()),
                    },
                ],
                ..Prompt::default()
            },
            &model_info(),
            None,
            false,
        )
        .expect("request should build");

        let assistant = &request.messages[0];
        assert_eq!(assistant.role, "assistant");
        assert_eq!(
            assistant.reasoning_content.as_deref(),
            Some("Need to inspect. Call a tool.")
        );
        assert!(assistant.tool_calls.is_some());
        assert_eq!(request.messages[1].role, "tool");
    }
}
