use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use deepseek_vibing_auth::default_client::build_reqwest_client;
use deepseek_vibing_model_provider_info::DEEPSEEK_BETA_PROVIDER_ID;
use deepseek_vibing_model_provider_info::DEEPSEEK_PROVIDER_ID;
use deepseek_vibing_protocol::models::ImageDetail;
use deepseek_vibing_protocol::user_input::UserInput;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use tokio::fs;
use tracing::debug;

use crate::config::VisionConfig;
use crate::session::turn_context::TurnContext;

const VISION_PROVIDER_OPENAI_COMPATIBLE: &str = "openai_compatible";
const DEFAULT_AUTO_TASK_HINT: &str = "Describe this user-provided image for a coding agent. Focus on visible text, UI state, errors, code, diagrams, and details that matter for implementation or debugging.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ImageObservation {
    pub ok: bool,
    pub image_id: String,
    pub source: String,
    pub summary: String,
    #[serde(default)]
    pub visible_text: Vec<String>,
    #[serde(default)]
    pub ui_elements: Vec<String>,
    #[serde(default)]
    pub important_details: Vec<String>,
    #[serde(default)]
    pub uncertain_details: Vec<String>,
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
}

impl ImageObservation {
    fn success(
        image_id: String,
        source: String,
        config: &VisionConfig,
        mime_type: Option<String>,
        artifact_path: Option<PathBuf>,
        provider_output: ProviderObservation,
    ) -> Self {
        Self {
            ok: true,
            image_id,
            source,
            summary: provider_output.summary,
            visible_text: provider_output.visible_text,
            ui_elements: provider_output.ui_elements,
            important_details: provider_output.important_details,
            uncertain_details: provider_output.uncertain_details,
            cached: false,
            kind: None,
            message: None,
            provider: Some(config.provider.clone()),
            model: Some(config.model.clone()),
            mime_type,
            artifact_path: artifact_path.map(|path| path.display().to_string()),
        }
    }

    fn error(
        image_id: String,
        source: String,
        kind: impl Into<String>,
        message: impl Into<String>,
        config: &VisionConfig,
    ) -> Self {
        let message = message.into();
        Self {
            ok: false,
            image_id,
            source,
            summary: format!("Vision observation failed: {message}"),
            visible_text: Vec::new(),
            ui_elements: Vec::new(),
            important_details: Vec::new(),
            uncertain_details: Vec::new(),
            cached: false,
            kind: Some(kind.into()),
            message: Some(message),
            provider: Some(config.provider.clone()),
            model: Some(config.model.clone()),
            mime_type: None,
            artifact_path: None,
        }
    }

    pub(crate) fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|err| {
            json!({
                "ok": false,
                "image_id": self.image_id,
                "source": self.source,
                "summary": format!("failed to serialize image observation: {err}"),
                "visible_text": [],
                "ui_elements": [],
                "important_details": [],
                "uncertain_details": [],
                "cached": false,
                "kind": "serialization_error",
                "message": err.to_string(),
            })
            .to_string()
        })
    }

    pub(crate) fn to_context_fragment(&self) -> String {
        format!(
            "<image_observation>\n{}\n</image_observation>",
            self.to_json_string()
        )
    }
}

#[derive(Debug, Clone)]
struct PreparedImage {
    image_id: String,
    source: String,
    provider_image_url: String,
    bytes: Option<Vec<u8>>,
    mime_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ProviderObservation {
    summary: String,
    visible_text: Vec<String>,
    ui_elements: Vec<String>,
    important_details: Vec<String>,
    uncertain_details: Vec<String>,
}

pub(crate) fn vision_enabled_for_deepseek(turn: &TurnContext) -> bool {
    matches!(
        turn.config.model_provider_id.as_str(),
        DEEPSEEK_PROVIDER_ID | DEEPSEEK_BETA_PROVIDER_ID
    ) && turn.config.vision.enabled
}

pub(crate) async fn observation_contexts_for_user_input(
    turn: &TurnContext,
    content: &[UserInput],
) -> Vec<String> {
    if !vision_enabled_for_deepseek(turn) {
        return Vec::new();
    }

    let mut contexts = Vec::new();
    for item in content {
        let observation = match item {
            UserInput::Image { image_url, detail } => Some(
                observe_image_url(turn, image_url, *detail, Some(DEFAULT_AUTO_TASK_HINT)).await,
            ),
            UserInput::LocalImage { path, detail } => Some(
                observe_local_image_path(turn, path, None, *detail, Some(DEFAULT_AUTO_TASK_HINT))
                    .await,
            ),
            UserInput::Text { .. } | UserInput::Skill { .. } | UserInput::Mention { .. } => None,
            _ => None,
        };
        if let Some(observation) = observation {
            contexts.push(observation.to_context_fragment());
        }
    }

    contexts
}

pub(crate) async fn observe_local_image_path(
    turn: &TurnContext,
    path: &Path,
    environment_id: Option<&str>,
    detail: Option<ImageDetail>,
    task_hint: Option<&str>,
) -> ImageObservation {
    let config = &turn.config.vision;
    let source = path.display().to_string();
    if !config.enabled {
        return ImageObservation::error(
            source_hash_id(&source),
            source,
            "vision_disabled",
            "Vision Bridge is disabled in config",
            config,
        );
    }

    let prepared = match prepare_local_image(turn, path, environment_id).await {
        Ok(prepared) => prepared,
        Err(message) => {
            return ImageObservation::error(
                source_hash_id(&source),
                source,
                "image_read_error",
                message,
                config,
            );
        }
    };

    observe_prepared_image(turn, prepared, detail, task_hint).await
}

pub(crate) async fn observe_image_url(
    turn: &TurnContext,
    image_url: &str,
    detail: Option<ImageDetail>,
    task_hint: Option<&str>,
) -> ImageObservation {
    let config = &turn.config.vision;
    if !config.enabled {
        return ImageObservation::error(
            source_hash_id(image_url),
            "inline_or_remote_image".to_string(),
            "vision_disabled",
            "Vision Bridge is disabled in config",
            config,
        );
    }

    let prepared = match prepare_image_url(image_url) {
        Ok(prepared) => prepared,
        Err(message) => {
            return ImageObservation::error(
                source_hash_id(image_url),
                "inline_or_remote_image".to_string(),
                "invalid_image_url",
                message,
                config,
            );
        }
    };

    observe_prepared_image(turn, prepared, detail, task_hint).await
}

async fn observe_prepared_image(
    turn: &TurnContext,
    prepared: PreparedImage,
    detail: Option<ImageDetail>,
    task_hint: Option<&str>,
) -> ImageObservation {
    let config = &turn.config.vision;
    let cache_key = cache_key(config, &prepared.image_id, detail, task_hint);
    let cache_path = turn
        .config
        .deepseek_vibing_home
        .as_path()
        .join("vision-cache")
        .join(format!("{cache_key}.json"));

    if config.cache
        && let Ok(bytes) = fs::read(&cache_path).await
        && let Ok(mut observation) = serde_json::from_slice::<ImageObservation>(&bytes)
    {
        observation.cached = true;
        return observation;
    }

    let artifact_path = persist_artifact(turn, &prepared).await;
    let observation = match call_vision_provider(
        config,
        turn.config.auth.vision_api_key(),
        &prepared,
        detail,
        task_hint,
    )
    .await
    {
        Ok(provider_output) => ImageObservation::success(
            prepared.image_id.clone(),
            prepared.source.clone(),
            config,
            prepared.mime_type.clone(),
            artifact_path,
            provider_output,
        ),
        Err(err) => ImageObservation::error(
            prepared.image_id.clone(),
            prepared.source.clone(),
            err.kind,
            err.message,
            config,
        ),
    };

    if config.cache
        && observation.ok
        && let Ok(bytes) = serde_json::to_vec(&observation)
    {
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent).await;
        }
        if let Err(err) = fs::write(&cache_path, bytes).await {
            debug!(
                "failed to write vision cache {}: {err}",
                cache_path.display()
            );
        }
    }

    observation
}

async fn prepare_local_image(
    turn: &TurnContext,
    path: &Path,
    environment_id: Option<&str>,
) -> Result<PreparedImage, String> {
    let turn_environment = environment_id.map_or_else(
        || turn.environments.primary(),
        |environment_id| {
            turn.environments
                .turn_environments
                .iter()
                .find(|environment| environment.environment_id == environment_id)
        },
    );
    let Some(turn_environment) = turn_environment else {
        return Err("no tool environment is available for reading local images".to_string());
    };

    let abs_path = turn_environment.cwd.join(path);
    let sandbox = turn.file_system_sandbox_context(None, &turn_environment.cwd);
    let fs_handle = turn_environment.environment.get_filesystem();
    let metadata = fs_handle
        .get_metadata(&abs_path, Some(&sandbox))
        .await
        .map_err(|err| format!("unable to locate image at `{}`: {err}", abs_path.display()))?;
    if !metadata.is_file {
        return Err(format!("image path `{}` is not a file", abs_path.display()));
    }
    let bytes = fs_handle
        .read_file(&abs_path, Some(&sandbox))
        .await
        .map_err(|err| format!("unable to read image at `{}`: {err}", abs_path.display()))?;
    let mime_type = mime_type_for_path_or_bytes(abs_path.as_path(), &bytes);
    let image_id = sha256_hex(&bytes);
    let provider_image_url = data_url(
        mime_type.as_deref().unwrap_or("application/octet-stream"),
        &bytes,
    );
    Ok(PreparedImage {
        image_id,
        source: abs_path.display().to_string(),
        provider_image_url,
        bytes: Some(bytes),
        mime_type,
    })
}

fn prepare_image_url(image_url: &str) -> Result<PreparedImage, String> {
    if image_url.starts_with("data:") {
        let (mime_type, bytes) = parse_base64_data_url(image_url)?;
        let image_id = sha256_hex(&bytes);
        Ok(PreparedImage {
            image_id,
            source: format!("inline:{}", mime_type.as_deref().unwrap_or("unknown")),
            provider_image_url: image_url.to_string(),
            bytes: Some(bytes),
            mime_type,
        })
    } else if image_url.starts_with("http://") || image_url.starts_with("https://") {
        Ok(PreparedImage {
            image_id: source_hash_id(image_url),
            source: "remote_url".to_string(),
            provider_image_url: image_url.to_string(),
            bytes: None,
            mime_type: None,
        })
    } else {
        Err("image_url must be a base64 data URL or http(s) URL".to_string())
    }
}

async fn persist_artifact(turn: &TurnContext, prepared: &PreparedImage) -> Option<PathBuf> {
    let bytes = prepared.bytes.as_ref()?;
    let ext = prepared
        .mime_type
        .as_deref()
        .and_then(extension_for_mime_type)
        .unwrap_or("bin");
    let path = turn
        .config
        .deepseek_vibing_home
        .as_path()
        .join("vision-artifacts")
        .join(format!("{}.{}", prepared.image_id, ext));
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent).await
    {
        debug!(
            "failed to create vision artifact dir {}: {err}",
            parent.display()
        );
        return None;
    }
    if let Err(err) = fs::write(&path, bytes).await {
        debug!("failed to write vision artifact {}: {err}", path.display());
        return None;
    }
    Some(path)
}

#[derive(Debug)]
struct VisionProviderError {
    kind: String,
    message: String,
}

async fn call_vision_provider(
    config: &VisionConfig,
    configured_api_key: Option<&str>,
    prepared: &PreparedImage,
    detail: Option<ImageDetail>,
    task_hint: Option<&str>,
) -> Result<ProviderObservation, VisionProviderError> {
    if config.provider != VISION_PROVIDER_OPENAI_COMPATIBLE {
        return Err(VisionProviderError {
            kind: "unsupported_provider".to_string(),
            message: format!(
                "Vision provider `{}` is unsupported; expected `{}`",
                config.provider, VISION_PROVIDER_OPENAI_COMPATIBLE
            ),
        });
    }

    let api_key = vision_api_key_for_request(config, configured_api_key)?;

    let request = json!({
        "model": config.model,
        "stream": false,
        "response_format": {"type": "json_object"},
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": vision_prompt_text(task_hint),
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": prepared.provider_image_url,
                            "detail": image_detail_for_provider(detail),
                        }
                    }
                ]
            }
        ]
    });
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let response = build_reqwest_client()
        .post(&url)
        .bearer_auth(&api_key)
        .timeout(Duration::from_millis(config.timeout_ms))
        .json(&request)
        .send()
        .await
        .map_err(|err| VisionProviderError {
            kind: "request_error".to_string(),
            message: err.to_string(),
        })?;

    let status = response.status();
    let text = response.text().await.map_err(|err| VisionProviderError {
        kind: "response_read_error".to_string(),
        message: err.to_string(),
    })?;
    if !status.is_success() {
        let text = sanitize_provider_body(&text, &api_key);
        return Err(VisionProviderError {
            kind: "provider_status".to_string(),
            message: format!("vision provider returned HTTP {status}: {text}"),
        });
    }

    let content = extract_chat_completion_content(&text)?;
    Ok(provider_observation_from_content(&content))
}

fn vision_api_key_for_request(
    config: &VisionConfig,
    configured_api_key: Option<&str>,
) -> Result<String, VisionProviderError> {
    if let Some(api_key) = configured_api_key.filter(|value| !value.trim().is_empty()) {
        return Ok(api_key.trim().to_string());
    }

    std::env::var(&config.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| VisionProviderError {
            kind: "missing_api_key".to_string(),
            message: format!(
                "missing vision API key: set `[auth].vision_api_key` in DeepSeek Vibing config.toml or set `{}`",
                config.api_key_env
            ),
        })
}

fn sanitize_provider_body(body: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        body.to_string()
    } else {
        body.replace(api_key, "[REDACTED]")
    }
}

fn vision_prompt_text(task_hint: Option<&str>) -> String {
    format!(
        "{}\n\nReturn JSON only with fields: summary string, visible_text string array, ui_elements string array, important_details string array, uncertain_details string array.",
        task_hint.unwrap_or(DEFAULT_AUTO_TASK_HINT)
    )
}

fn extract_chat_completion_content(text: &str) -> Result<String, VisionProviderError> {
    let value = serde_json::from_str::<Value>(text).map_err(|err| VisionProviderError {
        kind: "invalid_provider_json".to_string(),
        message: err.to_string(),
    })?;
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or_else(|| VisionProviderError {
            kind: "missing_provider_content".to_string(),
            message: "vision provider response did not include choices[0].message.content"
                .to_string(),
        })?;

    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => Ok(parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")),
        other => Err(VisionProviderError {
            kind: "invalid_provider_content".to_string(),
            message: format!("unsupported content shape: {other}"),
        }),
    }
}

fn provider_observation_from_content(content: &str) -> ProviderObservation {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .or_else(|| {
            trimmed
                .strip_prefix("```")
                .and_then(|value| value.strip_suffix("```"))
        })
        .map(str::trim)
        .unwrap_or(trimmed);
    let Ok(value) = serde_json::from_str::<Value>(json_text) else {
        return ProviderObservation {
            summary: trimmed.to_string(),
            ..ProviderObservation::default()
        };
    };
    let Some(object) = value.as_object() else {
        return ProviderObservation {
            summary: trimmed.to_string(),
            ..ProviderObservation::default()
        };
    };
    ProviderObservation {
        summary: object
            .get("summary")
            .or_else(|| object.get("description"))
            .and_then(Value::as_str)
            .unwrap_or(trimmed)
            .to_string(),
        visible_text: string_array_field(object.get("visible_text")),
        ui_elements: string_array_field(object.get("ui_elements")),
        important_details: string_array_field(object.get("important_details")),
        uncertain_details: string_array_field(object.get("uncertain_details")),
    }
}

fn string_array_field(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(value)) if !value.is_empty() => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn parse_base64_data_url(data_url: &str) -> Result<(Option<String>, Vec<u8>), String> {
    let without_prefix = data_url
        .strip_prefix("data:")
        .ok_or_else(|| "data URL must start with `data:`".to_string())?;
    let (metadata, payload) = without_prefix
        .split_once(',')
        .ok_or_else(|| "data URL is missing comma separator".to_string())?;
    let mut parts = metadata.split(';');
    let mime_type = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if !parts.any(|part| part.eq_ignore_ascii_case("base64")) {
        return Err("only base64 image data URLs are supported".to_string());
    }
    let bytes = BASE64_STANDARD
        .decode(payload)
        .map_err(|err| format!("failed to decode image data URL: {err}"))?;
    Ok((mime_type, bytes))
}

fn data_url(mime_type: &str, bytes: &[u8]) -> String {
    format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes))
}

fn mime_type_for_path_or_bytes(path: &Path, bytes: &[u8]) -> Option<String> {
    if let Ok(format) = image::guess_format(bytes)
        && let Some(mime) = mime_type_for_image_format(format)
    {
        return Some(mime.to_string());
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(mime_type_for_extension)
        .map(ToString::to_string)
}

fn mime_type_for_image_format(format: image::ImageFormat) -> Option<&'static str> {
    match format {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::Gif => Some("image/gif"),
        image::ImageFormat::WebP => Some("image/webp"),
        image::ImageFormat::Bmp => Some("image/bmp"),
        image::ImageFormat::Tiff => Some("image/tiff"),
        _ => None,
    }
}

fn mime_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "tif" | "tiff" => Some("image/tiff"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn extension_for_mime_type(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        "image/tiff" => Some("tiff"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

fn image_detail_for_provider(detail: Option<ImageDetail>) -> &'static str {
    match detail {
        Some(ImageDetail::Low) => "low",
        Some(ImageDetail::Auto) | None => "auto",
        Some(ImageDetail::High) | Some(ImageDetail::Original) => "high",
    }
}

fn cache_key(
    config: &VisionConfig,
    image_id: &str,
    detail: Option<ImageDetail>,
    task_hint: Option<&str>,
) -> String {
    let payload = format!(
        "{}\n{}\n{}\n{:?}\n{}\n{}",
        config.provider,
        config.base_url,
        config.model,
        detail,
        task_hint.unwrap_or(DEFAULT_AUTO_TASK_HINT),
        image_id
    );
    sha256_hex(payload.as_bytes())
}

fn source_hash_id(source: &str) -> String {
    sha256_hex(source.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_vibing_protocol::user_input::UserInput;
    use std::sync::Arc;

    #[test]
    fn parses_json_observation_content() {
        let observation = provider_observation_from_content(
            r#"{"summary":"screen","visible_text":["Build failed"],"ui_elements":["terminal"],"important_details":["exit 1"],"uncertain_details":["line number"]}"#,
        );
        assert_eq!(observation.summary, "screen");
        assert_eq!(observation.visible_text, vec!["Build failed"]);
        assert_eq!(observation.ui_elements, vec!["terminal"]);
        assert_eq!(observation.important_details, vec!["exit 1"]);
        assert_eq!(observation.uncertain_details, vec!["line number"]);
    }

    #[test]
    fn wraps_plain_text_observation_content() {
        let observation = provider_observation_from_content("a plain description");
        assert_eq!(observation.summary, "a plain description");
        assert!(observation.visible_text.is_empty());
    }

    #[test]
    fn data_url_source_does_not_expose_payload() {
        let prepared = prepare_image_url("data:image/png;base64,Zm9v").expect("valid data URL");
        assert_eq!(prepared.source, "inline:image/png");
        assert_ne!(prepared.image_id, "Zm9v");
    }

    #[tokio::test]
    async fn missing_api_key_is_structured_error() {
        let config = VisionConfig {
            api_key_env: "DEEPSEEK_VIBING_TEST_VISION_KEY_MISSING".to_string(),
            ..VisionConfig::default()
        };
        let prepared = prepare_image_url("data:image/png;base64,Zm9v").expect("valid data URL");

        let err = call_vision_provider(&config, None, &prepared, None, None)
            .await
            .expect_err("missing key should fail");

        assert_eq!(err.kind, "missing_api_key");
        assert!(
            err.message
                .contains("DEEPSEEK_VIBING_TEST_VISION_KEY_MISSING")
        );
        assert!(err.message.contains("[auth].vision_api_key"));
    }

    #[test]
    fn configured_vision_key_is_used_before_env_fallback() {
        let config = VisionConfig {
            api_key_env: "DEEPSEEK_VIBING_TEST_VISION_KEY_MISSING".to_string(),
            ..VisionConfig::default()
        };

        let api_key = vision_api_key_for_request(&config, Some("  sk-vision-config  "))
            .expect("configured key should resolve");

        assert_eq!(api_key, "sk-vision-config");
    }

    #[test]
    fn extracts_openai_compatible_string_content() {
        let content = extract_chat_completion_content(
            &json!({
                "choices": [{
                    "message": {
                        "content": "{\"summary\":\"terminal screenshot\",\"visible_text\":[\"error E0425\"]}"
                    }
                }]
            })
            .to_string(),
        )
        .expect("content");

        let observation = provider_observation_from_content(&content);
        assert_eq!(observation.summary, "terminal screenshot");
        assert_eq!(observation.visible_text, vec!["error E0425"]);
    }

    #[test]
    fn extracts_openai_compatible_array_content() {
        let content = extract_chat_completion_content(
            &json!({
                "choices": [{
                    "message": {
                        "content": [{"type": "text", "text": "plain screenshot description"}]
                    }
                }]
            })
            .to_string(),
        )
        .expect("content");

        assert_eq!(content, "plain screenshot description");
    }

    #[test]
    fn cache_key_includes_hint_and_model() {
        let base = VisionConfig::default();
        let other_model = VisionConfig {
            model: "other-vision".to_string(),
            ..base.clone()
        };

        assert_ne!(
            cache_key(&base, "image", None, Some("read text")),
            cache_key(&base, "image", None, Some("inspect layout"))
        );
        assert_ne!(
            cache_key(&base, "image", None, Some("read text")),
            cache_key(&other_model, "image", None, Some("read text"))
        );
    }

    #[tokio::test]
    async fn auto_observation_context_omits_raw_data_url_on_missing_key() {
        let (_session, mut turn) = crate::session::tests::make_session_and_context().await;
        Arc::make_mut(&mut turn.config).vision.api_key_env =
            "DEEPSEEK_VIBING_TEST_VISION_AUTO_MISSING".to_string();
        let raw_image_url = "data:image/png;base64,Zm9v";

        let contexts = observation_contexts_for_user_input(
            &turn,
            &[UserInput::Image {
                image_url: raw_image_url.to_string(),
                detail: None,
            }],
        )
        .await;

        assert_eq!(contexts.len(), 1);
        assert!(contexts[0].contains("<image_observation>"));
        assert!(contexts[0].contains("\"ok\":false"));
        assert!(contexts[0].contains("missing_api_key"));
        assert!(!contexts[0].contains(raw_image_url));
        assert!(!contexts[0].contains("Zm9v"));
    }
}
