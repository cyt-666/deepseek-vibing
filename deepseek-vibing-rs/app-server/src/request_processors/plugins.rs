use super::*;
use crate::error_code::internal_error;
use crate::error_code::invalid_request;
use deepseek_vibing_app_server_protocol::PluginAvailability;
use deepseek_vibing_config::types::McpServerConfig;
use deepseek_vibing_mcp::McpOAuthLoginSupport;
use deepseek_vibing_mcp::oauth_login_support;
use deepseek_vibing_mcp::should_retry_without_scopes;
use deepseek_vibing_rmcp_client::perform_oauth_login_silent;

#[derive(Clone)]
pub(crate) struct PluginRequestProcessor {
    auth_manager: Arc<AuthManager>,
    thread_manager: Arc<ThreadManager>,
    outgoing: Arc<OutgoingMessageSender>,
    config_manager: ConfigManager,
    workspace_settings_cache: Arc<workspace_settings::WorkspaceSettingsCache>,
}

fn plugin_skills_to_info(
    skills: &[deepseek_vibing_core::skills::SkillMetadata],
    disabled_skill_paths: &HashSet<AbsolutePathBuf>,
) -> Vec<SkillSummary> {
    skills
        .iter()
        .map(|skill| SkillSummary {
            name: skill.name.clone(),
            description: skill.description.clone(),
            short_description: skill.short_description.clone(),
            interface: skill.interface.clone().map(|interface| {
                deepseek_vibing_app_server_protocol::SkillInterface {
                    display_name: interface.display_name,
                    short_description: interface.short_description,
                    icon_small: interface.icon_small,
                    icon_large: interface.icon_large,
                    brand_color: interface.brand_color,
                    default_prompt: interface.default_prompt,
                }
            }),
            path: Some(skill.path_to_skills_md.clone()),
            enabled: !disabled_skill_paths.contains(&skill.path_to_skills_md),
        })
        .collect()
}

fn local_plugin_interface_to_info(interface: PluginManifestInterface) -> PluginInterface {
    PluginInterface {
        display_name: interface.display_name,
        short_description: interface.short_description,
        long_description: interface.long_description,
        developer_name: interface.developer_name,
        category: interface.category,
        capabilities: interface.capabilities,
        website_url: interface.website_url,
        privacy_policy_url: interface.privacy_policy_url,
        terms_of_service_url: interface.terms_of_service_url,
        default_prompt: interface.default_prompt,
        brand_color: interface.brand_color,
        composer_icon: interface.composer_icon,
        composer_icon_url: None,
        logo: interface.logo,
        logo_url: None,
        screenshots: interface.screenshots,
        screenshot_urls: Vec::new(),
    }
}

fn marketplace_plugin_source_to_info(source: MarketplacePluginSource) -> PluginSource {
    match source {
        MarketplacePluginSource::Local { path } => PluginSource::Local { path },
        MarketplacePluginSource::Git {
            url,
            path,
            ref_name,
            sha,
        } => PluginSource::Git {
            url,
            path,
            ref_name,
            sha,
        },
    }
}

fn convert_configured_marketplace_plugin_to_plugin_summary(
    plugin: deepseek_vibing_core_plugins::ConfiguredMarketplacePlugin,
) -> PluginSummary {
    PluginSummary {
        id: plugin.id,
        remote_plugin_id: None,
        local_version: plugin.local_version,
        installed: plugin.installed,
        enabled: plugin.enabled,
        name: plugin.name,
        source: marketplace_plugin_source_to_info(plugin.source),
        install_policy: plugin.policy.installation.into(),
        auth_policy: plugin.policy.authentication.into(),
        availability: PluginAvailability::Available,
        interface: plugin.interface.map(local_plugin_interface_to_info),
        keywords: plugin.keywords,
    }
}

impl PluginRequestProcessor {
    pub(crate) fn new(
        auth_manager: Arc<AuthManager>,
        thread_manager: Arc<ThreadManager>,
        outgoing: Arc<OutgoingMessageSender>,
        _analytics_events_client: AnalyticsEventsClient,
        config_manager: ConfigManager,
        workspace_settings_cache: Arc<workspace_settings::WorkspaceSettingsCache>,
    ) -> Self {
        Self {
            auth_manager,
            thread_manager,
            outgoing,
            config_manager,
            workspace_settings_cache,
        }
    }

    pub(crate) async fn plugin_list(
        &self,
        params: PluginListParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.plugin_list_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn plugin_installed(
        &self,
        params: PluginInstalledParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.plugin_installed_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn plugin_read(
        &self,
        params: PluginReadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.plugin_read_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn plugin_skill_read(
        &self,
        params: PluginSkillReadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.plugin_skill_read_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn plugin_install(
        &self,
        params: PluginInstallParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.plugin_install_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) async fn plugin_uninstall(
        &self,
        params: PluginUninstallParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.plugin_uninstall_response(params)
            .await
            .map(|response| Some(response.into()))
    }

    pub(crate) fn effective_plugins_changed_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        let thread_manager = Arc::clone(&self.thread_manager);
        let config_manager = self.config_manager.clone();
        Arc::new(move || {
            Self::spawn_effective_plugins_changed_task(
                Arc::clone(&thread_manager),
                config_manager.clone(),
            );
        })
    }

    fn on_effective_plugins_changed(&self) {
        Self::spawn_effective_plugins_changed_task(
            Arc::clone(&self.thread_manager),
            self.config_manager.clone(),
        );
    }

    fn spawn_effective_plugins_changed_task(
        thread_manager: Arc<ThreadManager>,
        config_manager: ConfigManager,
    ) {
        tokio::spawn(async move {
            thread_manager.plugins_manager().clear_cache();
            thread_manager.skills_manager().clear_cache();
            if thread_manager.list_thread_ids().await.is_empty() {
                return;
            }
            crate::mcp_refresh::queue_best_effort_refresh(&thread_manager, &config_manager).await;
        });
    }

    fn clear_plugin_related_caches(&self) {
        self.thread_manager.plugins_manager().clear_cache();
        self.thread_manager.skills_manager().clear_cache();
    }

    async fn load_latest_config(
        &self,
        fallback_cwd: Option<PathBuf>,
    ) -> Result<Config, JSONRPCErrorError> {
        self.config_manager
            .load_latest_config(fallback_cwd)
            .await
            .map_err(|err| internal_error(format!("failed to reload config: {err}")))
    }

    async fn workspace_deepseek_vibing_plugins_enabled(
        &self,
        config: &Config,
        auth: Option<&DeepSeekVibingAuth>,
    ) -> bool {
        match workspace_settings::deepseek_vibing_plugins_enabled_for_workspace(
            config,
            auth,
            Some(&self.workspace_settings_cache),
        )
        .await
        {
            Ok(enabled) => enabled,
            Err(err) => {
                warn!(
                    "failed to fetch workspace DeepSeekVibing plugins setting; allowing DeepSeekVibing plugins: {err:#}"
                );
                true
            }
        }
    }

    async fn plugin_list_response(
        &self,
        params: PluginListParams,
    ) -> Result<PluginListResponse, JSONRPCErrorError> {
        let plugins_manager = self.thread_manager.plugins_manager();
        let PluginListParams {
            cwds,
            marketplace_kinds,
        } = params;
        let roots = cwds.unwrap_or_default();
        let marketplace_kinds =
            marketplace_kinds.unwrap_or_else(|| vec![PluginListMarketplaceKind::Local]);
        let include_local = marketplace_kinds.contains(&PluginListMarketplaceKind::Local);

        let config = self.load_latest_config(/*fallback_cwd*/ None).await?;
        let empty_response = || PluginListResponse {
            marketplaces: Vec::new(),
            marketplace_load_errors: Vec::new(),
            featured_plugin_ids: Vec::new(),
        };
        if !config.features.enabled(Feature::Plugins) {
            return Ok(empty_response());
        }
        let auth = self.auth_manager.auth().await;
        if !self
            .workspace_deepseek_vibing_plugins_enabled(&config, auth.as_ref())
            .await
        {
            return Ok(empty_response());
        }
        let plugins_input = config.plugins_config_input();
        let (data, marketplace_load_errors) = if include_local {
            let config_for_marketplace_listing = plugins_input.clone();
            let plugins_manager_for_marketplace_listing = plugins_manager.clone();
            match tokio::task::spawn_blocking(move || {
                let outcome = plugins_manager_for_marketplace_listing
                    .list_marketplaces_for_config(&config_for_marketplace_listing, &roots)?;
                Ok::<
                    (
                        Vec<PluginMarketplaceEntry>,
                        Vec<deepseek_vibing_app_server_protocol::MarketplaceLoadErrorInfo>,
                    ),
                    MarketplaceError,
                >((
                    outcome
                        .marketplaces
                        .into_iter()
                        .map(|marketplace| PluginMarketplaceEntry {
                            name: marketplace.name,
                            path: Some(marketplace.path),
                            interface: marketplace.interface.map(|interface| {
                                MarketplaceInterface {
                                    display_name: interface.display_name,
                                }
                            }),
                            plugins: marketplace
                                .plugins
                                .into_iter()
                                .map(convert_configured_marketplace_plugin_to_plugin_summary)
                                .collect(),
                        })
                        .collect(),
                    outcome
                        .errors
                        .into_iter()
                        .map(
                            |err| deepseek_vibing_app_server_protocol::MarketplaceLoadErrorInfo {
                                marketplace_path: err.path,
                                message: err.message,
                            },
                        )
                        .collect(),
                ))
            })
            .await
            {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(err)) => {
                    return Err(Self::marketplace_error(err, "list marketplace plugins"));
                }
                Err(err) => {
                    return Err(internal_error(format!(
                        "failed to list marketplace plugins: {err}"
                    )));
                }
            }
        } else {
            (Vec::new(), Vec::new())
        };

        Ok(PluginListResponse {
            marketplaces: data,
            marketplace_load_errors,
            featured_plugin_ids: Vec::new(),
        })
    }

    async fn plugin_installed_response(
        &self,
        params: PluginInstalledParams,
    ) -> Result<PluginInstalledResponse, JSONRPCErrorError> {
        let plugins_manager = self.thread_manager.plugins_manager();
        let PluginInstalledParams {
            cwds,
            install_suggestion_plugin_names,
        } = params;
        let roots = cwds.unwrap_or_default();
        let install_suggestion_plugin_names = install_suggestion_plugin_names
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<_>>();

        let empty_response = || PluginInstalledResponse {
            marketplaces: Vec::new(),
            marketplace_load_errors: Vec::new(),
        };
        let config = self.load_latest_config(/*fallback_cwd*/ None).await?;
        if !config.features.enabled(Feature::Plugins) {
            return Ok(empty_response());
        }
        let auth = self.auth_manager.auth().await;
        if !self
            .workspace_deepseek_vibing_plugins_enabled(&config, auth.as_ref())
            .await
        {
            return Ok(empty_response());
        }

        let plugins_input = config.plugins_config_input();

        let (data, marketplace_load_errors) = self
            .load_local_installed_and_suggested_plugins(
                plugins_manager.clone(),
                &plugins_input,
                roots,
                install_suggestion_plugin_names,
            )
            .await?;

        Ok(PluginInstalledResponse {
            marketplaces: data,
            marketplace_load_errors,
        })
    }

    async fn load_local_installed_and_suggested_plugins(
        &self,
        plugins_manager: Arc<deepseek_vibing_core_plugins::PluginsManager>,
        plugins_input: &deepseek_vibing_core_plugins::PluginsConfigInput,
        roots: Vec<AbsolutePathBuf>,
        install_suggestion_plugin_names: HashSet<String>,
    ) -> Result<
        (
            Vec<PluginMarketplaceEntry>,
            Vec<deepseek_vibing_app_server_protocol::MarketplaceLoadErrorInfo>,
        ),
        JSONRPCErrorError,
    > {
        let config_for_marketplace_listing = plugins_input.clone();
        match tokio::task::spawn_blocking(move || {
            let outcome = plugins_manager
                .list_marketplaces_for_config(&config_for_marketplace_listing, &roots)?;
            Ok::<
                (
                    Vec<PluginMarketplaceEntry>,
                    Vec<deepseek_vibing_app_server_protocol::MarketplaceLoadErrorInfo>,
                ),
                MarketplaceError,
            >((
                outcome
                    .marketplaces
                    .into_iter()
                    .filter_map(|marketplace| {
                        let plugins = marketplace
                            .plugins
                            .into_iter()
                            .filter(|plugin| {
                                plugin.installed
                                    || install_suggestion_plugin_names.contains(&plugin.name)
                            })
                            .map(convert_configured_marketplace_plugin_to_plugin_summary)
                            .collect::<Vec<_>>();

                        (!plugins.is_empty()).then_some(PluginMarketplaceEntry {
                            name: marketplace.name,
                            path: Some(marketplace.path),
                            interface: marketplace.interface.map(|interface| {
                                MarketplaceInterface {
                                    display_name: interface.display_name,
                                }
                            }),
                            plugins,
                        })
                    })
                    .collect(),
                outcome
                    .errors
                    .into_iter()
                    .map(
                        |err| deepseek_vibing_app_server_protocol::MarketplaceLoadErrorInfo {
                            marketplace_path: err.path,
                            message: err.message,
                        },
                    )
                    .collect(),
            ))
        })
        .await
        {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(err)) => Err(Self::marketplace_error(
                err,
                "list installed and suggested marketplace plugins",
            )),
            Err(err) => Err(internal_error(format!(
                "failed to list installed and suggested plugins: {err}"
            ))),
        }
    }

    async fn plugin_read_response(
        &self,
        params: PluginReadParams,
    ) -> Result<PluginReadResponse, JSONRPCErrorError> {
        let plugins_manager = self.thread_manager.plugins_manager();
        let PluginReadParams {
            marketplace_path,
            remote_marketplace_name,
            plugin_name,
        } = params;
        let marketplace_path = match (marketplace_path, remote_marketplace_name) {
            (Some(marketplace_path), None) => marketplace_path,
            (None, Some(remote_marketplace_name)) => {
                return Err(invalid_request(format!(
                    "remote plugin read is not available for marketplace {remote_marketplace_name}"
                )));
            }
            (Some(_), Some(_)) | (None, None) => {
                return Err(invalid_request(
                    "plugin/read requires exactly one of marketplacePath or remoteMarketplaceName",
                ));
            }
        };
        let config_cwd = marketplace_path.as_path().parent().map(Path::to_path_buf);

        let config = self.load_latest_config(config_cwd).await?;
        let plugins_input = config.plugins_config_input();

        let request = PluginReadRequest {
            plugin_name,
            marketplace_path,
        };
        let outcome = plugins_manager
            .read_plugin_for_config(&plugins_input, &request)
            .await
            .map_err(|err| Self::marketplace_error(err, "read plugin details"))?;
        let environment_manager = self.thread_manager.environment_manager();
        let app_summaries = load_plugin_app_summaries(
            &config,
            &outcome.plugin.apps,
            Arc::clone(&environment_manager),
        )
        .await;
        let visible_skills = outcome
            .plugin
            .skills
            .iter()
            .filter(|skill| {
                skill.matches_product_restriction_for_product(
                    self.thread_manager.session_source().restriction_product(),
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let plugin = PluginDetail {
            marketplace_name: outcome.marketplace_name,
            marketplace_path: outcome.marketplace_path,
            summary: PluginSummary {
                id: outcome.plugin.id,
                remote_plugin_id: None,
                local_version: outcome.plugin.local_version,
                name: outcome.plugin.name,
                source: marketplace_plugin_source_to_info(outcome.plugin.source),
                installed: outcome.plugin.installed,
                enabled: outcome.plugin.enabled,
                install_policy: outcome.plugin.policy.installation.into(),
                auth_policy: outcome.plugin.policy.authentication.into(),
                availability: PluginAvailability::Available,
                interface: outcome.plugin.interface.map(local_plugin_interface_to_info),
                keywords: outcome.plugin.keywords,
            },
            description: outcome.plugin.description,
            skills: plugin_skills_to_info(&visible_skills, &outcome.plugin.disabled_skill_paths),
            hooks: outcome
                .plugin
                .hooks
                .into_iter()
                .map(
                    |hook| deepseek_vibing_app_server_protocol::PluginHookSummary {
                        key: hook.key,
                        event_name: hook.event_name.into(),
                    },
                )
                .collect(),
            apps: app_summaries,
            mcp_servers: outcome.plugin.mcp_server_names,
        };

        Ok(PluginReadResponse { plugin })
    }

    async fn plugin_skill_read_response(
        &self,
        params: PluginSkillReadParams,
    ) -> Result<PluginSkillReadResponse, JSONRPCErrorError> {
        let PluginSkillReadParams {
            remote_marketplace_name,
            ..
        } = params;
        Err(invalid_request(format!(
            "remote plugin skill read is not available for marketplace {remote_marketplace_name}"
        )))
    }

    async fn plugin_install_response(
        &self,
        params: PluginInstallParams,
    ) -> Result<PluginInstallResponse, JSONRPCErrorError> {
        let PluginInstallParams {
            marketplace_path,
            remote_marketplace_name,
            plugin_name,
        } = params;
        let marketplace_path = match (marketplace_path, remote_marketplace_name) {
            (Some(marketplace_path), None) => marketplace_path,
            (None, Some(remote_marketplace_name)) => {
                return Err(invalid_request(format!(
                    "remote plugin install is not available for marketplace {remote_marketplace_name}"
                )));
            }
            (Some(_), Some(_)) | (None, None) => {
                return Err(invalid_request(
                    "plugin/install requires exactly one of marketplacePath or remoteMarketplaceName",
                ));
            }
        };
        let config_cwd = marketplace_path.as_path().parent().map(Path::to_path_buf);
        let config = self.load_latest_config(config_cwd.clone()).await?;
        let auth = self.auth_manager.auth().await;

        if !self
            .workspace_deepseek_vibing_plugins_enabled(&config, auth.as_ref())
            .await
        {
            return Err(invalid_request(
                "DeepSeekVibing plugins are disabled for this workspace",
            ));
        }

        let plugins_manager = self.thread_manager.plugins_manager();
        let request = PluginInstallRequest {
            plugin_name,
            marketplace_path,
        };

        let result = plugins_manager
            .install_plugin(request)
            .await
            .map_err(Self::plugin_install_error)?;
        let config = match self.load_latest_config(config_cwd).await {
            Ok(config) => config,
            Err(err) => {
                warn!(
                    "failed to reload config after plugin install, using current config: {err:?}"
                );
                config
            }
        };

        self.on_effective_plugins_changed();

        let plugin_mcp_servers = load_plugin_mcp_servers(result.installed_path.as_path()).await;
        if !plugin_mcp_servers.is_empty() {
            self.start_plugin_mcp_oauth_logins(&config, plugin_mcp_servers)
                .await;
        }

        let plugin_apps = load_plugin_apps(result.installed_path.as_path()).await;
        let auth = self.auth_manager.auth().await;
        let apps_needing_auth = self
            .plugin_apps_needing_auth_for_install(
                &config,
                auth.as_ref()
                    .is_some_and(DeepSeekVibingAuth::is_chatgpt_auth),
                &result.plugin_id.as_key(),
                &plugin_apps,
            )
            .await;

        Ok(PluginInstallResponse {
            auth_policy: result.auth_policy.into(),
            apps_needing_auth,
        })
    }

    async fn plugin_apps_needing_auth_for_install(
        &self,
        config: &Config,
        is_chatgpt_auth: bool,
        plugin_id: &str,
        plugin_apps: &[deepseek_vibing_plugin::AppConnectorId],
    ) -> Vec<AppSummary> {
        if plugin_apps.is_empty() || !config.features.apps_enabled_for_auth(is_chatgpt_auth) {
            return Vec::new();
        }

        let environment_manager = self.thread_manager.environment_manager();
        let (all_connectors_result, accessible_connectors_result) = tokio::join!(
            connectors::list_all_connectors_with_options(config, /*force_refetch*/ false),
            connectors::list_accessible_connectors_from_mcp_tools_with_environment_manager(
                config,
                /*force_refetch*/ true,
                Arc::clone(&environment_manager)
            ),
        );

        let all_connectors = match all_connectors_result {
            Ok(connectors) => connectors,
            Err(err) => {
                warn!(
                    plugin = plugin_id,
                    "failed to load app metadata after plugin install: {err:#}"
                );
                connectors::list_cached_all_connectors(config)
                    .await
                    .unwrap_or_default()
            }
        };
        let all_connectors = connectors::connectors_for_plugin_apps(all_connectors, plugin_apps);
        let (accessible_connectors, deepseek_vibing_apps_ready) = match accessible_connectors_result
        {
            Ok(status) => (status.connectors, status.deepseek_vibing_apps_ready),
            Err(err) => {
                warn!(
                    plugin = plugin_id,
                    "failed to load accessible apps after plugin install: {err:#}"
                );
                (
                    connectors::list_cached_accessible_connectors_from_mcp_tools(config)
                        .await
                        .unwrap_or_default(),
                    false,
                )
            }
        };
        if !deepseek_vibing_apps_ready {
            warn!(
                plugin = plugin_id,
                "deepseek_vibing_apps MCP not ready after plugin install; skipping appsNeedingAuth check"
            );
        }

        plugin_apps_needing_auth(
            &all_connectors,
            &accessible_connectors,
            plugin_apps,
            deepseek_vibing_apps_ready,
        )
    }

    async fn start_plugin_mcp_oauth_logins(
        &self,
        config: &Config,
        plugin_mcp_servers: HashMap<String, McpServerConfig>,
    ) {
        for (name, server) in plugin_mcp_servers {
            let oauth_config = match oauth_login_support(&server.transport).await {
                McpOAuthLoginSupport::Supported(config) => config,
                McpOAuthLoginSupport::Unsupported => continue,
                McpOAuthLoginSupport::Unknown(err) => {
                    warn!(
                        "MCP server may or may not require login for plugin install {name}: {err}"
                    );
                    continue;
                }
            };

            let resolved_scopes = resolve_oauth_scopes(
                /*explicit_scopes*/ None,
                server.scopes.clone(),
                oauth_config.discovered_scopes.clone(),
            );

            let store_mode = config.mcp_oauth_credentials_store_mode;
            let callback_port = config.mcp_oauth_callback_port;
            let callback_url = config.mcp_oauth_callback_url.clone();
            let outgoing = Arc::clone(&self.outgoing);
            let notification_name = name.clone();

            tokio::spawn(async move {
                let oauth_client_id = server.oauth_client_id();
                let first_attempt = perform_oauth_login_silent(
                    &name,
                    &oauth_config.url,
                    store_mode,
                    oauth_config.http_headers.clone(),
                    oauth_config.env_http_headers.clone(),
                    &resolved_scopes.scopes,
                    oauth_client_id,
                    server.oauth_resource.as_deref(),
                    callback_port,
                    callback_url.as_deref(),
                )
                .await;

                let final_result = match first_attempt {
                    Err(err) if should_retry_without_scopes(&resolved_scopes, &err) => {
                        perform_oauth_login_silent(
                            &name,
                            &oauth_config.url,
                            store_mode,
                            oauth_config.http_headers,
                            oauth_config.env_http_headers,
                            &[],
                            oauth_client_id,
                            server.oauth_resource.as_deref(),
                            callback_port,
                            callback_url.as_deref(),
                        )
                        .await
                    }
                    result => result,
                };

                let (success, error) = match final_result {
                    Ok(()) => (true, None),
                    Err(err) => (false, Some(err.to_string())),
                };

                let notification = ServerNotification::McpServerOauthLoginCompleted(
                    McpServerOauthLoginCompletedNotification {
                        name: notification_name,
                        success,
                        error,
                    },
                );
                outgoing.send_server_notification(notification).await;
            });
        }
    }

    async fn plugin_uninstall_response(
        &self,
        params: PluginUninstallParams,
    ) -> Result<PluginUninstallResponse, JSONRPCErrorError> {
        let PluginUninstallParams { plugin_id } = params;
        if deepseek_vibing_plugin::PluginId::parse(&plugin_id).is_err() {
            return Err(invalid_request(
                "invalid plugin id; remote plugin uninstall is not available",
            ));
        }
        let plugins_manager = self.thread_manager.plugins_manager();

        plugins_manager
            .uninstall_plugin(plugin_id)
            .await
            .map_err(Self::plugin_uninstall_error)?;
        match self.load_latest_config(/*fallback_cwd*/ None).await {
            Ok(_) => self.on_effective_plugins_changed(),
            Err(err) => {
                warn!(
                    "failed to reload config after plugin uninstall, clearing plugin-related caches only: {err:?}"
                );
                self.clear_plugin_related_caches();
            }
        }
        Ok(PluginUninstallResponse {})
    }

    fn plugin_install_error(err: CorePluginInstallError) -> JSONRPCErrorError {
        if err.is_invalid_request() {
            return invalid_request(err.to_string());
        }

        match err {
            CorePluginInstallError::Marketplace(err) => {
                Self::marketplace_error(err, "install plugin")
            }
            CorePluginInstallError::Config(err) => {
                internal_error(format!("failed to persist installed plugin config: {err}"))
            }
            CorePluginInstallError::Join(err) => {
                internal_error(format!("failed to install plugin: {err}"))
            }
            CorePluginInstallError::Store(err) => {
                internal_error(format!("failed to install plugin: {err}"))
            }
        }
    }

    fn plugin_uninstall_error(err: CorePluginUninstallError) -> JSONRPCErrorError {
        if err.is_invalid_request() {
            return invalid_request(err.to_string());
        }

        match err {
            CorePluginUninstallError::Config(err) => {
                internal_error(format!("failed to clear plugin config: {err}"))
            }
            CorePluginUninstallError::Join(err) => {
                internal_error(format!("failed to uninstall plugin: {err}"))
            }
            CorePluginUninstallError::Store(err) => {
                internal_error(format!("failed to uninstall plugin: {err}"))
            }
            CorePluginUninstallError::InvalidPluginId(_) => {
                unreachable!("invalid plugin ids are handled above");
            }
        }
    }

    fn marketplace_error(err: MarketplaceError, action: &str) -> JSONRPCErrorError {
        match err {
            MarketplaceError::MarketplaceNotFound { .. }
            | MarketplaceError::InvalidMarketplaceFile { .. }
            | MarketplaceError::PluginNotFound { .. }
            | MarketplaceError::PluginNotAvailable { .. }
            | MarketplaceError::PluginsDisabled
            | MarketplaceError::InvalidPlugin(_) => invalid_request(err.to_string()),
            MarketplaceError::Io { .. } => internal_error(format!("failed to {action}: {err}")),
        }
    }
}

async fn load_plugin_app_summaries(
    config: &Config,
    plugin_apps: &[deepseek_vibing_plugin::AppConnectorId],
    environment_manager: Arc<EnvironmentManager>,
) -> Vec<AppSummary> {
    if plugin_apps.is_empty() {
        return Vec::new();
    }

    let connectors =
        match connectors::list_all_connectors_with_options(config, /*force_refetch*/ false).await {
            Ok(connectors) => connectors,
            Err(err) => {
                warn!("failed to load app metadata for plugin/read: {err:#}");
                connectors::list_cached_all_connectors(config)
                    .await
                    .unwrap_or_default()
            }
        };

    let plugin_connectors = connectors::connectors_for_plugin_apps(connectors, plugin_apps);

    let accessible_connectors =
        match connectors::list_accessible_connectors_from_mcp_tools_with_environment_manager(
            config,
            /*force_refetch*/ false,
            environment_manager,
        )
        .await
        {
            Ok(status) if status.deepseek_vibing_apps_ready => status.connectors,
            Ok(_) => {
                return plugin_connectors
                    .into_iter()
                    .map(AppSummary::from)
                    .collect();
            }
            Err(err) => {
                warn!("failed to load app auth state for plugin/read: {err:#}");
                return plugin_connectors
                    .into_iter()
                    .map(AppSummary::from)
                    .collect();
            }
        };

    let accessible_ids = accessible_connectors
        .iter()
        .map(|connector| connector.id.as_str())
        .collect::<HashSet<_>>();

    plugin_connectors
        .into_iter()
        .map(|connector| {
            let needs_auth = !accessible_ids.contains(connector.id.as_str());
            AppSummary {
                id: connector.id,
                name: connector.name,
                description: connector.description,
                install_url: connector.install_url,
                needs_auth,
            }
        })
        .collect()
}

fn plugin_apps_needing_auth(
    all_connectors: &[AppInfo],
    accessible_connectors: &[AppInfo],
    plugin_apps: &[deepseek_vibing_plugin::AppConnectorId],
    deepseek_vibing_apps_ready: bool,
) -> Vec<AppSummary> {
    if !deepseek_vibing_apps_ready {
        return Vec::new();
    }

    let accessible_ids = accessible_connectors
        .iter()
        .map(|connector| connector.id.as_str())
        .collect::<HashSet<_>>();
    let plugin_app_ids = plugin_apps
        .iter()
        .map(|connector_id| connector_id.0.as_str())
        .collect::<HashSet<_>>();

    all_connectors
        .iter()
        .filter(|connector| {
            plugin_app_ids.contains(connector.id.as_str())
                && !accessible_ids.contains(connector.id.as_str())
        })
        .cloned()
        .map(|connector| AppSummary {
            id: connector.id,
            name: connector.name,
            description: connector.description,
            install_url: connector.install_url,
            needs_auth: true,
        })
        .collect()
}
