pub(crate) use deepseek_vibing_skills::install_system_skills;
pub(crate) use deepseek_vibing_skills::system_cache_root_dir;

use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;

pub(crate) fn uninstall_system_skills(deepseek_vibing_home: &AbsolutePathBuf) {
    let _ = std::fs::remove_dir_all(system_cache_root_dir(deepseek_vibing_home));
}
