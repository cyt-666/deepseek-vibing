//! DeepSeekVibing Apps support for the host-owned apps MCP server.
//!
//! This module owns the pieces that are unique to ChatGPT-hosted app
//! connectors: cache scoping by authenticated user, disk cache reads/writes,
//! connector allow-list filtering, and the normalization that turns app
//! connector/tool metadata into model-visible MCP callable names.

use std::path::PathBuf;
use std::time::Instant;

use crate::mcp::DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME;
use crate::runtime::emit_duration;
use crate::tools::MCP_TOOLS_CACHE_WRITE_DURATION_METRIC;
use crate::tools::ToolInfo;
use anyhow::Context;
use deepseek_vibing_auth::DeepSeekVibingAuth;
use deepseek_vibing_protocol::mcp::McpServerInfo;
use deepseek_vibing_utils_plugins::mcp_connector::is_connector_id_allowed;
use deepseek_vibing_utils_plugins::mcp_connector::sanitize_name;
use serde::Deserialize;
use serde::Serialize;
use sha1::Digest;
use sha1::Sha1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepSeekVibingAppsToolsCacheKey {
    pub(crate) account_id: Option<String>,
    pub(crate) chatgpt_user_id: Option<String>,
    pub(crate) is_workspace_account: bool,
}

pub fn deepseek_vibing_apps_tools_cache_key(
    auth: Option<&DeepSeekVibingAuth>,
) -> DeepSeekVibingAppsToolsCacheKey {
    DeepSeekVibingAppsToolsCacheKey {
        account_id: auth.and_then(DeepSeekVibingAuth::get_account_id),
        chatgpt_user_id: auth.and_then(DeepSeekVibingAuth::get_chatgpt_user_id),
        is_workspace_account: auth.is_some_and(DeepSeekVibingAuth::is_workspace_account),
    }
}

#[derive(Clone)]
pub(crate) struct DeepSeekVibingAppsToolsCacheContext {
    pub(crate) deepseek_vibing_home: PathBuf,
    pub(crate) user_key: DeepSeekVibingAppsToolsCacheKey,
}

impl DeepSeekVibingAppsToolsCacheContext {
    pub(crate) fn tools_cache_path(&self) -> PathBuf {
        self.cache_path_in(DEEPSEEK_VIBING_APPS_TOOLS_CACHE_DIR)
    }

    pub(crate) fn server_info_cache_path(&self) -> PathBuf {
        self.cache_path_in(DEEPSEEK_VIBING_APPS_SERVER_INFO_CACHE_DIR)
    }

    fn cache_path_in(&self, cache_dir: &str) -> PathBuf {
        let user_key_json = serde_json::to_string(&self.user_key).unwrap_or_default();
        let user_key_hash = sha1_hex(&user_key_json);
        self.deepseek_vibing_home
            .join(cache_dir)
            .join(format!("{user_key_hash}.json"))
    }
}

pub(crate) enum CachedDeepSeekVibingAppsToolsLoad {
    Hit(Vec<ToolInfo>),
    Missing,
    Invalid,
}

pub(crate) fn normalize_deepseek_vibing_apps_tool_title(
    server_name: &str,
    connector_name: Option<&str>,
    value: &str,
) -> String {
    if server_name != DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME {
        return value.to_string();
    }

    let Some(connector_name) = connector_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return value.to_string();
    };

    let prefix = format!("{connector_name}_");
    if let Some(stripped) = value.strip_prefix(&prefix)
        && !stripped.is_empty()
    {
        return stripped.to_string();
    }

    value.to_string()
}

pub(crate) fn normalize_deepseek_vibing_apps_callable_name(
    server_name: &str,
    tool_name: &str,
    connector_id: Option<&str>,
    connector_name: Option<&str>,
) -> String {
    if server_name != DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME {
        return tool_name.to_string();
    }

    let tool_name = sanitize_name(tool_name);

    if let Some(connector_name) = connector_name
        .map(str::trim)
        .map(sanitize_name)
        .filter(|name| !name.is_empty())
        && let Some(stripped) = tool_name.strip_prefix(&connector_name)
        && !stripped.is_empty()
    {
        return stripped.to_string();
    }

    if let Some(connector_id) = connector_id
        .map(str::trim)
        .map(sanitize_name)
        .filter(|name| !name.is_empty())
        && let Some(stripped) = tool_name.strip_prefix(&connector_id)
        && !stripped.is_empty()
    {
        return stripped.to_string();
    }

    tool_name
}

pub(crate) fn normalize_deepseek_vibing_apps_callable_namespace(
    server_name: &str,
    connector_name: Option<&str>,
) -> String {
    if server_name == DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME
        && let Some(connector_name) = connector_name
    {
        format!("{}__{}", server_name, sanitize_name(connector_name))
    } else {
        server_name.to_string()
    }
}

pub(crate) fn write_cached_deepseek_vibing_apps_tools_if_needed(
    server_name: &str,
    cache_context: Option<&DeepSeekVibingAppsToolsCacheContext>,
    server_info: &McpServerInfo,
    tools: &[ToolInfo],
) {
    if server_name != DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME {
        return;
    }

    if let Some(cache_context) = cache_context {
        let cache_write_start = Instant::now();
        write_cached_deepseek_vibing_apps_tools(cache_context, tools);
        if let Err(err) = write_cached_deepseek_vibing_apps_server_info(cache_context, server_info)
        {
            tracing::warn!("failed to write DeepSeekVibing Apps server info cache: {err:#}");
        }
        emit_duration(
            MCP_TOOLS_CACHE_WRITE_DURATION_METRIC,
            cache_write_start.elapsed(),
            &[],
        );
    }
}

pub(crate) fn load_startup_cached_deepseek_vibing_apps_tools_snapshot(
    server_name: &str,
    cache_context: Option<&DeepSeekVibingAppsToolsCacheContext>,
) -> Option<Vec<ToolInfo>> {
    if server_name != DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME {
        return None;
    }

    let cache_context = cache_context?;

    match load_cached_deepseek_vibing_apps_tools(cache_context) {
        CachedDeepSeekVibingAppsToolsLoad::Hit(tools) => Some(tools),
        CachedDeepSeekVibingAppsToolsLoad::Missing | CachedDeepSeekVibingAppsToolsLoad::Invalid => {
            None
        }
    }
}

pub(crate) fn load_startup_cached_deepseek_vibing_apps_server_info(
    server_name: &str,
    cache_context: Option<&DeepSeekVibingAppsToolsCacheContext>,
) -> Option<McpServerInfo> {
    if server_name != DEEPSEEK_VIBING_APPS_MCP_SERVER_NAME {
        return None;
    }

    load_cached_deepseek_vibing_apps_server_info(cache_context?)
}

#[cfg(test)]
pub(crate) fn read_cached_deepseek_vibing_apps_tools(
    cache_context: &DeepSeekVibingAppsToolsCacheContext,
) -> Option<Vec<ToolInfo>> {
    match load_cached_deepseek_vibing_apps_tools(cache_context) {
        CachedDeepSeekVibingAppsToolsLoad::Hit(tools) => Some(tools),
        CachedDeepSeekVibingAppsToolsLoad::Missing | CachedDeepSeekVibingAppsToolsLoad::Invalid => {
            None
        }
    }
}

pub(crate) fn load_cached_deepseek_vibing_apps_tools(
    cache_context: &DeepSeekVibingAppsToolsCacheContext,
) -> CachedDeepSeekVibingAppsToolsLoad {
    let cache_path = cache_context.tools_cache_path();
    let bytes = match std::fs::read(cache_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return CachedDeepSeekVibingAppsToolsLoad::Missing;
        }
        Err(_) => return CachedDeepSeekVibingAppsToolsLoad::Invalid,
    };
    let cache: DeepSeekVibingAppsToolsDiskCache = match serde_json::from_slice(&bytes) {
        Ok(cache) => cache,
        Err(_) => return CachedDeepSeekVibingAppsToolsLoad::Invalid,
    };
    if cache.schema_version != DEEPSEEK_VIBING_APPS_TOOLS_CACHE_SCHEMA_VERSION {
        return CachedDeepSeekVibingAppsToolsLoad::Invalid;
    }
    CachedDeepSeekVibingAppsToolsLoad::Hit(filter_disallowed_deepseek_vibing_apps_tools(
        cache.tools,
    ))
}

pub(crate) fn write_cached_deepseek_vibing_apps_tools(
    cache_context: &DeepSeekVibingAppsToolsCacheContext,
    tools: &[ToolInfo],
) {
    let cache_path = cache_context.tools_cache_path();
    if let Some(parent) = cache_path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let tools = filter_disallowed_deepseek_vibing_apps_tools(tools.to_vec());
    let Ok(bytes) = serde_json::to_vec_pretty(&DeepSeekVibingAppsToolsDiskCache {
        schema_version: DEEPSEEK_VIBING_APPS_TOOLS_CACHE_SCHEMA_VERSION,
        tools,
    }) else {
        return;
    };
    let _ = std::fs::write(cache_path, bytes);
}

pub(crate) fn load_cached_deepseek_vibing_apps_server_info(
    cache_context: &DeepSeekVibingAppsToolsCacheContext,
) -> Option<McpServerInfo> {
    let bytes = std::fs::read(cache_context.server_info_cache_path()).ok()?;
    let cache: DeepSeekVibingAppsServerInfoDiskCache = serde_json::from_slice(&bytes).ok()?;
    (cache.schema_version == DEEPSEEK_VIBING_APPS_SERVER_INFO_CACHE_SCHEMA_VERSION)
        .then_some(cache.server_info)
}

fn write_cached_deepseek_vibing_apps_server_info(
    cache_context: &DeepSeekVibingAppsToolsCacheContext,
    server_info: &McpServerInfo,
) -> anyhow::Result<()> {
    let cache_path = cache_context.server_info_cache_path();
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create DeepSeekVibing Apps server info cache directory `{}`",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(&DeepSeekVibingAppsServerInfoDiskCache {
        schema_version: DEEPSEEK_VIBING_APPS_SERVER_INFO_CACHE_SCHEMA_VERSION,
        server_info: server_info.clone(),
    })
    .context("failed to serialize DeepSeekVibing Apps server info cache")?;
    std::fs::write(&cache_path, bytes).with_context(|| {
        format!(
            "failed to write DeepSeekVibing Apps server info cache `{}`",
            cache_path.display()
        )
    })?;
    Ok(())
}

pub(crate) fn filter_disallowed_deepseek_vibing_apps_tools(tools: Vec<ToolInfo>) -> Vec<ToolInfo> {
    tools
        .into_iter()
        .filter(|tool| {
            tool.connector_id
                .as_deref()
                .is_none_or(is_connector_id_allowed)
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeepSeekVibingAppsToolsDiskCache {
    schema_version: u8,
    tools: Vec<ToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeepSeekVibingAppsServerInfoDiskCache {
    schema_version: u8,
    server_info: McpServerInfo,
}

const DEEPSEEK_VIBING_APPS_TOOLS_CACHE_DIR: &str = "cache/deepseek_vibing_apps_tools";
pub(crate) const DEEPSEEK_VIBING_APPS_TOOLS_CACHE_SCHEMA_VERSION: u8 = 3;

const DEEPSEEK_VIBING_APPS_SERVER_INFO_CACHE_DIR: &str = "cache/deepseek_vibing_apps_server_info";
const DEEPSEEK_VIBING_APPS_SERVER_INFO_CACHE_SCHEMA_VERSION: u8 = 1;

fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    let sha1 = hasher.finalize();
    format!("{sha1:x}")
}
