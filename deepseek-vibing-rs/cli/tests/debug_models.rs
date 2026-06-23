use std::path::Path;

use anyhow::Result;
use tempfile::TempDir;

fn deepseek_vibing_command(deepseek_vibing_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(deepseek_vibing_utils_cargo_bin::cargo_bin(
        "deepseek-vibing",
    )?);
    cmd.env("DEEPSEEK_VIBING_HOME", deepseek_vibing_home);
    Ok(cmd)
}

#[test]
fn debug_models_bundled_prints_json() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    let mut cmd = deepseek_vibing_command(deepseek_vibing_home.path())?;
    let output = cmd.args(["debug", "models", "--bundled"]).output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    assert!(value["models"].is_array());
    assert!(!value["models"].as_array().unwrap_or(&Vec::new()).is_empty());

    Ok(())
}

#[test]
fn debug_models_default_prints_json_without_auth() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    let mut cmd = deepseek_vibing_command(deepseek_vibing_home.path())?;
    let output = cmd.args(["debug", "models"]).output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let value: serde_json::Value = serde_json::from_str(&stdout)?;
    assert!(value["models"].is_array());
    assert!(!value["models"].as_array().unwrap_or(&Vec::new()).is_empty());

    Ok(())
}
