use std::sync::Arc;

use deepseek_vibing_analytics::AnalyticsEventsClient;
use deepseek_vibing_auth::AuthManager;
use deepseek_vibing_core::config::Config;

pub(crate) fn analytics_events_client_from_config(
    auth_manager: Arc<AuthManager>,
    config: &Config,
) -> AnalyticsEventsClient {
    AnalyticsEventsClient::new(
        auth_manager,
        config.chatgpt_base_url.trim_end_matches('/').to_string(),
        config.analytics_enabled,
    )
}
