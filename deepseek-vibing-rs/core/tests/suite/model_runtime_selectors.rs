use anyhow::Result;
use core_test_support::responses;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_models_once;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::skip_if_no_network;
use core_test_support::submit_thread_settings;
use core_test_support::test_deepseek_vibing::test_deepseek_vibing;
use core_test_support::wait_for_event;
use deepseek_vibing_auth::DeepSeekVibingAuth;
use deepseek_vibing_core::config::Config;
use deepseek_vibing_models_manager::manager::RefreshStrategy;
use deepseek_vibing_models_manager::manager::SharedModelsManager;
use deepseek_vibing_models_manager::model_info::model_info_from_slug;
use deepseek_vibing_protocol::openai_models::ModelInfo;
use deepseek_vibing_protocol::openai_models::ModelPreset;
use deepseek_vibing_protocol::openai_models::ModelVisibility;
use deepseek_vibing_protocol::openai_models::ModelsResponse;
use deepseek_vibing_protocol::openai_models::ToolMode;
use deepseek_vibing_protocol::protocol::EventMsg;
use deepseek_vibing_protocol::protocol::Op;
use deepseek_vibing_protocol::protocol::ThreadSettingsOverrides;
use deepseek_vibing_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;

fn remote_model(slug: &str) -> ModelInfo {
    ModelInfo {
        visibility: ModelVisibility::List,
        used_fallback_model_metadata: false,
        ..model_info_from_slug(slug)
    }
}

fn tool_names(body: &Value) -> Vec<String> {
    body.get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| {
                    tool.get("name")
                        .or_else(|| tool.get("type"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn wait_for_model_available(manager: &SharedModelsManager, slug: &str) -> ModelPreset {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(model) = manager
            .list_models(RefreshStrategy::Online)
            .await
            .iter()
            .find(|model| model.model == slug)
            .cloned()
        {
            return model;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for the remote model {slug} to appear");
        }
        sleep(Duration::from_millis(25)).await;
    }
}

async fn response_body_for_remote_model(
    remote_model: ModelInfo,
    configure: impl FnOnce(&mut Config) + Send + 'static,
) -> Result<Value> {
    let server = responses::start_mock_server().await;
    let model_slug = remote_model.slug.clone();
    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: vec![remote_model],
        },
    )
    .await;
    let response_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let mut builder = test_deepseek_vibing()
        .with_auth(DeepSeekVibingAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(configure);
    let test = builder.build(&server).await?;
    let models_manager = test.thread_manager.get_models_manager();
    let available_model = wait_for_model_available(&models_manager, &model_slug).await;
    assert_eq!(available_model.model, model_slug);
    assert_eq!(models_mock.requests().len(), 1);

    submit_thread_settings(
        &test.deepseek_vibing,
        ThreadSettingsOverrides {
            model: Some(model_slug),
            ..Default::default()
        },
    )
    .await?;
    test.deepseek_vibing
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "list tools".into(),
                text_elements: Vec::new(),
            }],
            environments: None,
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: Default::default(),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event(&test.deepseek_vibing, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    Ok(response_mock.single_request().body_json())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_direct_tool_mode_selector_is_accepted() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let mut direct_model = remote_model("test-tool-mode-direct");
    direct_model.tool_mode = Some(ToolMode::Direct);
    let direct_body = response_body_for_remote_model(direct_model, |_| {}).await?;
    let direct_tools = tool_names(&direct_body);
    assert!(!direct_tools.is_empty());

    Ok(())
}
