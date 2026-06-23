#![cfg(not(target_os = "windows"))]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use core_test_support::PathExt;
use core_test_support::apps_test_server::AppsTestServer;
use core_test_support::apps_test_server::SEARCH_CALENDAR_CREATE_TOOL;
use core_test_support::apps_test_server::SEARCH_CALENDAR_NAMESPACE;
use core_test_support::apps_test_server::recorded_apps_tool_call_by_call_id;
use core_test_support::apps_test_server::search_capable_apps_builder;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_deepseek_vibing::TestDeepSeekVibing;
use core_test_support::test_deepseek_vibing::turn_permission_fields;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use deepseek_vibing_config::types::AppToolApproval;
use deepseek_vibing_core::config::Config;
use deepseek_vibing_features::Feature;
use deepseek_vibing_protocol::config_types::CollaborationMode;
use deepseek_vibing_protocol::config_types::ModeKind;
use deepseek_vibing_protocol::config_types::Settings;
use deepseek_vibing_protocol::models::PermissionProfile;
use deepseek_vibing_protocol::protocol::AskForApproval;
use deepseek_vibing_protocol::protocol::ElicitationAction;
use deepseek_vibing_protocol::protocol::EventMsg;
use deepseek_vibing_protocol::protocol::Op;
use deepseek_vibing_protocol::request_user_input::RequestUserInputAnswer;
use deepseek_vibing_protocol::request_user_input::RequestUserInputResponse;
use deepseek_vibing_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::HashMap;

fn set_calendar_approval_mode(config: &mut Config, approval_mode: AppToolApproval) {
    let approval_mode = match approval_mode {
        AppToolApproval::Auto => "auto",
        AppToolApproval::Prompt => "prompt",
        AppToolApproval::Approve => "approve",
    };
    let user_config_path = config.deepseek_vibing_home.join("config.toml").abs();
    let user_config = toml::from_str(&format!(
        r#"
[apps.calendar]
default_tools_approval_mode = "{approval_mode}"
"#
    ))
    .expect("apps config should parse");
    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config(&user_config_path, user_config);
}

async fn submit_user_turn(
    test: &TestDeepSeekVibing,
    text: &str,
    approval_policy: AskForApproval,
    collaboration_mode: Option<CollaborationMode>,
) -> Result<()> {
    let session_model = test.session_configured.model.clone();
    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, test.cwd.path());
    test.deepseek_vibing
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
            environments: None,
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: Default::default(),
            thread_settings: deepseek_vibing_protocol::protocol::ThreadSettingsOverrides {
                cwd: Some(test.cwd.path().to_path_buf()),
                approval_policy: Some(approval_policy),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                collaboration_mode: collaboration_mode.or({
                    Some(deepseek_vibing_protocol::config_types::CollaborationMode {
                        mode: deepseek_vibing_protocol::config_types::ModeKind::Default,
                        settings: deepseek_vibing_protocol::config_types::Settings {
                            model: session_model,
                            reasoning_effort: None,
                            developer_instructions: None,
                        },
                    })
                }),
                ..Default::default()
            },
        })
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approved_mcp_tool_call_metadata_records_prior_user_input_request() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let apps_server = AppsTestServer::mount(&server).await?;
    let call_id = "calendar-call-approval";
    let calendar_args = serde_json::to_string(&json!({
        "title": "Lunch",
        "starts_at": "2026-03-10T12:00:00Z"
    }))?;
    let mock = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_function_call_with_namespace(
                    call_id,
                    SEARCH_CALENDAR_NAMESPACE,
                    SEARCH_CALENDAR_CREATE_TOOL,
                    &calendar_args,
                ),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-1", "done"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = search_capable_apps_builder(apps_server.chatgpt_base_url.clone())
        .with_config(|config| {
            config
                .features
                .enable(Feature::ToolCallMcpElicitation)
                .expect("test config should allow feature update");
            set_calendar_approval_mode(config, AppToolApproval::Prompt);
        });
    let test = builder.build(&server).await?;

    submit_user_turn(
        &test,
        "Use [$calendar](app://calendar) to create a calendar event.",
        AskForApproval::OnRequest,
        /*collaboration_mode*/ None,
    )
    .await?;

    let EventMsg::McpToolCallBegin(begin) = wait_for_event(&test.deepseek_vibing, |event| {
        matches!(event, EventMsg::McpToolCallBegin(_))
    })
    .await
    else {
        unreachable!("event guard guarantees McpToolCallBegin");
    };
    assert_eq!(begin.call_id, call_id);

    let EventMsg::ElicitationRequest(request) = wait_for_event(&test.deepseek_vibing, |event| {
        matches!(event, EventMsg::ElicitationRequest(_))
    })
    .await
    else {
        unreachable!("event guard guarantees ElicitationRequest");
    };

    test.deepseek_vibing
        .submit(Op::ResolveElicitation {
            server_name: request.server_name,
            request_id: request.id,
            decision: ElicitationAction::Accept,
            content: None,
            meta: None,
        })
        .await?;

    wait_for_event(&test.deepseek_vibing, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    assert_eq!(mock.requests().len(), 2);
    let apps_tool_call = recorded_apps_tool_call_by_call_id(&server, call_id).await;

    assert_eq!(
        apps_tool_call.pointer(
            "/params/_meta/x-deepseek-vibing-turn-metadata/user_input_requested_during_turn"
        ),
        Some(&json!(true))
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_tool_call_metadata_records_prior_request_user_input_tool() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let apps_server = AppsTestServer::mount(&server).await?;
    let request_user_input_call_id = "user-input-call";
    let calendar_call_id = "calendar-call-after-user-input";
    let request_user_input_args = json!({
        "questions": [{
            "id": "confirm_path",
            "header": "Confirm",
            "question": "Proceed with the plan?",
            "options": [{
                "label": "Yes (Recommended)",
                "description": "Continue the current plan."
            }, {
                "label": "No",
                "description": "Stop and revisit the approach."
            }]
        }]
    })
    .to_string();
    let calendar_args = serde_json::to_string(&json!({
        "title": "Lunch",
        "starts_at": "2026-03-10T12:00:00Z"
    }))?;
    let mock = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_function_call(
                    request_user_input_call_id,
                    "request_user_input",
                    &request_user_input_args,
                ),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_function_call_with_namespace(
                    calendar_call_id,
                    SEARCH_CALENDAR_NAMESPACE,
                    SEARCH_CALENDAR_CREATE_TOOL,
                    &calendar_args,
                ),
                ev_completed("resp-2"),
            ]),
            sse(vec![
                ev_response_created("resp-3"),
                ev_assistant_message("msg-1", "done"),
                ev_completed("resp-3"),
            ]),
        ],
    )
    .await;

    let mut builder = search_capable_apps_builder(apps_server.chatgpt_base_url.clone())
        .with_config(|config| {
            set_calendar_approval_mode(config, AppToolApproval::Approve);
        });
    let test = builder.build(&server).await?;

    submit_user_turn(
        &test,
        "Ask for confirmation, then create a calendar event.",
        AskForApproval::Never,
        Some(CollaborationMode {
            mode: ModeKind::Plan,
            settings: Settings {
                model: test.session_configured.model.clone(),
                reasoning_effort: None,
                developer_instructions: None,
            },
        }),
    )
    .await?;

    let request = wait_for_event_match(&test.deepseek_vibing, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    assert_eq!(request.call_id, request_user_input_call_id);

    test.deepseek_vibing
        .submit(Op::UserInputAnswer {
            id: request.turn_id,
            response: RequestUserInputResponse {
                answers: HashMap::from([(
                    "confirm_path".to_string(),
                    RequestUserInputAnswer {
                        answers: vec!["Yes (Recommended)".to_string()],
                    },
                )]),
            },
        })
        .await?;

    let EventMsg::McpToolCallBegin(begin) = wait_for_event(&test.deepseek_vibing, |event| {
        matches!(event, EventMsg::McpToolCallBegin(_))
    })
    .await
    else {
        unreachable!("event guard guarantees McpToolCallBegin");
    };
    assert_eq!(begin.call_id, calendar_call_id);

    wait_for_event(&test.deepseek_vibing, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    assert_eq!(mock.requests().len(), 3);
    let apps_tool_call = recorded_apps_tool_call_by_call_id(&server, calendar_call_id).await;

    assert_eq!(
        apps_tool_call.pointer(
            "/params/_meta/x-deepseek-vibing-turn-metadata/user_input_requested_during_turn"
        ),
        Some(&json!(true))
    );

    Ok(())
}
