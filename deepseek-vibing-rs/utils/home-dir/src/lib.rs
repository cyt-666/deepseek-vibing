use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;
use dirs::home_dir;
use std::path::PathBuf;

pub const DEEPSEEK_VIBING_HOME_ENV: &str = "DEEPSEEK_VIBING_HOME";
pub const DEEPSEEK_VIBING_HOME_DIR: &str = ".deepseek-vibing";

/// Returns the path to the DeepSeek Vibing configuration directory, which can be
/// specified by the `DEEPSEEK_VIBING_HOME` environment variable. If not set,
/// defaults to `~/.deepseek-vibing`.
///
/// - If `DEEPSEEK_VIBING_HOME` is set, the value must exist and be a directory.
///   The value will be canonicalized and this function will Err otherwise.
/// - If `DEEPSEEK_VIBING_HOME` is not set, this function does not verify that
///   the directory exists.
///
/// The function name is retained for internal compatibility during the fork.
pub fn find_deepseek_vibing_home() -> std::io::Result<AbsolutePathBuf> {
    let deepseek_vibing_home_env = std::env::var(DEEPSEEK_VIBING_HOME_ENV)
        .ok()
        .filter(|val| !val.is_empty());
    find_deepseek_vibing_home_from_env(deepseek_vibing_home_env.as_deref(), None)
}

fn find_deepseek_vibing_home_from_env(
    deepseek_vibing_home_env: Option<&str>,
    _legacy_deepseek_vibing_home_env: Option<&str>,
) -> std::io::Result<AbsolutePathBuf> {
    match deepseek_vibing_home_env {
        Some(val) => {
            let path = PathBuf::from(val);
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "{DEEPSEEK_VIBING_HOME_ENV} points to {val:?}, but that path does not exist"
                    ),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read {DEEPSEEK_VIBING_HOME_ENV} {val:?}: {err}"),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "{DEEPSEEK_VIBING_HOME_ENV} points to {val:?}, but that path is not a directory"
                    ),
                ))
            } else {
                let canonical = path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize {DEEPSEEK_VIBING_HOME_ENV} {val:?}: {err}"),
                    )
                })?;
                AbsolutePathBuf::from_absolute_path(canonical)
            }
        }
        None => {
            let mut p = home_dir().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find home directory",
                )
            })?;
            p.push(DEEPSEEK_VIBING_HOME_DIR);
            AbsolutePathBuf::from_absolute_path(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DEEPSEEK_VIBING_HOME_DIR;
    use super::DEEPSEEK_VIBING_HOME_ENV;
    use super::find_deepseek_vibing_home_from_env;
    use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn find_deepseek_vibing_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-deepseek-vibing-home");
        let missing_str = missing
            .to_str()
            .expect("missing DeepSeek Vibing home path should be valid utf-8");

        let err = find_deepseek_vibing_home_from_env(Some(missing_str), None)
            .expect_err("missing DeepSeek Vibing home");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains(DEEPSEEK_VIBING_HOME_ENV),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_deepseek_vibing_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("deepseek-vibing-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file DeepSeek Vibing home path should be valid utf-8");

        let err = find_deepseek_vibing_home_from_env(Some(file_str), None)
            .expect_err("file DeepSeek Vibing home");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_deepseek_vibing_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp DeepSeek Vibing home path should be valid utf-8");

        let resolved =
            find_deepseek_vibing_home_from_env(Some(temp_str), None).expect("valid home");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_deepseek_vibing_home_without_env_uses_default_home_dir() {
        let resolved = find_deepseek_vibing_home_from_env(None, None).expect("default home");
        let mut expected = home_dir().expect("home dir");
        expected.push(DEEPSEEK_VIBING_HOME_DIR);
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn legacy_deepseek_vibing_home_env_is_ignored() {
        let legacy_home = TempDir::new().expect("legacy home");
        let resolved = find_deepseek_vibing_home_from_env(None, legacy_home.path().to_str())
            .expect("default home ignores DEEPSEEK_VIBING_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(DEEPSEEK_VIBING_HOME_DIR);
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }
}
