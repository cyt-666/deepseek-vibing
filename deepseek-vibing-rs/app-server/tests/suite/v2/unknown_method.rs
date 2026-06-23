use anyhow::Result;
use app_test_support::TestAppServer;
use deepseek_vibing_app_server_protocol::RequestId;
use tempfile::TempDir;
use tokio::time::Duration;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn account_and_auth_protocol_methods_are_unknown() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    let mut mcp = TestAppServer::new(deepseek_vibing_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    for method in [
        "account/login/start",
        "account/login/cancel",
        "account/logout",
        "account/read",
        "account/rateLimits/read",
        "account/sendAddCreditsNudgeEmail",
        "account/chatgptAuthTokens/refresh",
        "getAuthStatus",
    ] {
        let request_id = mcp.send_raw_request(method, None).await?;
        let error = timeout(
            DEFAULT_TIMEOUT,
            mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
        )
        .await??;
        assert_eq!(error.error.code, -32601, "method {method}");
    }

    Ok(())
}

#[tokio::test]
async fn removed_remote_plugin_share_methods_are_unknown() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    let mut mcp = TestAppServer::new(deepseek_vibing_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    for suffix in ["save", "updateTargets", "list", "checkout", "delete"] {
        assert_unknown_method(&mut mcp, &removed_plugin_share_method(suffix)).await?;
    }

    Ok(())
}

#[tokio::test]
async fn removed_thread_realtime_methods_are_unknown() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    let mut mcp = TestAppServer::new(deepseek_vibing_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    for suffix in ["start", "appendAudio", "appendText", "stop", "listVoices"] {
        assert_unknown_method(&mut mcp, &removed_thread_realtime_method(suffix)).await?;
    }

    Ok(())
}

async fn assert_unknown_method(mcp: &mut TestAppServer, method: &str) -> Result<()> {
    let request_id = mcp.send_raw_request(method, None).await?;
    let error = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert_eq!(error.error.code, -32601, "method {method}");
    Ok(())
}

fn removed_plugin_share_method(suffix: &str) -> String {
    ["plugin", "share", suffix].join("/")
}

fn removed_thread_realtime_method(suffix: &str) -> String {
    ["thread", "realtime", suffix].join("/")
}
