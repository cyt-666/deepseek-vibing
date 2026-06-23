use deepseek_vibing_tools::JsonSchema;
use deepseek_vibing_tools::ResponsesApiTool;
use deepseek_vibing_tools::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

pub const DESCRIBE_IMAGE_TOOL_NAME: &str = "describe_image";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescribeImageToolOptions {
    pub include_environment_id: bool,
}

pub fn create_describe_image_tool(options: DescribeImageToolOptions) -> ToolSpec {
    let mut properties = BTreeMap::from([
        (
            "path".to_string(),
            JsonSchema::string(Some("Local filesystem path to an image file.".to_string())),
        ),
        (
            "task_hint".to_string(),
            JsonSchema::string(Some(
                "Optional instruction describing what to look for in the image.".to_string(),
            )),
        ),
        (
            "detail".to_string(),
            JsonSchema::string_enum(
                vec![json!("auto"), json!("low"), json!("high")],
                Some("Vision detail hint. Defaults to `auto`.".to_string()),
            ),
        ),
    ]);
    if options.include_environment_id {
        properties.insert(
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Environment id from <environment_context>. Omit to use the primary environment."
                    .to_string(),
            )),
        );
    }

    ToolSpec::Function(ResponsesApiTool {
        name: DESCRIBE_IMAGE_TOOL_NAME.to_string(),
        description: "Describe a local image for DeepSeek by routing it through the configured Vision Bridge. Returns structured JSON text; the main model never receives the raw image."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, Some(vec!["path".to_string()]), Some(false.into())),
        output_schema: Some(describe_image_output_schema()),
    })
}

fn describe_image_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ok": {"type": "boolean"},
            "image_id": {"type": "string"},
            "source": {"type": "string"},
            "summary": {"type": "string"},
            "visible_text": {"type": "array", "items": {"type": "string"}},
            "ui_elements": {"type": "array", "items": {"type": "string"}},
            "important_details": {"type": "array", "items": {"type": "string"}},
            "uncertain_details": {"type": "array", "items": {"type": "string"}},
            "cached": {"type": "boolean"}
        },
        "required": [
            "ok",
            "image_id",
            "source",
            "summary",
            "visible_text",
            "ui_elements",
            "important_details",
            "uncertain_details",
            "cached"
        ],
        "additionalProperties": true
    })
}
