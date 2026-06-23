use crate::config::EvalGatesConfig;
use crate::config::EvalMode;
use crate::trajectory::TrajectorySummary;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

const OVERTHINKING_REASONING_TOKEN_THRESHOLD: i64 = 4096;
const OVERTHINKING_REASONING_TO_OUTPUT_RATIO: i64 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalTask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub prompt: String,
    pub files: Vec<EvalTaskFile>,
    pub verify: EvalTaskVerify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalTaskFile {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalTaskVerify {
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalTaskStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalCommandOutput {
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalTaskResult {
    pub task_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub status: EvalTaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trajectory_paths: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<EvalCommandOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalMetrics {
    pub task_count: usize,
    pub attempted_task_count: usize,
    pub passed_task_count: usize,
    pub failed_task_count: usize,
    pub skipped_task_count: usize,
    pub trajectory_count: usize,
    pub total_turns: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_turns: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_total_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_accept_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_failure_recovery_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_error_rate: Option<f64>,
    pub overthinking_task_count: u64,
    pub interrupt_abort_rollback_count: u64,
    pub total_tool_calls: usize,
    pub tool_error_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_estimate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalGateCheck {
    pub name: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalGateResult {
    pub enforced: bool,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<EvalGateCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalRunReport {
    pub run_id: String,
    pub generated_at_ms: i64,
    pub mode: EvalMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trajectory_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_results: Vec<EvalTaskResult>,
    pub metrics: EvalMetrics,
    pub gate: EvalGateResult,
}

pub struct EvalReportInput<'a> {
    pub run_id: String,
    pub mode: EvalMode,
    pub dataset_dir: Option<PathBuf>,
    pub trajectory_root: Option<PathBuf>,
    pub task_results: Vec<EvalTaskResult>,
    pub trajectories: Vec<TrajectorySummary>,
    pub gates: &'a EvalGatesConfig,
    pub enforce: bool,
    pub baseline: Option<&'a EvalRunReport>,
}

pub fn load_eval_tasks(dataset_dir: &Path) -> anyhow::Result<Vec<EvalTask>> {
    let mut paths = Vec::new();
    collect_task_files(dataset_dir, &mut paths)?;
    paths.sort();

    let mut tasks = Vec::new();
    let mut ids = HashSet::new();
    for path in paths {
        let contents = std::fs::read_to_string(&path).map_err(|err| {
            anyhow::anyhow!("failed to read eval task `{}`: {err}", path.display())
        })?;
        let task: EvalTask = toml::from_str(&contents).map_err(|err| {
            anyhow::anyhow!("failed to parse eval task `{}`: {err}", path.display())
        })?;
        validate_task(&task, &path)?;
        if !ids.insert(task.id.clone()) {
            anyhow::bail!("duplicate eval task id `{}`", task.id);
        }
        tasks.push(task);
    }

    Ok(tasks)
}

pub fn materialize_task_workspace(task: &EvalTask, workspace: &Path) -> anyhow::Result<()> {
    if workspace.exists() {
        std::fs::remove_dir_all(workspace)?;
    }
    std::fs::create_dir_all(workspace)?;
    for file in &task.files {
        validate_relative_path(&file.path)?;
        let path = workspace.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &file.content)?;
    }
    Ok(())
}

pub fn build_eval_report(input: EvalReportInput<'_>) -> EvalRunReport {
    let metrics = compute_metrics(&input.task_results, &input.trajectories);
    let gate = evaluate_gates(
        &metrics,
        input.gates,
        input.enforce,
        input.baseline.map(|report| &report.metrics),
    );
    EvalRunReport {
        run_id: input.run_id,
        generated_at_ms: Utc::now().timestamp_millis(),
        mode: input.mode,
        dataset_dir: input.dataset_dir,
        trajectory_root: input.trajectory_root,
        task_results: input.task_results,
        metrics,
        gate,
    }
}

pub fn evaluate_gates(
    metrics: &EvalMetrics,
    gates: &EvalGatesConfig,
    enforce: bool,
    baseline: Option<&EvalMetrics>,
) -> EvalGateResult {
    let mut checks = Vec::new();
    checks.push(min_check(
        "min_pass_rate",
        metrics.pass_rate,
        gates.min_pass_rate,
    ));
    checks.push(max_check(
        "max_avg_turns",
        metrics.avg_turns,
        gates.max_avg_turns,
    ));
    checks.push(max_check(
        "max_tool_error_rate",
        metrics.tool_error_rate,
        gates.max_tool_error_rate,
    ));
    if let Some(max_overthinking_tasks) = gates.max_overthinking_tasks {
        checks.push(max_check(
            "max_overthinking_tasks",
            Some(metrics.overthinking_task_count as f64),
            max_overthinking_tasks as f64,
        ));
    }

    if let Some(baseline) = baseline {
        checks.push(min_check(
            "baseline_pass_rate",
            metrics.pass_rate,
            baseline.pass_rate.unwrap_or(0.0),
        ));
        if let Some(value) = baseline.avg_turns {
            checks.push(max_check("baseline_avg_turns", metrics.avg_turns, value));
        }
        if let Some(value) = baseline.tool_error_rate {
            checks.push(max_check(
                "baseline_tool_error_rate",
                metrics.tool_error_rate,
                value,
            ));
        }
    }

    let passed = checks.iter().all(|check| check.passed);
    EvalGateResult {
        enforced: enforce || baseline.is_some(),
        passed,
        checks,
    }
}

pub fn default_eval_work_dir() -> PathBuf {
    std::env::temp_dir().join("deepseek-vibing-eval")
}

fn compute_metrics(
    task_results: &[EvalTaskResult],
    trajectories: &[TrajectorySummary],
) -> EvalMetrics {
    let task_count = task_results.len();
    let passed_task_count = task_results
        .iter()
        .filter(|result| result.status == EvalTaskStatus::Passed)
        .count();
    let failed_task_count = task_results
        .iter()
        .filter(|result| {
            matches!(
                result.status,
                EvalTaskStatus::Failed | EvalTaskStatus::Error
            )
        })
        .count();
    let skipped_task_count = task_results
        .iter()
        .filter(|result| result.status == EvalTaskStatus::Skipped)
        .count();
    let attempted_task_count = task_count.saturating_sub(skipped_task_count);
    let pass_rate = (attempted_task_count > 0)
        .then_some(passed_task_count as f64 / attempted_task_count as f64);

    let trajectory_count = trajectories.len();
    let total_turns = trajectories
        .iter()
        .map(|trajectory| trajectory.turns.len())
        .sum::<usize>();
    let denominator = attempted_task_count.max(trajectory_count);
    let avg_turns = (denominator > 0).then_some(total_turns as f64 / denominator as f64);

    let mut input_tokens = 0_i64;
    let mut cached_input_tokens = 0_i64;
    let mut total_tokens = 0_i64;
    let mut total_tool_calls = 0_usize;
    let mut tool_error_count = 0_usize;
    let mut patch_total = 0_usize;
    let mut patch_success = 0_usize;
    let mut overthinking_task_count = 0_u64;
    let mut interrupt_abort_rollback_count = 0_u64;

    for trajectory in trajectories {
        let mut trajectory_reasoning_tokens = 0_i64;
        let mut trajectory_output_tokens = 0_i64;
        for turn in &trajectory.turns {
            if turn.status != "completed" {
                interrupt_abort_rollback_count += 1;
            }
            if let Some(usage) = turn.token_usage.as_ref() {
                input_tokens += usage.input_tokens;
                cached_input_tokens += usage.cached_input_tokens;
                total_tokens += usage.total_tokens;
                trajectory_reasoning_tokens += usage.reasoning_output_tokens;
                trajectory_output_tokens += usage.output_tokens;
            }
            for tool_call in &turn.tool_calls {
                total_tool_calls += 1;
                if tool_is_error(tool_call) {
                    tool_error_count += 1;
                }
                if tool_call.tool == "apply_patch" {
                    patch_total += 1;
                    if tool_call.success == Some(true) {
                        patch_success += 1;
                    }
                }
            }
        }
        if is_overthinking(trajectory_reasoning_tokens, trajectory_output_tokens) {
            overthinking_task_count += 1;
        }
    }

    let avg_total_tokens = (denominator > 0).then_some(total_tokens as f64 / denominator as f64);
    let cache_hit_ratio =
        (input_tokens > 0).then_some(cached_input_tokens as f64 / input_tokens as f64);
    let tool_error_rate =
        (total_tool_calls > 0).then_some(tool_error_count as f64 / total_tool_calls as f64);
    let patch_accept_rate = (patch_total > 0).then_some(patch_success as f64 / patch_total as f64);
    let test_failure_recovery_rate = compute_test_failure_recovery_rate(task_results, trajectories);

    EvalMetrics {
        task_count,
        attempted_task_count,
        passed_task_count,
        failed_task_count,
        skipped_task_count,
        trajectory_count,
        total_turns,
        pass_rate,
        avg_turns,
        avg_total_tokens,
        cache_hit_ratio,
        patch_accept_rate,
        test_failure_recovery_rate,
        tool_error_rate,
        overthinking_task_count,
        interrupt_abort_rollback_count,
        total_tool_calls,
        tool_error_count,
        cost_estimate: None,
    }
}

fn compute_test_failure_recovery_rate(
    task_results: &[EvalTaskResult],
    trajectories: &[TrajectorySummary],
) -> Option<f64> {
    let by_path = trajectories
        .iter()
        .map(|trajectory| (trajectory.path.clone(), trajectory))
        .collect::<HashMap<_, _>>();
    let mut recovered = 0_usize;
    let mut recoverable = 0_usize;
    for result in task_results {
        let had_non_zero = result
            .trajectory_paths
            .iter()
            .filter_map(|path| by_path.get(path))
            .any(|trajectory| {
                trajectory.turns.iter().any(|turn| {
                    turn.tool_calls
                        .iter()
                        .any(|tool| tool.exit_code.is_some_and(|code| code != 0))
                })
            });
        if had_non_zero {
            recoverable += 1;
            if result.status == EvalTaskStatus::Passed {
                recovered += 1;
            }
        }
    }
    (recoverable > 0).then_some(recovered as f64 / recoverable as f64)
}

fn tool_is_error(tool_call: &crate::trajectory::TrajectoryToolCall) -> bool {
    tool_call.error.is_some()
        || tool_call.exit_code.is_some_and(|code| code != 0)
        || matches!(tool_call.status.as_str(), "failed" | "declined")
}

fn is_overthinking(reasoning_tokens: i64, output_tokens: i64) -> bool {
    reasoning_tokens > OVERTHINKING_REASONING_TOKEN_THRESHOLD
        && reasoning_tokens > output_tokens.saturating_mul(OVERTHINKING_REASONING_TO_OUTPUT_RATIO)
}

fn min_check(name: &str, value: Option<f64>, threshold: f64) -> EvalGateCheck {
    let passed = value.is_none_or(|value| value + f64::EPSILON >= threshold);
    EvalGateCheck {
        name: name.to_string(),
        passed,
        value,
        threshold: Some(threshold),
        message: match value {
            Some(value) => format!("{name}: value {value:.3} >= {threshold:.3}"),
            None => format!("{name}: not applicable"),
        },
    }
}

fn max_check(name: &str, value: Option<f64>, threshold: f64) -> EvalGateCheck {
    let passed = value.is_none_or(|value| value <= threshold + f64::EPSILON);
    EvalGateCheck {
        name: name.to_string(),
        passed,
        value,
        threshold: Some(threshold),
        message: match value {
            Some(value) => format!("{name}: value {value:.3} <= {threshold:.3}"),
            None => format!("{name}: not applicable"),
        },
    }
}

fn collect_task_files(path: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if path.is_file() {
        if is_task_file(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(path)
        .map_err(|err| anyhow::anyhow!("failed to read eval dataset `{}`: {err}", path.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_task_files(&path, files)?;
        } else if file_type.is_file() && is_task_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_task_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "toml")
}

fn validate_task(task: &EvalTask, source: &Path) -> anyhow::Result<()> {
    if task.id.trim().is_empty() {
        anyhow::bail!("eval task `{}` has empty id", source.display());
    }
    if task.title.trim().is_empty() {
        anyhow::bail!("eval task `{}` has empty title", task.id);
    }
    if task.prompt.trim().is_empty() {
        anyhow::bail!("eval task `{}` has empty prompt", task.id);
    }
    if task.files.is_empty() {
        anyhow::bail!("eval task `{}` must include at least one file", task.id);
    }
    if task.verify.command.is_empty() {
        anyhow::bail!("eval task `{}` must include verify.command", task.id);
    }
    for file in &task.files {
        validate_relative_path(&file.path).map_err(|err| {
            anyhow::anyhow!("eval task `{}` has invalid file path: {err}", task.id)
        })?;
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> anyhow::Result<()> {
    if path.is_absolute() {
        anyhow::bail!("path `{}` must be relative", path.display());
    }
    let mut saw_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => saw_normal = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!(
                    "path `{}` may not escape the task workspace",
                    path.display()
                );
            }
        }
    }
    if !saw_normal {
        anyhow::bail!("path `{}` is empty", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::TrajectoryTurn;
    use crate::trajectory::TrajectoryUsage;

    #[test]
    fn rejects_path_escape() {
        let task = EvalTask {
            id: "bad".to_string(),
            title: "Bad".to_string(),
            tags: Vec::new(),
            prompt: "fix it".to_string(),
            files: vec![EvalTaskFile {
                path: PathBuf::from("../Cargo.toml"),
                content: String::new(),
            }],
            verify: EvalTaskVerify {
                command: vec!["cargo".to_string(), "test".to_string()],
            },
        };

        assert!(validate_task(&task, Path::new("bad.toml")).is_err());
    }

    #[test]
    fn loads_valid_task_and_materializes_workspace() {
        let dataset = tempfile::tempdir().expect("dataset tempdir");
        std::fs::write(
            dataset.path().join("task.toml"),
            r#"
id = "t1"
title = "Task one"
tags = ["rust"]
prompt = "fix it"

[[files]]
path = "Cargo.toml"
content = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"

[verify]
command = ["cargo", "test", "--quiet"]
"#,
        )
        .expect("write task");

        let tasks = load_eval_tasks(dataset.path()).expect("load tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");

        let workspace = tempfile::tempdir().expect("workspace tempdir");
        materialize_task_workspace(&tasks[0], workspace.path()).expect("materialize");
        assert!(workspace.path().join("Cargo.toml").exists());
    }

    #[test]
    fn rejects_duplicate_task_ids() {
        let dataset = tempfile::tempdir().expect("dataset tempdir");
        for name in ["a.toml", "b.toml"] {
            std::fs::write(
                dataset.path().join(name),
                r#"
id = "dup"
title = "Duplicate"
prompt = "fix it"

[[files]]
path = "src/lib.rs"
content = ""

[verify]
command = ["cargo", "test"]
"#,
            )
            .expect("write task");
        }

        let err = load_eval_tasks(dataset.path()).expect_err("duplicates fail");
        assert!(err.to_string().contains("duplicate eval task id `dup`"));
    }

    #[test]
    fn metrics_include_cache_tool_patch_and_overthinking() {
        let task_results = vec![EvalTaskResult {
            task_id: "t1".to_string(),
            title: "Task".to_string(),
            tags: Vec::new(),
            status: EvalTaskStatus::Passed,
            workspace: None,
            trajectory_paths: vec![PathBuf::from("/tmp/trajectory.jsonl")],
            verify: None,
            error: None,
            skip_reason: None,
        }];
        let trajectory = TrajectorySummary {
            trajectory_id: "tr1".to_string(),
            path: PathBuf::from("/tmp/trajectory.jsonl"),
            thread_id: "thread".to_string(),
            provider: "deepseek".to_string(),
            model: "deepseek-v4-pro".to_string(),
            reasoning_effort: Some("high".to_string()),
            started_at_ms: 0,
            cwd: PathBuf::from("/tmp"),
            sandbox_tier: crate::trajectory::TrajectorySandboxTier::Function,
            network: crate::trajectory::TrajectoryNetworkStatus::Restricted,
            turns: vec![TrajectoryTurn {
                turn_id: "turn".to_string(),
                status: "completed".to_string(),
                started_at_ms: None,
                completed_at_ms: None,
                duration_ms: None,
                token_usage: Some(TrajectoryUsage {
                    input_tokens: 100,
                    cached_input_tokens: 40,
                    non_cached_input_tokens: 60,
                    output_tokens: 100,
                    reasoning_output_tokens: 5000,
                    total_tokens: 5200,
                }),
                tool_calls: vec![
                    crate::trajectory::TrajectoryToolCall {
                        call_id: "patch".to_string(),
                        turn_id: "turn".to_string(),
                        tool: "apply_patch".to_string(),
                        status: "completed".to_string(),
                        command: None,
                        cwd: None,
                        duration_ms: None,
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        aggregated_output: None,
                        files_changed: vec!["src/lib.rs".to_string()],
                        success: Some(true),
                        error: None,
                    },
                    crate::trajectory::TrajectoryToolCall {
                        call_id: "test".to_string(),
                        turn_id: "turn".to_string(),
                        tool: "exec_command".to_string(),
                        status: "completed".to_string(),
                        command: Some(vec!["cargo".to_string(), "test".to_string()]),
                        cwd: None,
                        duration_ms: Some(1),
                        exit_code: Some(1),
                        stdout: None,
                        stderr: None,
                        aggregated_output: None,
                        files_changed: Vec::new(),
                        success: Some(false),
                        error: None,
                    },
                ],
                diff: None,
            }],
            raw_trace_root: None,
        };

        let report = build_eval_report(EvalReportInput {
            run_id: "run".to_string(),
            mode: EvalMode::Hybrid,
            dataset_dir: None,
            trajectory_root: None,
            task_results,
            trajectories: vec![trajectory],
            gates: &EvalGatesConfig::default(),
            enforce: false,
            baseline: None,
        });

        assert_eq!(report.metrics.pass_rate, Some(1.0));
        assert_eq!(report.metrics.cache_hit_ratio, Some(0.4));
        assert_eq!(report.metrics.patch_accept_rate, Some(1.0));
        assert_eq!(report.metrics.tool_error_rate, Some(0.5));
        assert_eq!(report.metrics.test_failure_recovery_rate, Some(1.0));
        assert_eq!(report.metrics.overthinking_task_count, 1);
        assert!(!report.gate.enforced);
    }

    #[test]
    fn enforce_gate_fails_low_pass_rate() {
        let metrics = EvalMetrics {
            task_count: 1,
            attempted_task_count: 1,
            passed_task_count: 0,
            failed_task_count: 1,
            skipped_task_count: 0,
            trajectory_count: 0,
            total_turns: 0,
            pass_rate: Some(0.0),
            avg_turns: Some(1.0),
            avg_total_tokens: None,
            cache_hit_ratio: None,
            patch_accept_rate: None,
            test_failure_recovery_rate: None,
            tool_error_rate: Some(0.0),
            overthinking_task_count: 0,
            interrupt_abort_rollback_count: 0,
            total_tool_calls: 0,
            tool_error_count: 0,
            cost_estimate: None,
        };
        let gate = evaluate_gates(&metrics, &EvalGatesConfig::default(), true, None);

        assert!(gate.enforced);
        assert!(!gate.passed);
    }
}
