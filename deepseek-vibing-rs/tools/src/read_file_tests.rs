use super::*;
use crate::AdditionalProperties;
use crate::JsonSchemaPrimitiveType;
use crate::JsonSchemaType;
use crate::ToolSpec;
use pretty_assertions::assert_eq;
use serde_json::Value;

#[test]
fn create_read_file_tool_matches_expected_spec() {
    let ToolSpec::Function(tool) = create_read_file_tool(ReadFileToolOptions {
        include_environment_id: false,
    }) else {
        panic!("expected function tool");
    };

    assert_eq!(tool.name, "read_file");
    assert_eq!(tool.strict, false);
    assert_eq!(
        tool.parameters.required.as_deref(),
        Some(&["path".to_string()][..])
    );
    assert_eq!(
        tool.parameters.additional_properties,
        Some(AdditionalProperties::Boolean(false))
    );
    let properties = tool.parameters.properties.as_ref().expect("properties");
    assert!(properties.contains_key("path"));
    assert!(properties.contains_key("start_line"));
    assert!(properties.contains_key("limit"));
    assert!(!properties.contains_key("environment_id"));
    assert!(tool.output_schema.is_some());
}

#[test]
fn create_read_file_tool_includes_environment_id_when_requested() {
    let ToolSpec::Function(tool) = create_read_file_tool(ReadFileToolOptions {
        include_environment_id: true,
    }) else {
        panic!("expected function tool");
    };

    let properties = tool.parameters.properties.as_ref().expect("properties");
    assert!(properties.contains_key("environment_id"));
    assert_eq!(
        tool.parameters.required.as_deref(),
        Some(&["path".to_string()][..])
    );
}

#[test]
fn parse_read_file_args_applies_defaults_and_validates_bounds() {
    let args = parse_read_file_args(r#"{"path":"src/lib.rs"}"#).expect("valid args");
    assert_eq!(args.path, "src/lib.rs");
    assert_eq!(args.start_line, 1);
    assert_eq!(args.limit, 200);
    assert_eq!(args.environment_id, None);

    let args = parse_read_file_args(
        r#"{"path":"src/lib.rs","start_line":3,"limit":4,"environment_id":"env-1"}"#,
    )
    .expect("valid args");
    assert_eq!(args.start_line, 3);
    assert_eq!(args.limit, 4);
    assert_eq!(args.environment_id.as_deref(), Some("env-1"));

    assert_eq!(
        parse_read_file_args("{").unwrap_err().kind,
        "invalid_arguments"
    );
    assert_eq!(
        parse_read_file_args(r#"{"path":""}"#).unwrap_err().kind,
        "invalid_path"
    );
    assert_eq!(
        parse_read_file_args(r#"{"path":"x","start_line":0}"#)
            .unwrap_err()
            .kind,
        "invalid_start_line"
    );
    assert_eq!(
        parse_read_file_args(r#"{"path":"x","limit":1001}"#)
            .unwrap_err()
            .kind,
        "invalid_limit"
    );
}

#[test]
fn read_file_success_text_slices_lines_and_marks_truncation() {
    let output = read_file_success_text(
        "/tmp/sample.txt".to_string(),
        "one\ntwo\nthree\nfour\n",
        19,
        2,
        2,
    )
    .expect("success JSON");
    let value: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["path"], "/tmp/sample.txt");
    assert_eq!(value["start_line"], 2);
    assert_eq!(value["end_line"], 3);
    assert_eq!(value["total_lines"], 4);
    assert_eq!(value["size_bytes"], 19);
    assert_eq!(value["truncated"], true);
    assert_eq!(value["content"], "two\nthree");
}

#[test]
fn read_file_success_text_handles_out_of_range_start() {
    let output = read_file_success_text("/tmp/sample.txt".to_string(), "one\ntwo", 7, 5, 2)
        .expect("success JSON");
    let value: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(value["start_line"], 5);
    assert_eq!(value["end_line"], 2);
    assert_eq!(value["total_lines"], 2);
    assert_eq!(value["truncated"], false);
    assert_eq!(value["content"], "");
}

#[test]
fn read_file_error_text_is_structured_json() {
    let output = read_file_error_text("read", "not_found", "missing file");
    let value: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(value["ok"], false);
    assert_eq!(value["tool"], "read");
    assert_eq!(value["kind"], "not_found");
    assert_eq!(value["message"], "missing file");
}

#[test]
fn read_file_schema_uses_object_parameters() {
    let ToolSpec::Function(tool) = create_read_file_tool(ReadFileToolOptions {
        include_environment_id: false,
    }) else {
        panic!("expected function tool");
    };

    assert_eq!(
        tool.parameters.schema_type,
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
}
