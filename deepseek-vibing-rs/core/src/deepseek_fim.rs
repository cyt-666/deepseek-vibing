use std::time::Duration;

use deepseek_vibing_auth::default_client::build_reqwest_client;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::config::DEEPSEEK_FIM_MAX_TOKENS_LIMIT;
use crate::config::DeepSeekFimConfig;

const COMPLETIONS_PATH: &str = "completions";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FimCompletionRequest {
    pub prefix: String,
    pub suffix: Option<String>,
    pub model: String,
    pub max_tokens: u16,
    pub temperature: f32,
    pub stop: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FimCompletionOutput {
    pub text: String,
    pub model: String,
    pub finish_reason: Option<String>,
    pub usage: Option<FimUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FimUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub prompt_cache_hit_tokens: u64,
    pub prompt_cache_miss_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Error)]
pub enum FimCompletionError {
    #[error("DeepSeek FIM is disabled in config")]
    Disabled,
    #[error(
        "missing API key: set `[auth].deepseek_api_key` in DeepSeek Vibing config.toml or set {env_key}"
    )]
    MissingApiKey { env_key: String },
    #[error("max_tokens must be between 1 and {DEEPSEEK_FIM_MAX_TOKENS_LIMIT}; got {value}")]
    InvalidMaxTokens { value: u16 },
    #[error("failed to send FIM request: {0}")]
    Request(#[from] reqwest::Error),
    #[error("failed to read FIM response: {0}")]
    ResponseRead(reqwest::Error),
    #[error("DeepSeek FIM provider returned HTTP {status}: {body}")]
    ProviderStatus { status: u16, body: String },
    #[error("failed to parse FIM response JSON: {0}")]
    ResponseJson(serde_json::Error),
    #[error("DeepSeek FIM response did not contain a completion choice")]
    EmptyChoices,
}

#[derive(Debug, Serialize, PartialEq)]
struct DeepSeekFimWireRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suffix: Option<String>,
    max_tokens: u16,
    temperature: f32,
    stream: bool,
    echo: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekFimWireResponse {
    #[serde(default)]
    choices: Vec<DeepSeekFimChoice>,
    model: Option<String>,
    usage: Option<FimUsage>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekFimChoice {
    text: String,
    finish_reason: Option<String>,
}

pub async fn complete_fim(
    config: &DeepSeekFimConfig,
    request: FimCompletionRequest,
) -> Result<FimCompletionOutput, FimCompletionError> {
    complete_fim_with_auth(config, None, request).await
}

pub async fn complete_fim_with_auth(
    config: &DeepSeekFimConfig,
    configured_api_key: Option<&str>,
    request: FimCompletionRequest,
) -> Result<FimCompletionOutput, FimCompletionError> {
    if !config.enabled {
        return Err(FimCompletionError::Disabled);
    }
    validate_max_tokens(request.max_tokens)?;

    let api_key = fim_api_key_for_request(config, configured_api_key)?;
    let wire_request = build_fim_wire_request(request);
    let url = fim_completions_url(&config.base_url);
    let response = build_reqwest_client()
        .post(&url)
        .bearer_auth(&api_key)
        .timeout(Duration::from_millis(config.timeout_ms))
        .json(&wire_request)
        .send()
        .await?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(FimCompletionError::ResponseRead)?;
    if !status.is_success() {
        return Err(provider_status_error(
            status.as_u16(),
            sanitize_provider_body(&text, &api_key),
        ));
    }

    parse_fim_completion_output(&text)
}

fn fim_api_key_for_request(
    config: &DeepSeekFimConfig,
    configured_api_key: Option<&str>,
) -> Result<String, FimCompletionError> {
    if let Some(api_key) = configured_api_key.filter(|value| !value.trim().is_empty()) {
        return Ok(api_key.trim().to_string());
    }

    std::env::var(&config.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| FimCompletionError::MissingApiKey {
            env_key: config.api_key_env.clone(),
        })
}

fn validate_max_tokens(max_tokens: u16) -> Result<(), FimCompletionError> {
    if max_tokens == 0 || max_tokens > DEEPSEEK_FIM_MAX_TOKENS_LIMIT {
        return Err(FimCompletionError::InvalidMaxTokens { value: max_tokens });
    }
    Ok(())
}

fn build_fim_wire_request(request: FimCompletionRequest) -> DeepSeekFimWireRequest {
    DeepSeekFimWireRequest {
        model: request.model,
        prompt: request.prefix,
        suffix: request.suffix,
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        stream: false,
        echo: false,
        stop: request.stop,
    }
}

fn parse_fim_completion_output(text: &str) -> Result<FimCompletionOutput, FimCompletionError> {
    let response: DeepSeekFimWireResponse =
        serde_json::from_str(text).map_err(FimCompletionError::ResponseJson)?;
    let Some(choice) = response.choices.into_iter().next() else {
        return Err(FimCompletionError::EmptyChoices);
    };

    Ok(FimCompletionOutput {
        text: choice.text,
        model: response.model.unwrap_or_default(),
        finish_reason: choice.finish_reason,
        usage: response.usage,
    })
}

fn provider_status_error(status: u16, body: String) -> FimCompletionError {
    FimCompletionError::ProviderStatus { status, body }
}

fn sanitize_provider_body(body: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        body.to_string()
    } else {
        body.replace(api_key, "[REDACTED]")
    }
}

fn fim_completions_url(base_url: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), COMPLETIONS_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn request() -> FimCompletionRequest {
        FimCompletionRequest {
            prefix: "fn main() {".to_string(),
            suffix: Some("}".to_string()),
            model: "deepseek-v4-pro".to_string(),
            max_tokens: 128,
            temperature: 0.2,
            stop: Some(vec!["\n\n".to_string()]),
        }
    }

    #[test]
    fn request_json_uses_completion_fields_and_excludes_agent_fields() {
        let wire = build_fim_wire_request(request());
        let value = serde_json::to_value(wire).expect("serialize request");

        assert_eq!(value["model"], "deepseek-v4-pro");
        assert_eq!(value["prompt"], "fn main() {");
        assert_eq!(value["suffix"], "}");
        assert_eq!(value["max_tokens"], 128);
        assert_eq!(value["stream"], false);
        assert_eq!(value["echo"], false);
        assert!(value.get("messages").is_none());
        assert!(value.get("tools").is_none());
        assert!(value.get("reasoning_effort").is_none());
        assert!(!value.to_string().contains("data:image"));
    }

    #[test]
    fn parses_success_response_with_usage() {
        let output = parse_fim_completion_output(
            r#"{
              "choices":[{"text":"\n    println!(\"hi\");","finish_reason":"stop"}],
              "model":"deepseek-v4-pro",
              "usage":{
                "prompt_tokens":10,
                "completion_tokens":4,
                "prompt_cache_hit_tokens":6,
                "prompt_cache_miss_tokens":4,
                "total_tokens":14
              }
            }"#,
        )
        .expect("parse response");

        assert_eq!(output.text, "\n    println!(\"hi\");");
        assert_eq!(output.model, "deepseek-v4-pro");
        assert_eq!(output.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            output.usage,
            Some(FimUsage {
                prompt_tokens: 10,
                completion_tokens: 4,
                prompt_cache_hit_tokens: 6,
                prompt_cache_miss_tokens: 4,
                total_tokens: 14,
            })
        );
    }

    #[test]
    fn empty_choices_is_error() {
        let err = parse_fim_completion_output(r#"{"choices":[],"model":"deepseek-v4-pro"}"#)
            .expect_err("empty choices should fail");
        assert!(matches!(err, FimCompletionError::EmptyChoices));
    }

    #[test]
    fn invalid_max_tokens_is_error() {
        assert!(matches!(
            validate_max_tokens(0),
            Err(FimCompletionError::InvalidMaxTokens { value: 0 })
        ));
        assert!(matches!(
            validate_max_tokens(DEEPSEEK_FIM_MAX_TOKENS_LIMIT + 1),
            Err(FimCompletionError::InvalidMaxTokens { .. })
        ));
    }

    #[test]
    fn missing_api_key_is_clear() {
        let config = DeepSeekFimConfig {
            api_key_env: "DEEPSEEK_FIM_TEST_MISSING_KEY".to_string(),
            ..DeepSeekFimConfig::default()
        };
        let err = futures::executor::block_on(complete_fim(&config, request()))
            .expect_err("missing key should fail before network");

        assert_eq!(
            err.to_string(),
            "missing API key: set `[auth].deepseek_api_key` in DeepSeek Vibing config.toml or set DEEPSEEK_FIM_TEST_MISSING_KEY"
        );
    }

    #[test]
    fn configured_api_key_is_used_before_env_fallback() {
        let config = DeepSeekFimConfig {
            api_key_env: "DEEPSEEK_FIM_TEST_MISSING_KEY".to_string(),
            ..DeepSeekFimConfig::default()
        };

        let api_key = fim_api_key_for_request(&config, Some("  sk-config-fim  "))
            .expect("configured key should resolve");

        assert_eq!(api_key, "sk-config-fim");
    }

    #[test]
    fn provider_status_error_redacts_api_key() {
        let body =
            sanitize_provider_body(r#"{"error":"bad key sk-secret-token"}"#, "sk-secret-token");
        let err = provider_status_error(401, body);

        assert!(err.to_string().contains("[REDACTED]"));
        assert!(!err.to_string().contains("sk-secret-token"));
    }

    #[test]
    fn fim_url_trims_trailing_slash() {
        assert_eq!(
            fim_completions_url("https://api.deepseek.com/beta/"),
            "https://api.deepseek.com/beta/completions"
        );
    }

    #[test]
    fn json_mode_output_shape_is_stable() {
        let output = FimCompletionOutput {
            text: "middle".to_string(),
            model: "deepseek-v4-pro".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: None,
        };
        let value: Value = serde_json::to_value(output).expect("serialize output");

        assert_eq!(value["text"], "middle");
        assert_eq!(value["model"], "deepseek-v4-pro");
        assert_eq!(value["finish_reason"], "stop");
    }
}
