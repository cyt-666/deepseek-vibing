use std::path::Path;

use anyhow::Result;
use predicates::str::contains;
use tempfile::TempDir;

fn deepseek_vibing_command(deepseek_vibing_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(deepseek_vibing_utils_cargo_bin::cargo_bin(
        "deepseek-vibing",
    )?);
    cmd.env("DEEPSEEK_VIBING_HOME", deepseek_vibing_home);
    Ok(cmd)
}

#[test]
fn strict_config_rejects_unknown_config_fields_for_app_server() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    std::fs::write(
        deepseek_vibing_home.path().join("config.toml"),
        r#"
foo = "bar"
"#,
    )?;

    let mut cmd = deepseek_vibing_command(deepseek_vibing_home.path())?;
    cmd.args(["app-server", "--strict-config", "--listen", "off"])
        .assert()
        .failure()
        .stderr(contains("unknown configuration field"));

    Ok(())
}
