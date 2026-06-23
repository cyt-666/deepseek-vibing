use deepseek_vibing_app_server_protocol::AppInfo;
use deepseek_vibing_core::config::Config;
use deepseek_vibing_core::connectors::AccessibleConnectorsStatus;
use deepseek_vibing_exec_server::EnvironmentManager;
use std::sync::Arc;

pub(crate) async fn list_cached_accessible_connectors_from_mcp_tools(
    config: &Config,
) -> Option<Vec<AppInfo>> {
    deepseek_vibing_core::connectors::list_cached_accessible_connectors_from_mcp_tools(config).await
}

pub(crate) async fn list_accessible_connectors_from_mcp_tools_with_environment_manager(
    config: &Config,
    force_refetch: bool,
    environment_manager: Arc<EnvironmentManager>,
) -> anyhow::Result<AccessibleConnectorsStatus> {
    deepseek_vibing_core::connectors::list_accessible_connectors_from_mcp_tools_with_environment_manager(
        config,
        force_refetch,
        environment_manager,
    )
    .await
}

pub(crate) async fn list_cached_all_connectors(_config: &Config) -> Option<Vec<AppInfo>> {
    Some(Vec::new())
}

pub(crate) async fn list_all_connectors_with_options(
    _config: &Config,
    _force_refetch: bool,
) -> anyhow::Result<Vec<AppInfo>> {
    Ok(Vec::new())
}

pub(crate) fn with_app_enabled_state(connectors: Vec<AppInfo>, config: &Config) -> Vec<AppInfo> {
    deepseek_vibing_core::connectors::with_app_enabled_state(connectors, config)
}

pub(crate) fn merge_connectors_with_accessible(
    all: Vec<AppInfo>,
    accessible: Vec<AppInfo>,
    all_connectors_loaded: bool,
) -> Vec<AppInfo> {
    if all_connectors_loaded {
        let mut merged = all;
        for connector in accessible {
            if !merged.iter().any(|existing| existing.id == connector.id) {
                merged.push(connector);
            }
        }
        merged
    } else {
        accessible
    }
}

pub(crate) fn connectors_for_plugin_apps(
    connectors: Vec<AppInfo>,
    plugin_apps: &[deepseek_vibing_plugin::AppConnectorId],
) -> Vec<AppInfo> {
    connectors
        .into_iter()
        .filter(|connector| {
            plugin_apps
                .iter()
                .any(|app| app.0.as_str() == connector.id.as_str())
        })
        .collect()
}
