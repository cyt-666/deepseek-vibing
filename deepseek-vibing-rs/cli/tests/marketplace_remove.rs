use anyhow::Result;
use deepseek_vibing_config::MarketplaceConfigUpdate;
use deepseek_vibing_config::record_user_marketplace;
use deepseek_vibing_core_plugins::installed_marketplaces::marketplace_install_root;
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

fn configured_marketplace_update() -> MarketplaceConfigUpdate<'static> {
    MarketplaceConfigUpdate {
        last_updated: "2026-04-13T00:00:00Z",
        last_revision: None,
        source_type: "git",
        source: "https://github.com/owner/repo.git",
        ref_name: Some("main"),
        sparse_paths: &[],
    }
}

fn write_installed_marketplace(deepseek_vibing_home: &Path, marketplace_name: &str) -> Result<()> {
    let root = marketplace_install_root(deepseek_vibing_home).join(marketplace_name);
    std::fs::create_dir_all(root.join(".agents/plugins"))?;
    std::fs::write(root.join(".agents/plugins/marketplace.json"), "{}")?;
    std::fs::write(root.join("marker.txt"), "installed")?;
    Ok(())
}

#[tokio::test]
async fn marketplace_remove_deletes_config_and_installed_root() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;
    record_user_marketplace(
        deepseek_vibing_home.path(),
        "debug",
        &configured_marketplace_update(),
    )?;
    write_installed_marketplace(deepseek_vibing_home.path(), "debug")?;

    deepseek_vibing_command(deepseek_vibing_home.path())?
        .args(["plugin", "marketplace", "remove", "debug"])
        .assert()
        .success()
        .stdout(contains("Removed marketplace `debug`."));

    let config_path = deepseek_vibing_home.path().join("config.toml");
    let config = std::fs::read_to_string(config_path)?;
    assert!(!config.contains("[marketplaces.debug]"));
    assert!(
        !marketplace_install_root(deepseek_vibing_home.path())
            .join("debug")
            .exists()
    );
    Ok(())
}

#[tokio::test]
async fn marketplace_remove_rejects_unknown_marketplace() -> Result<()> {
    let deepseek_vibing_home = TempDir::new()?;

    deepseek_vibing_command(deepseek_vibing_home.path())?
        .args(["plugin", "marketplace", "remove", "debug"])
        .assert()
        .failure()
        .stderr(contains(
            "marketplace `debug` is not configured or installed",
        ));

    Ok(())
}
