use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

pub const READ_FILE_TOOL_NAME: &str = "read_file";
pub const READ_FILE_ALIAS_TOOL_NAME: &str = "read";
pub const DEFAULT_LINE_LIMIT: usize = 200;
pub const MAX_LINE_LIMIT: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadFileToolOptions {
    pub include_environment_id: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFileArgs {
    pub path: String,
    pub start_line: usize,
    pub limit: usize,
    pub environment_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFileToolError {
    pub kind: &'static str,
    pub message: String,
}

#[derive(Deserialize)]
struct RawReadFileArgs {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    environment_id: Option<String>,
}

#[derive(Serialize)]
struct ReadFileSuccess {
    ok: bool,
    path: String,
    start_line: usize,
    end_line: usize,
    total_lines: usize,
    size_bytes: usize,
    truncated: bool,
    content: String,
}

pub fn create_read_file_tool(options: ReadFileToolOptions) -> ToolSpec {
    let mut properties = BTreeMap::from([
        (
            "path".to_string(),
            JsonSchema::string(Some(
                "Path to the text file to read, relative to the current environment cwd unless absolute."
                    .to_string(),
            )),
        ),
        (
            "start_line".to_string(),
            JsonSchema::integer(Some(
                "1-based line number to start reading from. Defaults to 1.".to_string(),
            )),
        ),
        (
            "limit".to_string(),
            JsonSchema::integer(Some(
                "Maximum number of lines to return. Defaults to 200 and must be between 1 and 1000."
                    .to_string(),
            )),
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
        name: READ_FILE_TOOL_NAME.to_string(),
        description: "Read a UTF-8 text file from the workspace using the current filesystem sandbox. Prefer this over shell commands such as `cat` when inspecting files."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["path".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(json!({
            "type": "object",
            "properties": {
                "ok": {"type": "boolean"},
                "path": {"type": "string"},
                "start_line": {"type": "integer"},
                "end_line": {"type": "integer"},
                "total_lines": {"type": "integer"},
                "size_bytes": {"type": "integer"},
                "truncated": {"type": "boolean"},
                "content": {"type": "string"}
            },
            "required": [
                "ok",
                "path",
                "start_line",
                "end_line",
                "total_lines",
                "size_bytes",
                "truncated",
                "content"
            ],
            "additionalProperties": false
        })),
    })
}

pub fn parse_read_file_args(arguments: &str) -> Result<ReadFileArgs, ReadFileToolError> {
    let RawReadFileArgs {
        path,
        start_line,
        limit,
        environment_id,
    } = serde_json::from_str(arguments).map_err(|err| ReadFileToolError {
        kind: "invalid_arguments",
        message: format!("failed to parse function arguments: {err}"),
    })?;

    if path.trim().is_empty() {
        return Err(ReadFileToolError {
            kind: "invalid_path",
            message: "read_file.path must not be empty".to_string(),
        });
    }

    let start_line = start_line.unwrap_or(1);
    if start_line == 0 {
        return Err(ReadFileToolError {
            kind: "invalid_start_line",
            message: "read_file.start_line must be 1 or greater".to_string(),
        });
    }

    let limit = limit.unwrap_or(DEFAULT_LINE_LIMIT);
    if !(1..=MAX_LINE_LIMIT).contains(&limit) {
        return Err(ReadFileToolError {
            kind: "invalid_limit",
            message: format!("read_file.limit must be between 1 and {MAX_LINE_LIMIT}"),
        });
    }

    Ok(ReadFileArgs {
        path,
        start_line,
        limit,
        environment_id,
    })
}

pub fn read_file_success_text(
    path: String,
    text: &str,
    size_bytes: usize,
    start_line: usize,
    limit: usize,
) -> Result<String, serde_json::Error> {
    let lines = text.lines().collect::<Vec<_>>();
    let total_lines = lines.len();
    let start_index = start_line.saturating_sub(1);
    let selected_lines = lines
        .iter()
        .skip(start_index)
        .take(limit)
        .copied()
        .collect::<Vec<_>>();
    let content = selected_lines.join("\n");
    let selected_len = selected_lines.len();
    let end_line = if selected_len == 0 {
        total_lines
    } else {
        start_line + selected_len - 1
    };
    let truncated = start_index.saturating_add(selected_len) < total_lines;
    serde_json::to_string(&ReadFileSuccess {
        ok: true,
        path,
        start_line,
        end_line,
        total_lines,
        size_bytes,
        truncated,
        content,
    })
}

pub fn read_file_error_text(tool_name: &str, kind: &str, message: &str) -> String {
    json!({
        "ok": false,
        "kind": kind,
        "message": message,
        "tool": tool_name,
    })
    .to_string()
}

#[cfg(test)]
#[path = "read_file_tests.rs"]
mod tests;
