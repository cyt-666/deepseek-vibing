//! Read-path helpers for DeepSeekVibing memories.
//!
//! This crate owns memory injection, memory citation parsing, and telemetry
//! classification for read access to the memory folder. It intentionally does
//! not depend on the memory write pipeline.

pub mod citations;
mod metrics;
pub mod usage;

use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;

pub fn memory_root(deepseek_vibing_home: &AbsolutePathBuf) -> AbsolutePathBuf {
    deepseek_vibing_home.join("memories")
}
