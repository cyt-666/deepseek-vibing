use anyhow::Context;
use deepseek_vibing_plugin::PluginCapabilitySummary;
use std::collections::HashSet;
use tracing::warn;

use crate::OPENAI_BUNDLED_MARKETPLACE_NAME;
use crate::OPENAI_CURATED_MARKETPLACE_NAME;
use crate::PluginsConfigInput;
use crate::PluginsManager;
use crate::marketplace::MarketplacePluginInstallPolicy;

const TOOL_SUGGEST_DISCOVERABLE_PLUGIN_ALLOWLIST: &[&str] = &[
    "github@openai-curated",
    "notion@openai-curated",
    "slack@openai-curated",
    "gmail@openai-curated",
    "google-calendar@openai-curated",
    "google-drive@openai-curated",
    "openai-developers@openai-curated",
    "canva@openai-curated",
    "teams@openai-curated",
    "sharepoint@openai-curated",
    "outlook-email@openai-curated",
    "outlook-calendar@openai-curated",
    "linear@openai-curated",
    "figma@openai-curated",
    "chrome@openai-bundled",
    "computer-use@openai-bundled",
];

const TOOL_SUGGEST_DISCOVERABLE_MARKETPLACE_ALLOWLIST: &[&str] = &[
    OPENAI_BUNDLED_MARKETPLACE_NAME,
    OPENAI_CURATED_MARKETPLACE_NAME,
];

#[derive(Debug, Clone)]
pub struct ToolSuggestPluginDiscoveryInput {
    pub plugins: PluginsConfigInput,
    pub configured_plugin_ids: HashSet<String>,
    pub disabled_plugin_ids: HashSet<String>,
    pub loaded_plugin_app_connector_ids: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSuggestDiscoverablePlugin {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub has_skills: bool,
    pub mcp_server_names: Vec<String>,
    pub app_connector_ids: Vec<String>,
}

impl PluginsManager {
    pub async fn list_tool_suggest_discoverable_plugins(
        &self,
        input: &ToolSuggestPluginDiscoveryInput,
        _auth: Option<&deepseek_vibing_auth::DeepSeekVibingAuth>,
    ) -> anyhow::Result<Vec<ToolSuggestDiscoverablePlugin>> {
        if !input.plugins.plugins_enabled {
            return Ok(Vec::new());
        }

        let marketplaces = self
            .list_marketplaces_for_config(&input.plugins, &[])
            .context("failed to list plugin marketplaces for tool suggestions")?
            .marketplaces;
        let mut installed_app_connector_ids = self
            .plugins_for_config(&input.plugins)
            .await
            .capability_summaries()
            .iter()
            .flat_map(|plugin| plugin.app_connector_ids.iter())
            .map(|connector_id| connector_id.0.clone())
            .collect::<HashSet<_>>();
        installed_app_connector_ids.extend(input.loaded_plugin_app_connector_ids.iter().cloned());
        let mut discoverable_plugins = Vec::<ToolSuggestDiscoverablePlugin>::new();
        for marketplace in marketplaces {
            let marketplace_name = marketplace.name;
            let is_allowlisted_marketplace = TOOL_SUGGEST_DISCOVERABLE_MARKETPLACE_ALLOWLIST
                .contains(&marketplace_name.as_str());

            for plugin in marketplace.plugins {
                let is_configured_plugin = input.configured_plugin_ids.contains(plugin.id.as_str());
                let is_fallback_plugin =
                    TOOL_SUGGEST_DISCOVERABLE_PLUGIN_ALLOWLIST.contains(&plugin.id.as_str());
                if plugin.installed
                    || plugin.policy.installation == MarketplacePluginInstallPolicy::NotAvailable
                    || input.disabled_plugin_ids.contains(plugin.id.as_str())
                    || (!is_allowlisted_marketplace && !is_configured_plugin)
                {
                    continue;
                }

                let plugin_id = plugin.id.clone();

                match self
                    .read_plugin_detail_for_marketplace_plugin(
                        &input.plugins,
                        &marketplace_name,
                        plugin,
                    )
                    .await
                {
                    Ok(plugin) => {
                        let plugin: PluginCapabilitySummary = plugin.into();
                        let matches_installed_app =
                            plugin.app_connector_ids.iter().any(|connector_id| {
                                installed_app_connector_ids.contains(connector_id.0.as_str())
                            });
                        if !is_configured_plugin && !is_fallback_plugin && !matches_installed_app {
                            continue;
                        }

                        discoverable_plugins.push(ToolSuggestDiscoverablePlugin {
                            id: plugin.config_name,
                            name: plugin.display_name,
                            description: plugin.description,
                            has_skills: plugin.has_skills,
                            mcp_server_names: plugin.mcp_server_names,
                            app_connector_ids: plugin
                                .app_connector_ids
                                .into_iter()
                                .map(|connector_id| connector_id.0)
                                .collect(),
                        });
                    }
                    Err(err) => {
                        warn!("failed to load discoverable plugin suggestion {plugin_id}: {err:#}")
                    }
                }
            }
        }
        discoverable_plugins.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(discoverable_plugins)
    }
}
