//! DeepSeek Vibing auth compatibility layer.
//!
//! This crate intentionally does not implement ChatGPT/OAuth/device login,
//! agent identity, token refresh, or `auth.json` persistence. It provides the
//! small API-key based auth surface still needed by local CLI/TUI/core code
//! while the old DeepSeekVibing login system is removed from the build graph.

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
pub use deepseek_vibing_config::types::AuthCredentialsStoreMode;
use deepseek_vibing_model_provider_info::ModelProviderInfo;
use deepseek_vibing_otel::AuthEnvTelemetryMetadata;
pub use deepseek_vibing_protocol::auth::AuthMode;
use deepseek_vibing_protocol::auth::PlanType;
use deepseek_vibing_protocol::auth::RefreshTokenFailedError;
use deepseek_vibing_protocol::auth::RefreshTokenFailedReason;
use deepseek_vibing_protocol::config_types::ModelProviderAuthInfo;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::watch;

pub mod default_client {
    pub use deepseek_vibing_client::DEEPSEEK_VIBING_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR;
    pub use deepseek_vibing_client::DEFAULT_ORIGINATOR;
    pub use deepseek_vibing_client::DeepSeekVibingHttpClient;
    pub use deepseek_vibing_client::DeepSeekVibingRequestBuilder;
    pub use deepseek_vibing_client::Originator;
    pub use deepseek_vibing_client::RESIDENCY_HEADER_NAME;
    pub use deepseek_vibing_client::SetOriginatorError;
    pub use deepseek_vibing_client::USER_AGENT_SUFFIX;
    pub use deepseek_vibing_client::build_reqwest_client;
    pub use deepseek_vibing_client::create_client;
    pub use deepseek_vibing_client::default_headers;
    pub use deepseek_vibing_client::get_deepseek_vibing_user_agent;
    pub use deepseek_vibing_client::is_first_party_chat_originator;
    pub use deepseek_vibing_client::is_first_party_originator;
    pub use deepseek_vibing_client::originator;
    pub use deepseek_vibing_client::set_default_client_residency_requirement;
    pub use deepseek_vibing_client::set_default_originator;
    pub use deepseek_vibing_client::try_build_reqwest_client;
}

pub const DEEPSEEK_API_KEY_ENV_VAR: &str = "DEEPSEEK_API_KEY";
pub const VISION_API_KEY_ENV_VAR: &str = "VISION_API_KEY";

// Deprecated compatibility constants. They remain only so legacy internal code
// can be deleted incrementally without keeping the old login crate alive.
pub const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
pub const DEEPSEEK_VIBING_API_KEY_ENV_VAR: &str = "DEEPSEEK_VIBING_API_KEY";
pub const DEEPSEEK_VIBING_ACCESS_TOKEN_ENV_VAR: &str = "DEEPSEEK_VIBING_ACCESS_TOKEN";
pub const REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR: &str = "DEEPSEEK_VIBING_REFRESH_TOKEN_URL_OVERRIDE";
pub const REVOKE_TOKEN_URL_OVERRIDE_ENV_VAR: &str = "DEEPSEEK_VIBING_REVOKE_TOKEN_URL_OVERRIDE";
pub const CLIENT_ID: &str = "deepseek-vibing-local";

#[derive(Debug, Clone, Default)]
pub struct ServerOptions;

#[derive(Debug, Clone, Default)]
pub struct ShutdownHandle;

#[derive(Debug, Clone, Default)]
pub struct LoginServer;

#[derive(Debug, Clone, Default)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyAuth {
    api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnsupportedBackendAuth {
    token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepSeekVibingAuth {
    ApiKey(ApiKeyAuth),
    Chatgpt(UnsupportedBackendAuth),
    ChatgptAuthTokens(UnsupportedBackendAuth),
    AgentIdentity(AgentIdentityAuth),
}

pub mod auth {
    pub use crate::AgentIdentityAuth;
    pub use crate::AgentIdentityAuthRecord;
    pub use crate::ExternalAuth;
    pub use crate::ExternalAuthRefreshContext;
    pub use crate::ExternalAuthRefreshReason;
    pub use crate::ExternalAuthTokens;
    use std::path::Path;

    pub fn login_with_chatgpt_auth_tokens(
        _deepseek_vibing_home: &Path,
        _access_token: &str,
        _chatgpt_account_id: &str,
        _chatgpt_plan_type: Option<&str>,
    ) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "ChatGPT auth tokens have been removed from DeepSeek Vibing",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentIdentityAuthRecord {
    pub agent_runtime_id: String,
    pub agent_private_key: String,
    pub account_id: String,
    pub task_id: Option<String>,
    pub fedramp: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentIdentityAuth {
    record: AgentIdentityAuthRecord,
}

impl AgentIdentityAuth {
    pub async fn load(record: AgentIdentityAuthRecord) -> std::io::Result<Self> {
        Ok(Self { record })
    }

    pub fn record(&self) -> &AgentIdentityAuthRecord {
        &self.record
    }

    pub fn account_id(&self) -> &str {
        &self.record.account_id
    }

    pub fn process_task_id(&self) -> &str {
        self.record.task_id.as_deref().unwrap_or("")
    }

    pub fn is_fedramp_account(&self) -> bool {
        self.record.fedramp
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TokenData {
    pub id_token: IdToken,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IdToken {
    pub email: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub fedramp: bool,
}

impl IdToken {
    pub fn is_fedramp_account(&self) -> bool {
        self.fedramp
    }
}

pub mod token_data {
    pub use crate::IdToken;
    pub use crate::TokenData;

    pub fn parse_chatgpt_jwt_claims(_token: &str) -> std::io::Result<IdToken> {
        Ok(IdToken::default())
    }

    pub fn parse_jwt_expiration(
        _token: &str,
    ) -> std::io::Result<Option<chrono::DateTime<chrono::Utc>>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthDotJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    pub tokens: Option<TokenData>,
    pub last_refresh: Option<DateTime<Utc>>,
    pub auth_mode: Option<AuthMode>,
    pub agent_identity: Option<AgentIdentityAuthRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAuthTokens {
    pub access_token: String,
    pub chatgpt_metadata: Option<ExternalAuthChatgptMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAuthChatgptMetadata {
    pub account_id: String,
    pub plan_type: Option<String>,
}

impl ExternalAuthTokens {
    pub fn access_token_only(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            chatgpt_metadata: None,
        }
    }

    pub fn chatgpt(
        access_token: impl Into<String>,
        chatgpt_account_id: impl Into<String>,
        chatgpt_plan_type: Option<String>,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            chatgpt_metadata: Some(ExternalAuthChatgptMetadata {
                account_id: chatgpt_account_id.into(),
                plan_type: chatgpt_plan_type,
            }),
        }
    }

    pub fn chatgpt_metadata(&self) -> Option<&ExternalAuthChatgptMetadata> {
        self.chatgpt_metadata.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalAuthRefreshReason {
    Unauthorized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAuthRefreshContext {
    pub reason: ExternalAuthRefreshReason,
    pub previous_account_id: Option<String>,
}

#[async_trait]
pub trait ExternalAuth: Send + Sync {
    fn auth_mode(&self) -> AuthMode;

    async fn resolve(&self) -> std::io::Result<Option<ExternalAuthTokens>> {
        Ok(None)
    }

    async fn refresh(
        &self,
        _context: ExternalAuthRefreshContext,
    ) -> std::io::Result<ExternalAuthTokens>;
}

#[derive(Debug, thiserror::Error)]
pub enum RefreshTokenError {
    #[error("{0}")]
    Permanent(#[from] RefreshTokenFailedError),
    #[error(transparent)]
    Transient(#[from] std::io::Error),
}

impl From<RefreshTokenError> for std::io::Error {
    fn from(err: RefreshTokenError) -> Self {
        match err {
            RefreshTokenError::Permanent(failed) => std::io::Error::other(failed),
            RefreshTokenError::Transient(inner) => inner,
        }
    }
}

impl DeepSeekVibingAuth {
    pub fn from_api_key(api_key: impl Into<String>) -> Self {
        Self::ApiKey(ApiKeyAuth {
            api_key: api_key.into(),
        })
    }

    pub fn create_dummy_chatgpt_auth_for_testing() -> Self {
        Self::Chatgpt(UnsupportedBackendAuth {
            token: Some("dummy-chatgpt-token".to_string()),
        })
    }

    pub async fn from_auth_storage(
        _home: &Path,
        _mode: AuthCredentialsStoreMode,
        _chatgpt_base_url: Option<&str>,
    ) -> std::io::Result<Option<Self>> {
        Ok(None)
    }

    pub async fn from_agent_identity_jwt(
        _jwt: &str,
        _chatgpt_base_url: Option<&str>,
    ) -> std::io::Result<Self> {
        Err(std::io::Error::other(
            "Agent Identity auth has been removed from DeepSeek Vibing",
        ))
    }

    pub fn auth_mode(&self) -> AuthMode {
        match self {
            Self::ApiKey(_) => AuthMode::ApiKey,
            Self::Chatgpt(_) => AuthMode::Chatgpt,
            Self::ChatgptAuthTokens(_) => AuthMode::ChatgptAuthTokens,
            Self::AgentIdentity(_) => AuthMode::AgentIdentity,
        }
    }

    pub fn api_auth_mode(&self) -> AuthMode {
        self.auth_mode()
    }

    pub fn is_api_key_auth(&self) -> bool {
        matches!(self, Self::ApiKey(_))
    }

    pub fn is_chatgpt_auth(&self) -> bool {
        false
    }

    pub fn is_external_chatgpt_tokens(&self) -> bool {
        false
    }

    pub fn uses_deepseek_vibing_backend(&self) -> bool {
        false
    }

    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::ApiKey(auth) => Some(auth.api_key.as_str()),
            _ => None,
        }
    }

    pub fn get_token(&self) -> Result<String, std::io::Error> {
        match self {
            Self::ApiKey(auth) => Ok(auth.api_key.clone()),
            Self::Chatgpt(auth) | Self::ChatgptAuthTokens(auth) => auth
                .token
                .clone()
                .ok_or_else(|| std::io::Error::other("ChatGPT auth has been removed")),
            Self::AgentIdentity(_) => Err(std::io::Error::other(
                "Agent Identity auth has been removed from DeepSeek Vibing",
            )),
        }
    }

    pub fn get_token_data(&self) -> Result<TokenData, std::io::Error> {
        Err(std::io::Error::other(
            "ChatGPT token storage has been removed from DeepSeek Vibing",
        ))
    }

    pub fn get_current_token_data(&self) -> Option<TokenData> {
        None
    }

    pub fn get_current_auth_json(&self) -> Option<AuthDotJson> {
        None
    }

    pub fn get_account_id(&self) -> Option<String> {
        None
    }

    pub fn get_chatgpt_user_id(&self) -> Option<String> {
        None
    }

    pub fn get_account_email(&self) -> Option<String> {
        None
    }

    pub fn account_plan_type(&self) -> Option<PlanType> {
        None
    }

    pub fn is_workspace_account(&self) -> bool {
        false
    }

    pub fn is_fedramp_account(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Default)]
struct CachedAuth {
    auth: Option<DeepSeekVibingAuth>,
}

pub trait AuthManagerConfig {
    fn deepseek_vibing_home(&self) -> PathBuf;
    fn deepseek_api_key(&self) -> Option<String>;
}

pub struct AuthManager {
    deepseek_vibing_home: PathBuf,
    inner: RwLock<CachedAuth>,
    auth_change_tx: watch::Sender<u64>,
    enable_env_fallback: bool,
    external_auth: RwLock<Option<Arc<dyn ExternalAuth>>>,
}

impl Debug for AuthManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthManager")
            .field("deepseek_vibing_home", &self.deepseek_vibing_home)
            .field("has_auth", &self.auth_cached().is_some())
            .field("enable_env_fallback", &self.enable_env_fallback)
            .field("has_external_auth", &self.has_external_auth())
            .finish_non_exhaustive()
    }
}

impl AuthManager {
    pub async fn new(
        deepseek_vibing_home: PathBuf,
        enable_env_fallback: bool,
        _auth_credentials_store_mode: AuthCredentialsStoreMode,
        _chatgpt_base_url: Option<String>,
    ) -> Self {
        let auth = if enable_env_fallback {
            std::env::var(DEEPSEEK_API_KEY_ENV_VAR)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(DeepSeekVibingAuth::from_api_key)
        } else {
            None
        };
        let (auth_change_tx, _auth_change_rx) = watch::channel(0);
        Self {
            deepseek_vibing_home,
            inner: RwLock::new(CachedAuth { auth }),
            auth_change_tx,
            enable_env_fallback,
            external_auth: RwLock::new(None),
        }
    }

    pub async fn shared(
        deepseek_vibing_home: PathBuf,
        enable_env_fallback: bool,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
        chatgpt_base_url: Option<String>,
    ) -> Arc<Self> {
        Arc::new(
            Self::new(
                deepseek_vibing_home,
                enable_env_fallback,
                auth_credentials_store_mode,
                chatgpt_base_url,
            )
            .await,
        )
    }

    pub async fn shared_from_config(
        config: &impl AuthManagerConfig,
        enable_env_fallback: bool,
    ) -> Arc<Self> {
        let auth = config
            .deepseek_api_key()
            .map(DeepSeekVibingAuth::from_api_key)
            .or_else(|| {
                if enable_env_fallback {
                    std::env::var(DEEPSEEK_API_KEY_ENV_VAR)
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                        .map(DeepSeekVibingAuth::from_api_key)
                } else {
                    None
                }
            });
        let (auth_change_tx, _auth_change_rx) = watch::channel(0);
        Arc::new(Self {
            deepseek_vibing_home: config.deepseek_vibing_home(),
            inner: RwLock::new(CachedAuth { auth }),
            auth_change_tx,
            enable_env_fallback,
            external_auth: RwLock::new(None),
        })
    }

    pub fn from_auth_for_testing(auth: DeepSeekVibingAuth) -> Arc<Self> {
        Self::from_auth_for_testing_with_home(auth, PathBuf::from("non-existent"))
    }

    pub fn from_auth_for_testing_with_home(
        auth: DeepSeekVibingAuth,
        deepseek_vibing_home: PathBuf,
    ) -> Arc<Self> {
        let (auth_change_tx, _auth_change_rx) = watch::channel(0);
        Arc::new(Self {
            deepseek_vibing_home,
            inner: RwLock::new(CachedAuth { auth: Some(auth) }),
            auth_change_tx,
            enable_env_fallback: false,
            external_auth: RwLock::new(None),
        })
    }

    pub fn external_bearer_only(config: ModelProviderAuthInfo) -> Arc<Self> {
        let _ = config;
        let (auth_change_tx, _auth_change_rx) = watch::channel(0);
        Arc::new(Self {
            deepseek_vibing_home: PathBuf::from("non-existent"),
            inner: RwLock::new(CachedAuth { auth: None }),
            auth_change_tx,
            enable_env_fallback: false,
            external_auth: RwLock::new(None),
        })
    }

    pub fn auth_cached(&self) -> Option<DeepSeekVibingAuth> {
        self.inner.read().ok().and_then(|guard| guard.auth.clone())
    }

    pub async fn auth(&self) -> Option<DeepSeekVibingAuth> {
        if let Some(external_auth) = self.external_auth() {
            if external_auth.auth_mode() == AuthMode::ApiKey
                && let Ok(Some(tokens)) = external_auth.resolve().await
            {
                return Some(DeepSeekVibingAuth::from_api_key(tokens.access_token));
            }
        }
        self.auth_cached()
    }

    pub async fn reload(&self) -> bool {
        false
    }

    pub async fn logout(&self) -> std::io::Result<bool> {
        if let Ok(mut guard) = self.inner.write() {
            let had_auth = guard.auth.take().is_some();
            if had_auth {
                self.auth_change_tx.send_modify(|revision| *revision += 1);
            }
            return Ok(had_auth);
        }
        Ok(false)
    }

    pub async fn logout_with_revoke(&self) -> std::io::Result<bool> {
        self.logout().await
    }

    pub fn refresh_failure_for_auth(
        &self,
        _auth: &DeepSeekVibingAuth,
    ) -> Option<RefreshTokenFailedError> {
        None
    }

    pub async fn refresh_token(&self) -> Result<(), RefreshTokenError> {
        Ok(())
    }

    pub async fn refresh_token_from_authority(&self) -> Result<(), RefreshTokenError> {
        Ok(())
    }

    pub fn auth_change_receiver(&self) -> watch::Receiver<u64> {
        self.auth_change_tx.subscribe()
    }

    pub fn set_external_auth(&self, external_auth: Arc<dyn ExternalAuth>) {
        if let Ok(mut guard) = self.external_auth.write() {
            *guard = Some(external_auth);
        }
    }

    pub fn clear_external_auth(&self) {
        if let Ok(mut guard) = self.external_auth.write() {
            *guard = None;
        }
    }

    fn external_auth(&self) -> Option<Arc<dyn ExternalAuth>> {
        self.external_auth
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
    }

    pub fn has_external_auth(&self) -> bool {
        self.external_auth().is_some()
    }

    pub fn is_external_chatgpt_auth_active(&self) -> bool {
        false
    }

    pub fn deepseek_vibing_api_key_env_enabled(&self) -> bool {
        self.enable_env_fallback
    }

    pub fn get_api_auth_mode(&self) -> Option<AuthMode> {
        self.auth_cached().map(|auth| auth.api_auth_mode())
    }

    pub fn auth_mode(&self) -> Option<AuthMode> {
        self.auth_cached().map(|auth| auth.auth_mode())
    }

    pub fn current_auth_uses_deepseek_vibing_backend(&self) -> bool {
        false
    }

    pub fn unauthorized_recovery(self: &Arc<Self>) -> UnauthorizedRecovery {
        UnauthorizedRecovery { attempted: false }
    }

    pub fn set_forced_chatgpt_workspace_id(&self, _workspace_id: Option<Vec<String>>) {}

    pub fn forced_chatgpt_workspace_id(&self) -> Option<Vec<String>> {
        None
    }
}

pub struct UnauthorizedRecovery {
    attempted: bool,
}

pub struct UnauthorizedRecoveryStepResult {
    auth_state_changed: Option<bool>,
}

impl UnauthorizedRecoveryStepResult {
    pub fn auth_state_changed(&self) -> Option<bool> {
        self.auth_state_changed
    }
}

impl UnauthorizedRecovery {
    pub fn has_next(&self) -> bool {
        !self.attempted && false
    }

    pub fn mode_name(&self) -> &'static str {
        "api_key"
    }

    pub fn step_name(&self) -> &'static str {
        "none"
    }

    pub fn unavailable_reason(&self) -> &'static str {
        "DeepSeek Vibing API-key auth cannot be refreshed automatically."
    }

    pub async fn next(&mut self) -> Result<UnauthorizedRecoveryStepResult, RefreshTokenError> {
        self.attempted = true;
        Err(RefreshTokenFailedError::new(
            RefreshTokenFailedReason::Other,
            "DeepSeek Vibing API-key auth cannot be refreshed automatically.",
        )
        .into())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthEnvTelemetry {
    pub openai_api_key_env_present: bool,
    pub deepseek_vibing_api_key_env_present: bool,
    pub deepseek_vibing_api_key_env_enabled: bool,
    pub provider_env_key_name: Option<String>,
    pub provider_env_key_present: Option<bool>,
    pub refresh_token_url_override_present: bool,
}

impl AuthEnvTelemetry {
    pub fn to_otel_metadata(&self) -> AuthEnvTelemetryMetadata {
        AuthEnvTelemetryMetadata {
            openai_api_key_env_present: self.openai_api_key_env_present,
            deepseek_vibing_api_key_env_present: self.deepseek_vibing_api_key_env_present,
            deepseek_vibing_api_key_env_enabled: self.deepseek_vibing_api_key_env_enabled,
            provider_env_key_name: self.provider_env_key_name.clone(),
            provider_env_key_present: self.provider_env_key_present,
            refresh_token_url_override_present: self.refresh_token_url_override_present,
        }
    }
}

pub mod auth_env_telemetry {
    pub use crate::AuthEnvTelemetry;

    pub fn collect_auth_env_telemetry(
        provider: &deepseek_vibing_model_provider_info::ModelProviderInfo,
        deepseek_vibing_api_key_env_enabled: bool,
    ) -> AuthEnvTelemetry {
        crate::collect_auth_env_telemetry(provider, deepseek_vibing_api_key_env_enabled)
    }
}

pub fn collect_auth_env_telemetry(
    provider: &ModelProviderInfo,
    deepseek_vibing_api_key_env_enabled: bool,
) -> AuthEnvTelemetry {
    AuthEnvTelemetry {
        openai_api_key_env_present: false,
        deepseek_vibing_api_key_env_present: false,
        deepseek_vibing_api_key_env_enabled,
        provider_env_key_name: provider.env_key.as_ref().map(|_| "configured".to_string()),
        provider_env_key_present: provider.env_key.as_deref().map(env_var_present),
        refresh_token_url_override_present: false,
    }
}

fn env_var_present(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => !value.trim().is_empty(),
        Err(std::env::VarError::NotUnicode(_)) => true,
        Err(std::env::VarError::NotPresent) => false,
    }
}

pub fn read_openai_api_key_from_env() -> Option<String> {
    std::env::var(DEEPSEEK_API_KEY_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub fn read_deepseek_vibing_access_token_from_env() -> Option<String> {
    None
}

pub fn load_auth_dot_json(
    _deepseek_vibing_home: &Path,
    _mode: AuthCredentialsStoreMode,
) -> std::io::Result<Option<AuthDotJson>> {
    Ok(None)
}

pub fn save_auth(
    _deepseek_vibing_home: &Path,
    _auth: &AuthDotJson,
    _mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "auth.json storage has been removed from DeepSeek Vibing",
    ))
}

pub fn login_with_api_key(
    _deepseek_vibing_home: &Path,
    _api_key: &str,
    _mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "Use ~/.deepseek-vibing/config.toml [auth].deepseek_api_key instead of login",
    ))
}

pub fn login_with_access_token(
    _deepseek_vibing_home: &Path,
    _access_token: &str,
    _mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "ChatGPT access-token login has been removed from DeepSeek Vibing",
    ))
}

pub fn logout(
    _deepseek_vibing_home: &Path,
    _mode: AuthCredentialsStoreMode,
) -> std::io::Result<bool> {
    Ok(false)
}

pub async fn logout_with_revoke(
    _deepseek_vibing_home: &Path,
    _mode: AuthCredentialsStoreMode,
) -> std::io::Result<bool> {
    Ok(false)
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub deepseek_vibing_home: PathBuf,
    pub auth_credentials_store_mode: AuthCredentialsStoreMode,
    pub forced_login_method: Option<deepseek_vibing_protocol::config_types::ForcedLoginMethod>,
    pub chatgpt_base_url: Option<String>,
    pub forced_chatgpt_workspace_id: Option<Vec<String>>,
}

pub async fn enforce_login_restrictions(_config: &AuthConfig) -> std::io::Result<()> {
    Ok(())
}
