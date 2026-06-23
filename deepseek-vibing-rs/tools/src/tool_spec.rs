use crate::FreeformTool;
use crate::JsonSchema;
use crate::LoadableToolSpec;
use crate::ResponsesApiNamespace;
use crate::ResponsesApiNamespaceTool;
use crate::ResponsesApiTool;
use deepseek_vibing_protocol::config_types::WebSearchContextSize;
use deepseek_vibing_protocol::config_types::WebSearchFilters as ConfigWebSearchFilters;
use deepseek_vibing_protocol::config_types::WebSearchUserLocation as ConfigWebSearchUserLocation;
use deepseek_vibing_protocol::config_types::WebSearchUserLocationType;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

/// When serialized as JSON, this produces a valid "Tool" in the OpenAI
/// Responses API.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum ToolSpec {
    #[serde(rename = "function")]
    Function(ResponsesApiTool),
    #[serde(rename = "namespace")]
    Namespace(ResponsesApiNamespace),
    #[serde(rename = "tool_search")]
    ToolSearch {
        execution: String,
        description: String,
        parameters: JsonSchema,
    },
    #[serde(rename = "image_generation")]
    ImageGeneration { output_format: String },
    // TODO: Understand why we get an error on web_search although the API docs
    // say it's supported.
    // https://platform.openai.com/docs/guides/tools-web-search?api-mode=responses#:~:text=%7B%20type%3A%20%22web_search%22%20%7D%2C
    // The `external_web_access` field determines whether the web search is over
    // cached or live content.
    // https://platform.openai.com/docs/guides/tools-web-search#live-internet-access
    #[serde(rename = "web_search")]
    WebSearch {
        #[serde(skip_serializing_if = "Option::is_none")]
        external_web_access: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filters: Option<ResponsesApiWebSearchFilters>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<ResponsesApiWebSearchUserLocation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_context_size: Option<WebSearchContextSize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_content_types: Option<Vec<String>>,
    },
    #[serde(rename = "custom")]
    Freeform(FreeformTool),
}

impl ToolSpec {
    pub fn name(&self) -> &str {
        match self {
            ToolSpec::Function(tool) => tool.name.as_str(),
            ToolSpec::Namespace(namespace) => namespace.name.as_str(),
            ToolSpec::ToolSearch { .. } => "tool_search",
            ToolSpec::ImageGeneration { .. } => "image_generation",
            ToolSpec::WebSearch { .. } => "web_search",
            ToolSpec::Freeform(tool) => tool.name.as_str(),
        }
    }
}

impl From<LoadableToolSpec> for ToolSpec {
    fn from(value: LoadableToolSpec) -> Self {
        match value {
            LoadableToolSpec::Function(tool) => ToolSpec::Function(tool),
            LoadableToolSpec::Namespace(namespace) => ToolSpec::Namespace(namespace),
        }
    }
}

/// Returns JSON values that are compatible with Function Calling in the
/// Responses API:
/// https://platform.openai.com/docs/guides/function-calling?api-mode=responses
pub fn create_tools_json_for_responses_api(
    tools: &[ToolSpec],
) -> Result<Vec<Value>, serde_json::Error> {
    let mut tools_json = Vec::new();

    for tool in tools {
        let json = serde_json::to_value(tool)?;
        tools_json.push(json);
    }

    Ok(tools_json)
}

/// Returns JSON values that are compatible with OpenAI-format Chat Completions
/// tool calling as implemented by DeepSeek.
pub fn create_tools_json_for_deepseek_chat(
    tools: &[ToolSpec],
    strict: bool,
) -> Result<Vec<Value>, serde_json::Error> {
    let mut tools_json = Vec::new();

    for tool in tools {
        match tool {
            ToolSpec::Function(function) => {
                tools_json.push(deepseek_function_tool_json(function, strict)?);
            }
            ToolSpec::Namespace(namespace) => {
                for namespace_tool in &namespace.tools {
                    let ResponsesApiNamespaceTool::Function(function) = namespace_tool;
                    tools_json.push(deepseek_function_tool_json(function, strict)?);
                }
            }
            ToolSpec::ToolSearch { .. }
            | ToolSpec::ImageGeneration { .. }
            | ToolSpec::WebSearch { .. }
            | ToolSpec::Freeform(_) => {}
        }
    }

    Ok(tools_json)
}

fn deepseek_function_tool_json(
    tool: &ResponsesApiTool,
    strict: bool,
) -> Result<Value, serde_json::Error> {
    let mut function = serde_json::Map::new();
    function.insert("name".to_string(), serde_json::to_value(&tool.name)?);
    function.insert(
        "description".to_string(),
        serde_json::to_value(&tool.description)?,
    );
    let mut parameters = serde_json::to_value(&tool.parameters)?;
    normalize_deepseek_stable_schema(&mut parameters);
    let strict_enabled = strict && normalize_deepseek_strict_schema(&mut parameters);
    if strict && !strict_enabled {
        warn!(
            tool = tool.name.as_str(),
            "DeepSeek strict tool schema is unsupported for this tool; falling back to stable tool calling"
        );
    }
    function.insert("parameters".to_string(), parameters);
    if strict_enabled {
        function.insert("strict".to_string(), Value::Bool(true));
    }

    let mut root = serde_json::Map::new();
    root.insert("type".to_string(), Value::String("function".to_string()));
    root.insert("function".to_string(), Value::Object(function));
    Ok(Value::Object(root))
}

fn normalize_deepseek_stable_schema(schema: &mut Value) {
    let Value::Object(object) = schema else {
        return;
    };

    if object.get("type").is_some_and(|value| value == "object") {
        object
            .entry("additionalProperties")
            .or_insert(Value::Bool(false));
        if let Some(Value::Object(properties)) = object.get_mut("properties") {
            for property_schema in properties.values_mut() {
                normalize_deepseek_stable_schema(property_schema);
            }
        }
    }

    if let Some(items) = object.get_mut("items") {
        normalize_deepseek_stable_schema(items);
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(variants)) = object.get_mut(keyword) {
            for variant in variants {
                normalize_deepseek_stable_schema(variant);
            }
        }
    }
}

fn normalize_deepseek_strict_schema(schema: &mut Value) -> bool {
    let Value::Object(object) = schema else {
        return false;
    };

    if !object.get("type").is_some_and(|value| value == "object") {
        return false;
    }

    object
        .entry("additionalProperties")
        .or_insert(Value::Bool(false));

    let Some(Value::Object(properties)) = object.get("properties") else {
        object.insert("required".to_string(), Value::Array(Vec::new()));
        return true;
    };

    let required = properties
        .keys()
        .map(|key| Value::String(key.clone()))
        .collect::<Vec<_>>();
    object.insert("required".to_string(), Value::Array(required));

    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for property_schema in properties.values_mut() {
            normalize_deepseek_strict_schema_variant(property_schema);
        }
    }

    true
}

fn normalize_deepseek_strict_schema_variant(schema: &mut Value) {
    normalize_deepseek_stable_schema(schema);

    if schema
        .as_object()
        .is_some_and(|object| object.get("type").is_some_and(|value| value == "object"))
    {
        let _ = normalize_deepseek_strict_schema(schema);
        return;
    }

    let Some(object) = schema.as_object_mut() else {
        return;
    };

    if let Some(items) = object.get_mut("items") {
        normalize_deepseek_strict_schema_variant(items);
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(variants)) = object.get_mut(keyword) {
            for variant in variants {
                normalize_deepseek_strict_schema_variant(variant);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiWebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
}

impl From<ConfigWebSearchFilters> for ResponsesApiWebSearchFilters {
    fn from(filters: ConfigWebSearchFilters) -> Self {
        Self {
            allowed_domains: filters.allowed_domains,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiWebSearchUserLocation {
    #[serde(rename = "type")]
    pub r#type: WebSearchUserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl From<ConfigWebSearchUserLocation> for ResponsesApiWebSearchUserLocation {
    fn from(user_location: ConfigWebSearchUserLocation) -> Self {
        Self {
            r#type: user_location.r#type,
            country: user_location.country,
            region: user_location.region,
            city: user_location.city,
            timezone: user_location.timezone,
        }
    }
}

#[cfg(test)]
#[path = "tool_spec_tests.rs"]
mod tests;
