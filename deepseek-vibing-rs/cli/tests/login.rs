use std::path::Path;

use anyhow::Result;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;

fn deepseek_vibing_command(deepseek_vibing_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(deepseek_vibing_utils_cargo_bin::cargo_bin(
        "deepseek-vibing",
    )?);
    cmd.env("DEEPSEEK_VIBING_HOME", deepseek_vibing_home);
    Ok(cmd)
}

fn write_file_auth_config(deepseek_vibing_home: &Path) -> Result<()> {
    std::fs::write(
        deepseek_vibing_home.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    )?;
    Ok(())
}

fn read_auth_json(deepseek_vibing_home: &Path) -> Result<Value> {
    let auth_json = std::fs::read_to_string(deepseek_vibing_home.join("auth.json"))?;
    Ok(serde_json::from_str(&auth_json)?)
}

#[test]
fn login_with_api_key_reads_stdin_and_writes_auth_json() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    write_file_auth_config(deepseek_vibing_home.path())?;

    let mut cmd = deepseek_vibing_command(deepseek_vibing_home.path())?;
    cmd.args([
        "-c",
        "forced_login_method=\"api\"",
        "login",
        "--with-api-key",
    ])
    .write_stdin("sk-test\n")
    .assert()
    .success()
    .stderr(contains("Successfully logged in"));

    let auth = read_auth_json(deepseek_vibing_home.path())?;
    assert_eq!(auth["OPENAI_API_KEY"], "sk-test");
    assert!(auth.get("tokens").is_none());
    assert!(auth.get("agent_identity").is_none());

    Ok(())
}

#[test]
fn login_with_access_token_rejects_invalid_jwt() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    write_file_auth_config(deepseek_vibing_home.path())?;

    let mut cmd = deepseek_vibing_command(deepseek_vibing_home.path())?;
    cmd.args(["login", "--with-access-token"])
        .write_stdin("not-a-jwt\n")
        .assert()
        .failure()
        .stderr(contains("Error logging in with access token"));

    Ok(())
}
