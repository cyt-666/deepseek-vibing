use deepseek_vibing_tools::FreeformTool;
use deepseek_vibing_tools::FreeformToolFormat;
use deepseek_vibing_tools::JsonSchema;
use deepseek_vibing_tools::ResponsesApiTool;
use deepseek_vibing_tools::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

const APPLY_PATCH_LARK_GRAMMAR: &str = include_str!("apply_patch.lark");

/// Returns a custom tool that can be used to edit files. Well-suited for GPT-5 models
/// https://platform.openai.com/docs/guides/function-calling#custom-tools
pub fn create_apply_patch_freeform_tool(include_environment_id: bool) -> ToolSpec {
    let definition = if include_environment_id {
        APPLY_PATCH_LARK_GRAMMAR.replace(
            "start: begin_patch hunk+ end_patch",
            "start: begin_patch environment_id? hunk+ end_patch\nenvironment_id: \"*** Environment ID: \" filename LF",
        )
    } else {
        APPLY_PATCH_LARK_GRAMMAR.to_string()
    };
    ToolSpec::Freeform(FreeformTool {
        name: "apply_patch".to_string(),
        description: "Use the `apply_patch` tool to edit files. This is a FREEFORM tool, so do not wrap the patch in JSON.".to_string(),
        format: FreeformToolFormat {
            r#type: "grammar".to_string(),
            syntax: "lark".to_string(),
            definition,
        },
    })
}

pub fn create_apply_patch_function_tool(include_environment_id: bool) -> ToolSpec {
    let mut properties = BTreeMap::from([(
        "patch".to_string(),
        JsonSchema::string(Some(
            "Complete apply_patch text, not git/unified diff; include `*** Begin Patch` and `*** End Patch`."
                .to_string(),
        )),
    )]);
    if include_environment_id {
        properties.insert(
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Environment id. Omit for the primary environment.".to_string(),
            )),
        );
    }

    ToolSpec::Function(ResponsesApiTool {
        name: "apply_patch".to_string(),
        description: "Edit files with a complete DeepSeek Vibing apply_patch string.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["patch".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(json!({
            "type": "object",
            "properties": {
                "ok": {"type": "boolean"},
                "message": {"type": "string"}
            },
            "additionalProperties": true
        })),
    })
}

#[cfg(test)]
#[path = "apply_patch_spec_tests.rs"]
mod tests;
