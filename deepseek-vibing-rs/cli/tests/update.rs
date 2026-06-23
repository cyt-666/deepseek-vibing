use anyhow::Result;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

fn deepseek_vibing_command(deepseek_vibing_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(deepseek_vibing_utils_cargo_bin::cargo_bin(
        "deepseek-vibing",
    )?);
    cmd.env("DEEPSEEK_VIBING_HOME", deepseek_vibing_home);
    Ok(cmd)
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn update_does_not_start_interactive_prompt() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;

    deepseek_vibing_command(deepseek_vibing_home.path())?
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(
            "`deepseek-vibing update` is not available in debug builds",
        ));

    Ok(())
}
