use anyhow::Result;
use deepseek_vibing_core::build_prompt_input;
use deepseek_vibing_core::config::ConfigBuilder;
use deepseek_vibing_core::config::ConfigOverrides;
use deepseek_vibing_protocol::models::ContentItem;
use deepseek_vibing_protocol::models::ResponseItem;
use deepseek_vibing_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

#[tokio::test]
async fn build_prompt_input_includes_context_and_user_message() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    let cwd = TempDir::new()?;
    let mut config = ConfigBuilder::default()
        .deepseek_vibing_home(deepseek_vibing_home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            deepseek_vibing_self_exe: Some(std::env::current_exe()?),
            ..ConfigOverrides::default()
        })
        .build()
        .await?;
    config.user_instructions = Some("Project-specific test instructions".to_string());

    let input = build_prompt_input(
        config,
        vec![UserInput::Text {
            text: "hello from debug prompt".to_string(),
            text_elements: Vec::new(),
        }],
        /*state_db*/ None,
    )
    .await?;

    let expected_user_message = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "hello from debug prompt".to_string(),
        }],
        phase: None,
    };
    assert_eq!(input.last(), Some(&expected_user_message));
    assert!(input.iter().any(|item| {
        let ResponseItem::Message { content, .. } = item else {
            return false;
        };

        content.iter().any(|content_item| {
            let (ContentItem::InputText { text } | ContentItem::OutputText { text }) = content_item
            else {
                return false;
            };
            text.contains("Project-specific test instructions")
        })
    }));

    Ok(())
}
