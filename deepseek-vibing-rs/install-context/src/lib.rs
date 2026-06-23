use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;

const BIN_DIRNAME: &str = "bin";
const PACKAGE_METADATA_FILENAME: &str = "deepseek-vibing-package.json";
const PATH_DIRNAME: &str = "deepseek-vibing-path";
const RELEASES_DIRNAME: &str = "releases";
const RESOURCES_DIRNAME: &str = "deepseek-vibing-resources";
const STANDALONE_PACKAGES_DIRNAME: &str = "standalone";
const ZSH_DIRNAME: &str = "zsh";
static INSTALL_CONTEXT: OnceLock<InstallContext> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandalonePlatform {
    Unix,
    Windows,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeepSeekVibingPackageLayout {
    /// The package root that contains the metadata file and layout directories.
    pub package_dir: AbsolutePathBuf,
    /// Directory containing the DeepSeekVibing entrypoint executable.
    pub bin_dir: AbsolutePathBuf,
    /// Directory containing managed helper binaries and data files, when present.
    pub resources_dir: Option<AbsolutePathBuf>,
    /// Folder that should be prepended to the PATH, when present.
    pub path_dir: Option<AbsolutePathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallContext {
    pub method: InstallMethod,
    pub package_layout: Option<DeepSeekVibingPackageLayout>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallMethod {
    Standalone {
        /// The managed standalone release directory. Legacy installs use paths
        /// such as
        /// `~/.deepseek-vibing/packages/standalone/releases/0.111.0-x86_64-unknown-linux-musl`.
        /// Package-layout installs use the package root that contains `bin/`,
        /// `deepseek-vibing-resources/`, and `deepseek-vibing-path/`.
        release_dir: AbsolutePathBuf,
        /// The bundled resource directory for managed dependencies.
        resources_dir: Option<AbsolutePathBuf>,
        /// The platform of the standalone release, either `Unix` or `Windows`.
        platform: StandalonePlatform,
    },
    /// A DeepSeekVibing binary launched through the npm-managed `deepseek_vibing.js` shim.
    Npm,
    /// A DeepSeekVibing binary launched through the bun-managed `deepseek_vibing.js` shim.
    Bun,
    /// A DeepSeekVibing binary that appears to come from a Homebrew install prefix.
    Brew,
    /// Any other execution environment.
    ///
    /// This commonly covers `cargo run`, app-bundled DeepSeekVibing binaries, custom
    /// internal launchers, and tests that execute DeepSeekVibing from an arbitrary path.
    Other,
}

impl InstallContext {
    pub fn from_exe(
        is_macos: bool,
        current_exe: Option<&Path>,
        managed_by_npm: bool,
        managed_by_bun: bool,
    ) -> Self {
        let deepseek_vibing_home = deepseek_vibing_utils_home_dir::find_deepseek_vibing_home().ok();
        Self::from_exe_with_deepseek_vibing_home(
            is_macos,
            current_exe,
            managed_by_npm,
            managed_by_bun,
            deepseek_vibing_home.as_deref(),
        )
    }

    fn from_exe_with_deepseek_vibing_home(
        is_macos: bool,
        current_exe: Option<&Path>,
        managed_by_npm: bool,
        managed_by_bun: bool,
        deepseek_vibing_home: Option<&Path>,
    ) -> Self {
        let package_layout = current_exe.and_then(DeepSeekVibingPackageLayout::from_exe);
        let method = if managed_by_npm {
            InstallMethod::Npm
        } else if managed_by_bun {
            InstallMethod::Bun
        } else if let Some(exe_path) = current_exe {
            install_method_from_exe(
                exe_path,
                deepseek_vibing_home,
                package_layout.as_ref(),
                is_macos,
            )
        } else {
            InstallMethod::Other
        };

        Self {
            method,
            package_layout,
        }
    }

    pub fn current() -> &'static Self {
        INSTALL_CONTEXT.get_or_init(|| {
            let current_exe = std::env::current_exe().ok();
            let managed_by_npm = std::env::var_os("DEEPSEEK_VIBING_MANAGED_BY_NPM").is_some();
            let managed_by_bun = std::env::var_os("DEEPSEEK_VIBING_MANAGED_BY_BUN").is_some();
            Self::from_exe(
                cfg!(target_os = "macos"),
                current_exe.as_deref(),
                managed_by_npm,
                managed_by_bun,
            )
        })
    }

    pub fn rg_command(&self) -> PathBuf {
        if let Some(package_layout) = &self.package_layout
            && let Some(path_dir) = &package_layout.path_dir
        {
            let bundled_rg = path_dir.join(default_rg_command());
            if bundled_rg.is_file() {
                return bundled_rg.into_path_buf();
            }
        }

        if let InstallMethod::Standalone {
            resources_dir: Some(resources_dir),
            ..
        } = &self.method
        {
            let bundled_rg = resources_dir.join(default_rg_command());
            if bundled_rg.is_file() {
                return bundled_rg.into_path_buf();
            }
        }

        default_rg_command()
    }

    pub fn bundled_resource(&self, file_name: impl AsRef<Path>) -> Option<AbsolutePathBuf> {
        if let Some(package_layout) = &self.package_layout
            && let Some(resources_dir) = &package_layout.resources_dir
        {
            let resource = resources_dir.join(file_name.as_ref());
            if resource.is_file() {
                return Some(resource);
            }
        }

        if let InstallMethod::Standalone {
            resources_dir: Some(resources_dir),
            ..
        } = &self.method
        {
            let resource = resources_dir.join(file_name);
            if resource.is_file() {
                return Some(resource);
            }
        }

        None
    }

    pub fn bundled_zsh_path(&self) -> Option<AbsolutePathBuf> {
        if cfg!(windows) {
            None
        } else {
            self.bundled_resource(zsh_resource_path())
        }
    }

    pub fn bundled_zsh_bin_dir(&self) -> Option<AbsolutePathBuf> {
        self.bundled_zsh_path()?.parent()
    }
}

impl DeepSeekVibingPackageLayout {
    fn from_exe(exe_path: &Path) -> Option<Self> {
        let canonical_exe = canonical_absolute_path(exe_path)?;
        let bin_dir = canonical_exe.parent()?;
        if bin_dir.file_name() != Some(OsStr::new(BIN_DIRNAME)) {
            return None;
        }

        let package_dir = bin_dir.parent()?;
        if !package_dir.join(PACKAGE_METADATA_FILENAME).is_file() {
            return None;
        }

        Some(Self {
            resources_dir: existing_dir(package_dir.join(RESOURCES_DIRNAME)),
            path_dir: existing_dir(package_dir.join(PATH_DIRNAME)),
            package_dir,
            bin_dir,
        })
    }
}

fn install_method_from_exe(
    exe_path: &Path,
    deepseek_vibing_home: Option<&Path>,
    package_layout: Option<&DeepSeekVibingPackageLayout>,
    is_macos: bool,
) -> InstallMethod {
    if let Some(standalone_method) =
        standalone_install_method(exe_path, deepseek_vibing_home, package_layout)
    {
        return standalone_method;
    }

    if is_macos && (exe_path.starts_with("/opt/homebrew") || exe_path.starts_with("/usr/local")) {
        InstallMethod::Brew
    } else {
        InstallMethod::Other
    }
}

fn standalone_install_method(
    exe_path: &Path,
    deepseek_vibing_home: Option<&Path>,
    package_layout: Option<&DeepSeekVibingPackageLayout>,
) -> Option<InstallMethod> {
    let canonical_deepseek_vibing_home = canonical_absolute_path(deepseek_vibing_home?)?;
    let release_dir = if let Some(package_layout) = package_layout {
        package_layout.package_dir.clone()
    } else {
        canonical_absolute_path(exe_path)?.parent()?
    };
    let releases_root = canonical_deepseek_vibing_home
        .join("packages")
        .join(STANDALONE_PACKAGES_DIRNAME)
        .join(RELEASES_DIRNAME);
    if !release_dir.starts_with(releases_root.as_path()) {
        return None;
    }

    let resources_dir = release_dir.join(RESOURCES_DIRNAME);
    Some(InstallMethod::Standalone {
        release_dir,
        resources_dir: resources_dir.is_dir().then_some(resources_dir),
        platform: standalone_platform(),
    })
}

fn canonical_absolute_path(path: &Path) -> Option<AbsolutePathBuf> {
    let canonical_path = std::fs::canonicalize(path).ok()?;
    AbsolutePathBuf::from_absolute_path(canonical_path).ok()
}

fn standalone_platform() -> StandalonePlatform {
    if cfg!(windows) {
        StandalonePlatform::Windows
    } else {
        StandalonePlatform::Unix
    }
}

fn existing_dir(path: AbsolutePathBuf) -> Option<AbsolutePathBuf> {
    path.is_dir().then_some(path)
}

fn default_rg_command() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("rg.exe")
    } else {
        PathBuf::from("rg")
    }
}

fn zsh_resource_path() -> PathBuf {
    PathBuf::from(ZSH_DIRNAME).join(BIN_DIRNAME).join("zsh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::fs;

    const TEST_RESOURCE_NAME: &str = "deepseek-vibing-test-helper";

    #[test]
    fn detects_standalone_install_from_release_layout() -> std::io::Result<()> {
        let deepseek_vibing_home = tempfile::tempdir()?;
        let release_dir = deepseek_vibing_home
            .path()
            .join("packages/standalone/releases/1.2.3-x86_64-unknown-linux-musl");
        let resources_dir = release_dir.join(RESOURCES_DIRNAME);
        fs::create_dir_all(&resources_dir)?;
        let exe_path = release_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;
        fs::write(resources_dir.join(default_rg_command()), "")?;
        fs::write(resources_dir.join(TEST_RESOURCE_NAME), "")?;
        let canonical_release_dir =
            AbsolutePathBuf::from_absolute_path(release_dir.canonicalize()?)?;
        let canonical_resources_dir =
            AbsolutePathBuf::from_absolute_path(resources_dir.canonicalize()?)?;

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ Some(deepseek_vibing_home.path()),
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Standalone {
                    release_dir: canonical_release_dir,
                    resources_dir: Some(canonical_resources_dir.clone()),
                    platform: standalone_platform(),
                },
                package_layout: None,
            }
        );
        assert_eq!(
            context.bundled_resource(TEST_RESOURCE_NAME),
            Some(canonical_resources_dir.join(TEST_RESOURCE_NAME))
        );
        Ok(())
    }

    #[test]
    fn standalone_rg_falls_back_when_resources_are_missing() -> std::io::Result<()> {
        let deepseek_vibing_home = tempfile::tempdir()?;
        let release_dir = deepseek_vibing_home
            .path()
            .join("packages/standalone/releases/1.2.3-x86_64-unknown-linux-musl");
        fs::create_dir_all(&release_dir)?;
        let exe_path = release_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ Some(deepseek_vibing_home.path()),
        );
        assert_eq!(context.rg_command(), default_rg_command());
        Ok(())
    }

    #[test]
    fn detects_package_layout_independently_from_install_method() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        let resources_dir = package_dir.path().join(RESOURCES_DIRNAME);
        let path_dir = package_dir.path().join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&resources_dir)?;
        fs::create_dir_all(&path_dir)?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;
        fs::write(resources_dir.join(TEST_RESOURCE_NAME), "")?;
        fs::write(path_dir.join(default_rg_command()), "")?;
        if !cfg!(windows) {
            let zsh_path = resources_dir.join(zsh_resource_path());
            fs::create_dir_all(zsh_path.parent().expect("zsh path should have parent"))?;
            fs::write(&zsh_path, "")?;
        }
        let canonical_package_dir =
            AbsolutePathBuf::from_absolute_path(package_dir.path().canonicalize()?)?;
        let canonical_bin_dir = AbsolutePathBuf::from_absolute_path(bin_dir.canonicalize()?)?;
        let canonical_resources_dir =
            AbsolutePathBuf::from_absolute_path(resources_dir.canonicalize()?)?;
        let canonical_path_dir = AbsolutePathBuf::from_absolute_path(path_dir.canonicalize()?)?;
        let package_layout = DeepSeekVibingPackageLayout {
            package_dir: canonical_package_dir,
            bin_dir: canonical_bin_dir,
            resources_dir: Some(canonical_resources_dir.clone()),
            path_dir: Some(canonical_path_dir.clone()),
        };

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Other,
                package_layout: Some(package_layout),
            }
        );
        assert_eq!(
            context.rg_command(),
            canonical_path_dir
                .join(default_rg_command())
                .into_path_buf()
        );
        assert_eq!(
            context.bundled_resource(TEST_RESOURCE_NAME),
            Some(canonical_resources_dir.join(TEST_RESOURCE_NAME))
        );
        if cfg!(windows) {
            assert_eq!(context.bundled_zsh_path(), None);
            assert_eq!(context.bundled_zsh_bin_dir(), None);
        } else {
            assert_eq!(
                context.bundled_zsh_path(),
                Some(canonical_resources_dir.join(zsh_resource_path()))
            );
            assert_eq!(
                context.bundled_zsh_bin_dir(),
                Some(canonical_resources_dir.join(ZSH_DIRNAME).join(BIN_DIRNAME))
            );
        }
        Ok(())
    }

    #[test]
    fn standalone_package_layout_keeps_standalone_install_method() -> std::io::Result<()> {
        let deepseek_vibing_home = tempfile::tempdir()?;
        let package_dir = deepseek_vibing_home
            .path()
            .join("packages/standalone/releases/1.2.3-x86_64-unknown-linux-musl");
        let bin_dir = package_dir.join(BIN_DIRNAME);
        let resources_dir = package_dir.join(RESOURCES_DIRNAME);
        let path_dir = package_dir.join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&resources_dir)?;
        fs::create_dir_all(&path_dir)?;
        fs::write(package_dir.join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;
        fs::write(resources_dir.join(TEST_RESOURCE_NAME), "")?;
        fs::write(path_dir.join(default_rg_command()), "")?;
        let canonical_package_dir =
            AbsolutePathBuf::from_absolute_path(package_dir.canonicalize()?)?;
        let canonical_bin_dir = AbsolutePathBuf::from_absolute_path(bin_dir.canonicalize()?)?;
        let canonical_resources_dir =
            AbsolutePathBuf::from_absolute_path(resources_dir.canonicalize()?)?;
        let canonical_path_dir = AbsolutePathBuf::from_absolute_path(path_dir.canonicalize()?)?;

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ Some(deepseek_vibing_home.path()),
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Standalone {
                    release_dir: canonical_package_dir.clone(),
                    resources_dir: Some(canonical_resources_dir.clone()),
                    platform: standalone_platform(),
                },
                package_layout: Some(DeepSeekVibingPackageLayout {
                    package_dir: canonical_package_dir,
                    bin_dir: canonical_bin_dir,
                    resources_dir: Some(canonical_resources_dir.clone()),
                    path_dir: Some(canonical_path_dir.clone()),
                }),
            }
        );
        assert_eq!(
            context.rg_command(),
            canonical_path_dir
                .join(default_rg_command())
                .into_path_buf()
        );
        assert_eq!(
            context.bundled_resource(TEST_RESOURCE_NAME),
            Some(canonical_resources_dir.join(TEST_RESOURCE_NAME))
        );
        Ok(())
    }

    #[test]
    fn npm_managed_package_keeps_package_layout() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        let path_dir = package_dir.path().join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&path_dir)?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;
        fs::write(path_dir.join(default_rg_command()), "")?;
        let canonical_path_dir = AbsolutePathBuf::from_absolute_path(path_dir.canonicalize()?)?;

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ true,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(context.method, InstallMethod::Npm);
        assert!(context.package_layout.is_some());
        assert_eq!(
            context.rg_command(),
            canonical_path_dir
                .join(default_rg_command())
                .into_path_buf()
        );
        Ok(())
    }

    #[test]
    fn standalone_package_rg_falls_back_when_deepseek_vibing_path_is_missing() -> std::io::Result<()>
    {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(context.rg_command(), default_rg_command());
        Ok(())
    }

    #[test]
    fn bundled_file_lookups_ignore_directories() -> std::io::Result<()> {
        let package_dir = tempfile::tempdir()?;
        let bin_dir = package_dir.path().join(BIN_DIRNAME);
        let resources_dir = package_dir.path().join(RESOURCES_DIRNAME);
        let path_dir = package_dir.path().join(PATH_DIRNAME);
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(resources_dir.join(TEST_RESOURCE_NAME))?;
        fs::create_dir_all(path_dir.join(default_rg_command()))?;
        fs::write(package_dir.path().join(PACKAGE_METADATA_FILENAME), "{}")?;
        let exe_path = bin_dir.join(if cfg!(windows) {
            "deepseek_vibing.exe"
        } else {
            "deepseek-vibing"
        });
        fs::write(&exe_path, "")?;

        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(&exe_path),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(context.rg_command(), default_rg_command());
        assert_eq!(context.bundled_resource(TEST_RESOURCE_NAME), None);
        Ok(())
    }

    #[test]
    fn npm_and_bun_take_precedence() {
        let npm_context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(Path::new("/tmp/codex")),
            /*managed_by_npm*/ true,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(
            npm_context,
            InstallContext {
                method: InstallMethod::Npm,
                package_layout: None,
            }
        );

        let bun_context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ false,
            /*current_exe*/ Some(Path::new("/tmp/codex")),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ true,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(
            bun_context,
            InstallContext {
                method: InstallMethod::Bun,
                package_layout: None,
            }
        );
    }

    #[test]
    fn brew_is_detected_on_macos_prefixes() {
        let context = InstallContext::from_exe_with_deepseek_vibing_home(
            /*is_macos*/ true,
            /*current_exe*/ Some(Path::new("/opt/homebrew/bin/deepseek-vibing")),
            /*managed_by_npm*/ false,
            /*managed_by_bun*/ false,
            /*deepseek_vibing_home*/ None,
        );
        assert_eq!(
            context,
            InstallContext {
                method: InstallMethod::Brew,
                package_layout: None,
            }
        );
    }
}
