//! Local-only cloud requirements compatibility layer.
//!
//! DeepSeek Vibing does not fetch ChatGPT/DeepSeekVibing backend-managed workspace
//! requirements. The public functions remain so existing config-loading code can
//! request a loader, but the loader resolves to `None`.

use deepseek_vibing_auth::AuthManager;
use deepseek_vibing_config::CloudRequirementsLoader;
use deepseek_vibing_config::types::AuthCredentialsStoreMode;
use std::path::PathBuf;
use std::sync::Arc;

pub fn cloud_requirements_loader(
    _auth_manager: Arc<AuthManager>,
    _chatgpt_base_url: String,
    _deepseek_vibing_home: PathBuf,
) -> CloudRequirementsLoader {
    CloudRequirementsLoader::default()
}

pub async fn cloud_requirements_loader_for_storage(
    _deepseek_vibing_home: PathBuf,
    _enable_deepseek_vibing_api_key_env: bool,
    _credentials_store_mode: AuthCredentialsStoreMode,
    _chatgpt_base_url: String,
) -> CloudRequirementsLoader {
    CloudRequirementsLoader::default()
}
