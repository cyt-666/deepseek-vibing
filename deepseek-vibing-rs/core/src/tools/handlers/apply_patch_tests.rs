use super::*;
use core_test_support::PathBufExt;
use core_test_support::PathExt;
use deepseek_vibing_apply_patch::MaybeApplyPatchVerified;
use deepseek_vibing_exec_server::LOCAL_FS;
use deepseek_vibing_protocol::models::PermissionProfile;
use deepseek_vibing_protocol::permissions::FileSystemSandboxPolicy;
use deepseek_vibing_protocol::protocol::AskForApproval;
use deepseek_vibing_protocol::protocol::FileChange;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use crate::session::tests::make_session_and_context;
use crate::tools::context::ToolInvocation;
use crate::tools::hook_names::HookToolName;
use crate::tools::registry::PostToolUsePayload;
use crate::tools::registry::PreToolUsePayload;
use crate::turn_diff_tracker::TurnDiffTracker;

fn sample_patch() -> &'static str {
    r#"*** Begin Patch
*** Add File: hello.txt
+hello
*** End Patch"#
}

async fn invocation_for_payload(payload: ToolPayload) -> ToolInvocation {
    let (session, turn) = make_session_and_context().await;
    invocation_from_session_turn_payload(session, turn, payload)
}

async fn invocation_for_payload_in_cwd(
    payload: ToolPayload,
    cwd: AbsolutePathBuf,
) -> ToolInvocation {
    let (session, mut turn) = make_session_and_context().await;
    turn.environments
        .turn_environments
        .first_mut()
        .expect("primary environment")
        .cwd = cwd;
    turn.approval_policy
        .set(AskForApproval::Never)
        .expect("test setup should allow updating approval policy");
    turn.permission_profile = PermissionProfile::Disabled;
    invocation_from_session_turn_payload(session, turn, payload)
}

fn invocation_from_session_turn_payload(
    session: crate::session::session::Session,
    turn: crate::session::turn_context::TurnContext,
    payload: ToolPayload,
) -> ToolInvocation {
    ToolInvocation {
        session: session.into(),
        turn: turn.into(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        tracker: Arc::new(Mutex::new(TurnDiffTracker::new())),
        call_id: "call-apply-patch".to_string(),
        tool_name: deepseek_vibing_tools::ToolName::plain("apply_patch"),
        source: crate::tools::context::ToolCallSource::Direct,
        payload,
    }
}

fn function_payload(patch: &str) -> ToolPayload {
    ToolPayload::Function {
        arguments: json!({ "patch": patch }).to_string(),
    }
}

#[tokio::test]
async fn pre_tool_use_payload_uses_freeform_patch_input() {
    let patch = sample_patch();
    let payload = ToolPayload::Custom {
        input: patch.to_string(),
    };
    let invocation = invocation_for_payload(payload).await;
    let handler = ApplyPatchHandler::default();

    assert_eq!(
        handler.pre_tool_use_payload(&invocation),
        Some(PreToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_input: json!({ "command": patch }),
        })
    );
}

#[tokio::test]
async fn post_tool_use_payload_uses_patch_input_and_tool_output() {
    let patch = sample_patch();
    let payload = ToolPayload::Custom {
        input: patch.to_string(),
    };
    let invocation = invocation_for_payload(payload).await;
    let output = ApplyPatchToolOutput::from_text("Success. Updated files.".to_string());
    let handler = ApplyPatchHandler::default();

    assert_eq!(
        handler.post_tool_use_payload(&invocation, &output),
        Some(PostToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_use_id: "call-apply-patch".to_string(),
            tool_input: json!({ "command": patch }),
            tool_response: json!("Success. Updated files."),
        })
    );
}

#[tokio::test]
async fn pre_tool_use_payload_uses_function_patch_argument() {
    let patch = sample_patch();
    let invocation = invocation_for_payload(function_payload(patch)).await;
    let handler = ApplyPatchHandler::new_function(/*multi_environment*/ false);

    assert_eq!(
        handler.pre_tool_use_payload(&invocation),
        Some(PreToolUsePayload {
            tool_name: HookToolName::apply_patch(),
            tool_input: json!({ "command": patch }),
        })
    );
}

#[tokio::test]
async fn function_payload_runs_apply_patch_against_turn_environment() {
    let tmp = TempDir::new().expect("tmp");
    let cwd = tmp.path().abs();
    let invocation = invocation_for_payload_in_cwd(function_payload(sample_patch()), cwd).await;
    let handler = ApplyPatchHandler::new_function(/*multi_environment*/ false);

    handler
        .handle(invocation)
        .await
        .expect("patch should apply");

    assert_eq!(
        std::fs::read_to_string(tmp.path().join("hello.txt")).expect("patched file"),
        "hello\n"
    );
}

#[test]
fn function_payload_rejects_empty_patch() {
    let handler = ApplyPatchHandler::new_function(/*multi_environment*/ false);
    let err = handler
        .parse_payload(function_payload(" \n\t"))
        .expect_err("empty patch should fail");

    assert_eq!(
        err,
        FunctionCallError::RespondToModel("apply_patch.patch must not be empty".to_string())
    );
}

#[test]
fn function_payload_rejects_environment_header_inside_patch() {
    let handler = ApplyPatchHandler::new_function(/*multi_environment*/ true);
    let err = handler
        .parse_payload(function_payload(
            "*** Begin Patch\n*** Environment ID: remote\n*** Add File: hello.txt\n+hello\n*** End Patch",
        ))
        .expect_err("environment header should fail");

    assert_eq!(
        err,
        FunctionCallError::RespondToModel(
            "apply_patch.patch must not include `*** Environment ID:`; pass `environment_id` as a JSON argument instead"
                .to_string(),
        )
    );
}

#[test]
fn function_payload_rejects_invalid_json() {
    let handler = ApplyPatchHandler::new_function(/*multi_environment*/ false);
    let err = handler
        .parse_payload(ToolPayload::Function {
            arguments: "{".to_string(),
        })
        .expect_err("invalid JSON should fail");

    assert!(
        err.to_string()
            .contains("failed to parse function arguments"),
        "{err}"
    );
}

#[test]
fn freeform_handler_rejects_function_payload() {
    let handler = ApplyPatchHandler::new_freeform(/*multi_environment*/ false);
    let err = handler
        .parse_payload(function_payload(sample_patch()))
        .expect_err("wrong shape should fail");

    assert_eq!(
        err,
        FunctionCallError::RespondToModel(
            "apply_patch handler received unsupported payload".to_string()
        )
    );
}

#[test]
fn diff_consumer_streams_apply_patch_changes() {
    let mut consumer = ApplyPatchArgumentDiffConsumer::default();
    assert!(
        consumer
            .push_delta("call-1".to_string(), "*** Begin Patch\n")
            .is_none()
    );

    let event = consumer
        .push_delta("call-1".to_string(), "*** Add File: hello.txt\n+hello")
        .expect("progress event");
    assert_eq!(
        (event.call_id, event.changes),
        (
            "call-1".to_string(),
            HashMap::from([(
                PathBuf::from("hello.txt"),
                FileChange::Add {
                    content: String::new(),
                },
            )]),
        )
    );

    assert!(
        consumer
            .push_delta("call-1".to_string(), "\n+world")
            .is_none()
    );
    assert!(
        consumer
            .push_delta("call-1".to_string(), "\n*** End Patch")
            .is_none()
    );

    let event = consumer
        .finish_update_on_complete()
        .expect("finish parser")
        .expect("progress event");
    assert_eq!(
        (event.call_id, event.changes),
        (
            "call-1".to_string(),
            HashMap::from([(
                PathBuf::from("hello.txt"),
                FileChange::Add {
                    content: "hello\nworld\n".to_string(),
                },
            )]),
        )
    );
}

#[test]
fn diff_consumer_streams_apply_patch_changes_with_environment_header() {
    let mut consumer = ApplyPatchArgumentDiffConsumer::default();
    assert!(
        consumer
            .push_delta(
                "call-1".to_string(),
                "*** Begin Patch\n*** Environment ID: remote\n",
            )
            .is_none()
    );

    let event = consumer
        .push_delta("call-1".to_string(), "*** Add File: hello.txt\n+hello")
        .expect("progress event");
    assert_eq!(
        event.changes,
        HashMap::from([(
            PathBuf::from("hello.txt"),
            FileChange::Add {
                content: String::new(),
            },
        )])
    );
}

#[test]
fn diff_consumer_sends_next_update_after_buffer_interval() {
    let mut consumer = ApplyPatchArgumentDiffConsumer::default();
    consumer.push_delta("call-1".to_string(), "*** Begin Patch\n");
    let first = consumer
        .push_delta("call-1".to_string(), "*** Add File: hello.txt\n+hello")
        .expect("first progress event");
    assert_eq!(
        first.changes,
        HashMap::from([(
            PathBuf::from("hello.txt"),
            FileChange::Add {
                content: String::new(),
            },
        )])
    );

    consumer.last_sent_at =
        Some(std::time::Instant::now() - APPLY_PATCH_ARGUMENT_DIFF_BUFFER_INTERVAL);
    let second = consumer
        .push_delta("call-1".to_string(), "\n+world")
        .expect("second progress event");
    assert_eq!(
        second.changes,
        HashMap::from([(
            PathBuf::from("hello.txt"),
            FileChange::Add {
                content: "hello\n".to_string(),
            },
        )])
    );
}

#[test]
fn reconcile_environment_id_requires_selection_when_enabled() {
    assert_eq!(
        require_environment_id(Some("remote"), /*allow_environment_id*/ false),
        Err(FunctionCallError::RespondToModel(
            "apply_patch environment selection is unavailable for this turn".to_string(),
        ))
    );
    assert_eq!(
        require_environment_id(
            /*parsed_environment_id*/ None, /*allow_environment_id*/ true
        ),
        Ok(None)
    );
}

#[tokio::test]
async fn approval_keys_include_move_destination() {
    let tmp = TempDir::new().expect("tmp");
    let cwd_path = tmp.path();
    let cwd = cwd_path.abs();
    std::fs::create_dir_all(cwd_path.join("old")).expect("create old dir");
    std::fs::create_dir_all(cwd_path.join("renamed/dir")).expect("create dest dir");
    std::fs::write(cwd_path.join("old/name.txt"), "old content\n").expect("write old file");
    let patch = r#"*** Begin Patch
*** Update File: old/name.txt
*** Move to: renamed/dir/name.txt
@@
-old content
+new content
*** End Patch"#;
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    let action = match deepseek_vibing_apply_patch::maybe_parse_apply_patch_verified(
        &argv,
        &cwd,
        LOCAL_FS.as_ref(),
        /*sandbox*/ None,
    )
    .await
    {
        MaybeApplyPatchVerified::Body(action) => action,
        other => panic!("expected patch body, got: {other:?}"),
    };

    let keys = file_paths_for_action(&action);
    assert_eq!(keys.len(), 2);
}

#[test]
fn write_permissions_for_paths_skip_dirs_already_writable_under_workspace_root() {
    let tmp = TempDir::new().expect("tmp");
    let cwd_path = tmp.path();
    let cwd = cwd_path.abs();
    let nested = cwd_path.join("nested");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    let file_path = AbsolutePathBuf::try_from(nested.join("file.txt"))
        .expect("nested file path should be absolute");
    let sandbox_policy = FileSystemSandboxPolicy::workspace_write(
        &[],
        /*exclude_tmpdir_env_var*/ true,
        /*exclude_slash_tmp*/ false,
    );

    let permissions = write_permissions_for_paths(&[file_path], &sandbox_policy, &cwd);

    assert_eq!(permissions, None);
}

#[test]
fn write_permissions_for_paths_keep_dirs_outside_workspace_root() {
    let tmp = TempDir::new().expect("tmp");
    let cwd = tmp.path().join("workspace");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&cwd).expect("create cwd");
    std::fs::create_dir_all(&outside).expect("create outside dir");
    let file_path = AbsolutePathBuf::try_from(outside.join("file.txt"))
        .expect("outside file path should be absolute");
    let cwd_abs = cwd.abs();
    let sandbox_policy = FileSystemSandboxPolicy::workspace_write(
        &[],
        /*exclude_tmpdir_env_var*/ true,
        /*exclude_slash_tmp*/ true,
    );

    let permissions = write_permissions_for_paths(&[file_path], &sandbox_policy, &cwd_abs);
    let expected_outside =
        dunce::simplified(&outside.canonicalize().expect("canonicalize outside dir")).abs();

    assert_eq!(
        permissions
            .and_then(|profile| profile.file_system)
            .and_then(|fs| fs.legacy_read_write_roots())
            .and_then(|(_read, write)| write),
        Some(vec![expected_outside])
    );
}
