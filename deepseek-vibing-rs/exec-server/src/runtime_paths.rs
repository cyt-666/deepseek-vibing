use std::path::PathBuf;

use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;

/// Runtime paths needed by exec-server child processes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecServerRuntimePaths {
    /// Stable path to the DeepSeekVibing executable used to launch hidden helper modes.
    pub deepseek_vibing_self_exe: AbsolutePathBuf,
    /// Path to the Linux sandbox helper alias used when the platform sandbox
    /// needs to re-enter DeepSeekVibing by argv0.
    pub deepseek_vibing_linux_sandbox_exe: Option<AbsolutePathBuf>,
}

impl ExecServerRuntimePaths {
    pub fn from_optional_paths(
        deepseek_vibing_self_exe: Option<PathBuf>,
        deepseek_vibing_linux_sandbox_exe: Option<PathBuf>,
    ) -> std::io::Result<Self> {
        let deepseek_vibing_self_exe = deepseek_vibing_self_exe.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "DeepSeekVibing executable path is not configured",
            )
        })?;
        Self::new(deepseek_vibing_self_exe, deepseek_vibing_linux_sandbox_exe)
    }

    pub fn new(
        deepseek_vibing_self_exe: PathBuf,
        deepseek_vibing_linux_sandbox_exe: Option<PathBuf>,
    ) -> std::io::Result<Self> {
        Ok(Self {
            deepseek_vibing_self_exe: absolute_path(deepseek_vibing_self_exe)?,
            deepseek_vibing_linux_sandbox_exe: deepseek_vibing_linux_sandbox_exe
                .map(absolute_path)
                .transpose()?,
        })
    }
}

fn absolute_path(path: PathBuf) -> std::io::Result<AbsolutePathBuf> {
    AbsolutePathBuf::from_absolute_path(path.as_path())
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))
}
