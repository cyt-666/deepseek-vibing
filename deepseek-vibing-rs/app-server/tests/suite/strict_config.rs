use std::process::Command;

use anyhow::Result;
use tempfile::TempDir;

#[test]
fn strict_config_rejects_unknown_config_fields_for_standalone_app_server() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    std::fs::write(
        deepseek_vibing_home.path().join("config.toml"),
        r#"
foo = "bar"
"#,
    )?;

    let output = Command::new(deepseek_vibing_utils_cargo_bin::cargo_bin(
        "deepseek-vibing-app-server",
    )?)
    .env("DEEPSEEK_VIBING_HOME", deepseek_vibing_home.path())
    .env(
        "DEEPSEEK_VIBING_APP_SERVER_MANAGED_CONFIG_PATH",
        deepseek_vibing_home.path().join("managed_config.toml"),
    )
    .args(["--strict-config", "--listen", "off"])
    .output()?;

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("unknown configuration field `foo`"),
        "expected strict config error in stderr, got: {stderr}"
    );

    Ok(())
}
