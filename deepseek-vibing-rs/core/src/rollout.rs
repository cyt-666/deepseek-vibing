use crate::config::Config;
pub use deepseek_vibing_rollout::ARCHIVED_SESSIONS_SUBDIR;
pub use deepseek_vibing_rollout::Cursor;
pub use deepseek_vibing_rollout::INTERACTIVE_SESSION_SOURCES;
pub use deepseek_vibing_rollout::RolloutRecorder;
pub use deepseek_vibing_rollout::RolloutRecorderParams;
pub use deepseek_vibing_rollout::SESSIONS_SUBDIR;
pub use deepseek_vibing_rollout::SessionMeta;
pub use deepseek_vibing_rollout::SortDirection;
pub use deepseek_vibing_rollout::ThreadItem;
pub use deepseek_vibing_rollout::ThreadSortKey;
pub use deepseek_vibing_rollout::ThreadsPage;
pub use deepseek_vibing_rollout::append_thread_name;
pub use deepseek_vibing_rollout::find_archived_thread_path_by_id_str;
#[deprecated(note = "use find_thread_path_by_id_str")]
pub use deepseek_vibing_rollout::find_conversation_path_by_id_str;
pub use deepseek_vibing_rollout::find_thread_meta_by_name_str;
pub use deepseek_vibing_rollout::find_thread_name_by_id;
pub use deepseek_vibing_rollout::find_thread_names_by_ids;
pub use deepseek_vibing_rollout::find_thread_path_by_id_str;
pub use deepseek_vibing_rollout::parse_cursor;
pub use deepseek_vibing_rollout::read_head_for_summary;
pub use deepseek_vibing_rollout::read_session_meta_line;
pub use deepseek_vibing_rollout::rollout_date_parts;

impl deepseek_vibing_rollout::RolloutConfigView for Config {
    fn deepseek_vibing_home(&self) -> &std::path::Path {
        self.deepseek_vibing_home.as_path()
    }

    fn sqlite_home(&self) -> &std::path::Path {
        self.sqlite_home.as_path()
    }

    fn cwd(&self) -> &std::path::Path {
        self.cwd.as_path()
    }

    fn model_provider_id(&self) -> &str {
        self.model_provider_id.as_str()
    }

    fn generate_memories(&self) -> bool {
        self.memories.generate_memories
    }
}

pub(crate) mod list {
    pub use deepseek_vibing_rollout::find_thread_path_by_id_str;
}

#[cfg(test)]
pub(crate) mod recorder {
    pub use deepseek_vibing_rollout::RolloutRecorder;
}

pub(crate) use crate::session_rollout_init_error::map_session_init_error;

pub(crate) mod truncation {
    pub(crate) use crate::thread_rollout_truncation::*;
}
