use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::config::DeepSeekContextPrefixConfig;
use regex_lite::Regex;
use sha2::Digest;
use sha2::Sha256;
use tracing::debug;
use tracing::warn;
use wildmatch::WildMatch;

const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    ".git/**",
    "target",
    "target/**",
    "target-*",
    "target-*/**",
    "node_modules",
    "node_modules/**",
    ".pnpm-store",
    ".pnpm-store/**",
    "bazel-*",
    "bazel-*/**",
    ".cache",
    ".cache/**",
    "dist",
    "dist/**",
    "build",
    "build/**",
    "out",
    "out/**",
];

const MANIFEST_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "deno.json",
    "deno.jsonc",
    "tsconfig.json",
    "pnpm-workspace.yaml",
    "pnpm-workspace.yml",
    "MODULE.bazel",
    "BUILD.bazel",
    "flake.nix",
];

const LOCK_FILES: &[&str] = &[
    "Cargo.lock",
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "bun.lockb",
    "poetry.lock",
    "uv.lock",
    "go.sum",
    "MODULE.bazel.lock",
    "flake.lock",
];

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "swift", "c",
    "cc", "cpp", "cxx", "h", "hpp",
];

static RUST_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:fn|struct|enum|trait|mod|type|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .expect("valid Rust symbol regex")
});

static TS_JS_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function|class|interface|type|const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)",
    )
    .expect("valid TypeScript/JavaScript symbol regex")
});

static PYTHON_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:async\s+)?(?:def|class)\s+([A-Za-z_][A-Za-z0-9_]*)")
        .expect("valid Python symbol regex")
});

static GO_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:func|type|const|var)\s+(?:\([^)]+\)\s+)?([A-Za-z_][A-Za-z0-9_]*)")
        .expect("valid Go symbol regex")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoContextPrefix {
    pub(crate) files: Vec<RepoContextFileBlock>,
    omitted_files: usize,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoContextFileBlock {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) size_bytes: usize,
    pub(crate) exports: Vec<String>,
    pub(crate) summary: String,
    pub(crate) content_anchor: String,
    category: RepoContextCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RepoContextCategory {
    Manifest,
    Docs,
    Lock,
    Source,
    Included,
}

#[derive(Debug, Clone)]
struct Candidate {
    rel_path: String,
    abs_path: PathBuf,
    category: RepoContextCategory,
}

pub(crate) fn build_repo_context_prefix(
    root: &Path,
    config: &DeepSeekContextPrefixConfig,
) -> Option<String> {
    if !config.enabled || config.max_bytes == 0 || config.max_files == 0 {
        return None;
    }

    match collect_repo_context_prefix(root, config) {
        Ok(prefix) => {
            if prefix.files.is_empty() {
                None
            } else {
                Some(prefix.render(config.max_bytes))
            }
        }
        Err(err) => {
            warn!(error = %err, cwd = %root.display(), "failed to build DeepSeek repo context prefix");
            None
        }
    }
}

fn collect_repo_context_prefix(
    root: &Path,
    config: &DeepSeekContextPrefixConfig,
) -> std::io::Result<RepoContextPrefix> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let include_patterns = compile_patterns(&config.include);
    let mut exclude_patterns = compile_patterns(&config.exclude);
    exclude_patterns.extend(
        DEFAULT_EXCLUDES
            .iter()
            .map(|pattern| WildMatch::new(pattern)),
    );

    let mut candidates = collect_candidates(&root, &include_patterns, &exclude_patterns)?;
    candidates.sort_by(compare_candidates);

    let omitted_files = candidates.len().saturating_sub(config.max_files);
    let mut files = Vec::new();
    for candidate in candidates.into_iter().take(config.max_files) {
        match read_file_block(&candidate, config.max_file_bytes) {
            Ok(Some(block)) => files.push(block),
            Ok(None) => {}
            Err(err) => {
                debug!(
                    error = %err,
                    path = %candidate.abs_path.display(),
                    "skipping unreadable repo context file"
                );
            }
        }
    }

    Ok(RepoContextPrefix {
        files,
        omitted_files,
        truncated: omitted_files > 0,
    })
}

fn compile_patterns(patterns: &[String]) -> Vec<WildMatch> {
    patterns
        .iter()
        .filter_map(|pattern| {
            let trimmed = pattern.trim();
            (!trimmed.is_empty()).then(|| WildMatch::new(trimmed))
        })
        .collect()
}

fn collect_candidates(
    root: &Path,
    include_patterns: &[WildMatch],
    exclude_patterns: &[WildMatch],
) -> std::io::Result<Vec<Candidate>> {
    let mut candidates = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = fs::read_dir(&dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let Ok(rel_path) = path.strip_prefix(root) else {
                continue;
            };
            let rel = normalize_path(rel_path);
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                if !matches_any(&rel, exclude_patterns) {
                    stack.push(path);
                }
                continue;
            }

            if !file_type.is_file() || matches_any(&rel, exclude_patterns) {
                continue;
            }

            let category = classify_path(&rel, include_patterns);
            if let Some(category) = category {
                candidates.push(Candidate {
                    rel_path: rel,
                    abs_path: path,
                    category,
                });
            }
        }
    }

    Ok(candidates)
}

fn classify_path(rel_path: &str, include_patterns: &[WildMatch]) -> Option<RepoContextCategory> {
    if !include_patterns.is_empty() {
        return matches_any(rel_path, include_patterns).then_some(RepoContextCategory::Included);
    }

    let file_name = Path::new(rel_path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(rel_path);
    if MANIFEST_FILES.contains(&file_name) {
        return Some(RepoContextCategory::Manifest);
    }
    if file_name.eq_ignore_ascii_case("AGENTS.md")
        || file_name.to_ascii_lowercase().starts_with("readme")
    {
        return Some(RepoContextCategory::Docs);
    }
    if LOCK_FILES.contains(&file_name) {
        return Some(RepoContextCategory::Lock);
    }
    if Path::new(rel_path)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| SOURCE_EXTENSIONS.contains(&ext))
    {
        return Some(RepoContextCategory::Source);
    }

    None
}

fn compare_candidates(left: &Candidate, right: &Candidate) -> Ordering {
    left.category
        .cmp(&right.category)
        .then_with(|| left.rel_path.cmp(&right.rel_path))
}

fn read_file_block(
    candidate: &Candidate,
    max_file_bytes: usize,
) -> std::io::Result<Option<RepoContextFileBlock>> {
    if max_file_bytes == 0 {
        return Ok(None);
    }

    let bytes = fs::read(&candidate.abs_path)?;
    let size_bytes = bytes.len();
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(None);
    };
    let content_anchor = truncate_utf8_bytes(text.trim(), max_file_bytes);
    let exports = extract_exports(&candidate.rel_path, text);
    let summary = summarize_file(&candidate.rel_path, text, &exports);

    Ok(Some(RepoContextFileBlock {
        path: candidate.rel_path.clone(),
        sha256: sha256_hex(&bytes),
        size_bytes,
        exports,
        summary,
        content_anchor,
        category: candidate.category,
    }))
}

impl RepoContextPrefix {
    fn render(&self, max_bytes: usize) -> String {
        let mut rendered = String::from(
            "<deepseek_repo_context_prefix>\n\
             Stable repo prefix for DeepSeek prompt cache. Dynamic task state, tool outputs, and diffs appear later in the conversation.\n",
        );
        for file in &self.files {
            let block = file.render();
            if rendered
                .len()
                .saturating_add(block.len())
                .saturating_add(64)
                > max_bytes
            {
                break;
            }
            rendered.push_str(&block);
        }
        if self.truncated {
            rendered.push_str(&format!(
                "<omitted_files count=\"{}\" reason=\"max_files\" />\n",
                self.omitted_files
            ));
        }
        rendered.push_str("</deepseek_repo_context_prefix>");
        rendered
    }
}

impl RepoContextFileBlock {
    fn render(&self) -> String {
        let exports = if self.exports.is_empty() {
            String::new()
        } else {
            self.exports.join(", ")
        };
        format!(
            "<file path=\"{}\" sha256=\"{}\" size_bytes=\"{}\" category=\"{:?}\" exports=\"{}\">\nsummary: {}\ncontent_anchor:\n{}\n</file>\n",
            escape_attr(&self.path),
            self.sha256,
            self.size_bytes,
            self.category,
            escape_attr(&exports),
            escape_text(&self.summary),
            escape_text(&self.content_anchor),
        )
    }
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn matches_any(path: &str, patterns: &[WildMatch]) -> bool {
    patterns.iter().any(|pattern| pattern.matches(path))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn extract_exports(path: &str, text: &str) -> Vec<String> {
    let extension = Path::new(path).extension().and_then(OsStr::to_str);
    let regex = match extension {
        Some("rs") => Some(&*RUST_SYMBOL_RE),
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") => Some(&*TS_JS_SYMBOL_RE),
        Some("py") => Some(&*PYTHON_SYMBOL_RE),
        Some("go") => Some(&*GO_SYMBOL_RE),
        _ => None,
    };
    let Some(regex) = regex else {
        return Vec::new();
    };

    let mut exports = Vec::new();
    for line in text.lines().map(str::trim_start) {
        let Some(captures) = regex.captures(line) else {
            continue;
        };
        let Some(name) = captures.get(1).map(|name| name.as_str().to_string()) else {
            continue;
        };
        if !exports.contains(&name) {
            exports.push(name);
        }
        if exports.len() >= 24 {
            break;
        }
    }
    exports
}

fn summarize_file(path: &str, text: &str, exports: &[String]) -> String {
    if !exports.is_empty() {
        return format!("{} exports {}", path, exports.join(", "));
    }
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("empty file");
    format!("{path}: {}", truncate_utf8_bytes(first_line, 180))
}

fn truncate_utf8_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut truncated = text[..end].to_string();
    truncated.push_str("\n[truncated]");
    truncated
}

fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, text).expect("write test file");
    }

    #[test]
    fn context_prefix_is_deterministic_and_contains_file_metadata() {
        let temp = tempfile::tempdir().expect("create temp dir");
        write(
            &temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        );
        write(
            &temp.path().join("src/lib.rs"),
            "pub struct Demo;\npub fn run() {}\n",
        );
        write(&temp.path().join("README.md"), "# Demo\n");

        let config = DeepSeekContextPrefixConfig::default();
        let first = build_repo_context_prefix(temp.path(), &config).expect("prefix");
        let second = build_repo_context_prefix(temp.path(), &config).expect("prefix");

        assert_eq!(first, second);
        assert!(first.contains("path=\"Cargo.toml\""));
        assert!(first.contains("sha256=\""));
        assert!(first.contains("size_bytes=\""));
        assert!(first.contains("exports=\"Demo, run\""));
        assert!(first.contains("content_anchor:"));
    }

    #[test]
    fn context_prefix_excludes_generated_directories_by_default() {
        let temp = tempfile::tempdir().expect("create temp dir");
        write(
            &temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        );
        write(
            &temp.path().join("target/generated.rs"),
            "pub fn hidden() {}\n",
        );

        let config = DeepSeekContextPrefixConfig::default();
        let prefix = build_repo_context_prefix(temp.path(), &config).expect("prefix");

        assert!(prefix.contains("Cargo.toml"));
        assert!(!prefix.contains("generated.rs"));
        assert!(!prefix.contains("hidden"));
    }

    #[test]
    fn context_prefix_honors_include_and_exclude_overrides() {
        let temp = tempfile::tempdir().expect("create temp dir");
        write(&temp.path().join("src/keep.rs"), "pub fn keep() {}\n");
        write(&temp.path().join("src/drop.rs"), "pub fn drop_me() {}\n");
        write(&temp.path().join("README.md"), "# ignored by include\n");

        let config = DeepSeekContextPrefixConfig {
            include: vec!["src/*.rs".to_string()],
            exclude: vec!["src/drop.rs".to_string()],
            ..DeepSeekContextPrefixConfig::default()
        };
        let prefix = build_repo_context_prefix(temp.path(), &config).expect("prefix");

        assert!(prefix.contains("src/keep.rs"));
        assert!(!prefix.contains("src/drop.rs"));
        assert!(!prefix.contains("README.md"));
    }

    #[test]
    fn context_prefix_respects_size_and_file_limits() {
        let temp = tempfile::tempdir().expect("create temp dir");
        write(
            &temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        );
        write(&temp.path().join("src/a.rs"), "pub fn a() {}\n");
        write(&temp.path().join("src/b.rs"), "pub fn b() {}\n");

        let config = DeepSeekContextPrefixConfig {
            max_bytes: 500,
            max_file_bytes: 8,
            max_files: 1,
            ..DeepSeekContextPrefixConfig::default()
        };
        let prefix = build_repo_context_prefix(temp.path(), &config).expect("prefix");

        assert!(prefix.len() <= 500);
        assert_eq!(prefix.matches("<file ").count(), 1);
        assert!(prefix.contains("[truncated]"));
        assert!(prefix.contains("omitted_files"));
    }
}
