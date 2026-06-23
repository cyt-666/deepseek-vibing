use deepseek_vibing_auth::AuthManager;
use deepseek_vibing_core::config::Config;

pub(crate) async fn rate_limits_ok(_auth_manager: &AuthManager, _config: &Config) -> bool {
    true
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
