#[cfg(target_os = "linux")]
mod bwrap;
pub mod landlock;
mod manager;
pub mod policy_transforms;
#[cfg(target_os = "macos")]
pub mod seatbelt;

#[cfg(target_os = "linux")]
pub use bwrap::find_system_bwrap_in_path;
#[cfg(target_os = "linux")]
pub use bwrap::system_bwrap_warning;
pub use manager::SandboxCommand;
pub use manager::SandboxExecRequest;
pub use manager::SandboxManager;
pub use manager::SandboxTransformError;
pub use manager::SandboxTransformRequest;
pub use manager::SandboxType;
pub use manager::SandboxablePreference;
pub use manager::compatibility_sandbox_policy_for_permission_profile;
pub use manager::get_platform_sandbox;
pub use manager::with_managed_mitm_ca_readable_root;

use deepseek_vibing_protocol::error::DeepSeekVibingErr;

#[cfg(not(target_os = "linux"))]
pub fn system_bwrap_warning(
    _permission_profile: &deepseek_vibing_protocol::models::PermissionProfile,
) -> Option<String> {
    None
}

impl From<SandboxTransformError> for DeepSeekVibingErr {
    fn from(err: SandboxTransformError) -> Self {
        match err {
            SandboxTransformError::MissingLinuxSandboxExecutable => {
                DeepSeekVibingErr::LandlockSandboxExecutableNotProvided
            }
            #[cfg(target_os = "linux")]
            SandboxTransformError::Wsl1UnsupportedForBubblewrap => {
                DeepSeekVibingErr::UnsupportedOperation(
                    crate::bwrap::WSL1_BWRAP_WARNING.to_string(),
                )
            }
            #[cfg(not(target_os = "macos"))]
            SandboxTransformError::SeatbeltUnavailable => DeepSeekVibingErr::UnsupportedOperation(
                "seatbelt sandbox is only available on macOS".to_string(),
            ),
        }
    }
}
