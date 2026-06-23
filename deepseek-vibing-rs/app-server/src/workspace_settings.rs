use deepseek_vibing_auth::DeepSeekVibingAuth;
use deepseek_vibing_core::config::Config;

#[derive(Debug, Default)]
pub(crate) struct WorkspaceSettingsCache;

pub(crate) async fn deepseek_vibing_plugins_enabled_for_workspace(
    _config: &Config,
    _auth: Option<&DeepSeekVibingAuth>,
    _cache: Option<&WorkspaceSettingsCache>,
) -> anyhow::Result<bool> {
    Ok(true)
}
