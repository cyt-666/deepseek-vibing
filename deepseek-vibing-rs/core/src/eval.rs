use crate::config::EvalGatesConfig;
use crate::config::EvalMode;
use crate::config::EvalPricingConfig;
use crate::config::EvalPricingModelConfig;
use crate::trajectory::TrajectorySummary;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
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
    pub cost_estimate: Option<EvalCostEstimate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCostEstimate {
    pub currency: String,
    pub total: f64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub by_model: BTreeMap<String, EvalModelCostEstimate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalModelCostEstimate {
    pub total: f64,
    pub cache_hit_input_tokens: i64,
    pub cache_miss_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_comparison: Option<EvalBaselineComparison>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalBaselineComparison {
    pub baseline_run_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metric_deltas: Vec<EvalMetricDelta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_regressions: Vec<EvalTaskStatusChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_recoveries: Vec<EvalTaskStatusChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalMetricDelta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalTaskStatusChange {
    pub task_id: String,
    pub baseline_status: EvalTaskStatus,
    pub current_status: EvalTaskStatus,
}

pub struct EvalReportInput<'a> {
    pub run_id: String,
    pub mode: EvalMode,
    pub dataset_dir: Option<PathBuf>,
    pub trajectory_root: Option<PathBuf>,
    pub task_results: Vec<EvalTaskResult>,
    pub trajectories: Vec<TrajectorySummary>,
    pub gates: &'a EvalGatesConfig,
    pub pricing: Option<&'a EvalPricingConfig>,
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
    let metrics = compute_metrics(&input.task_results, &input.trajectories, input.pricing);
    let baseline_comparison = input
        .baseline
        .map(|baseline| compare_to_baseline(&metrics, &input.task_results, baseline));
    let mut gate = evaluate_gates(
        &metrics,
        input.gates,
        input.enforce,
        input.baseline.map(|report| &report.metrics),
    );
    if let Some(comparison) = baseline_comparison.as_ref()
        && !comparison.task_regressions.is_empty()
    {
        gate.enforced = true;
        gate.passed = false;
        gate.checks.push(EvalGateCheck {
            name: "baseline_task_regressions".to_string(),
            passed: false,
            value: Some(comparison.task_regressions.len() as f64),
            threshold: Some(0.0),
            message: format!(
                "baseline_task_regressions: {} previously passing task(s) regressed",
                comparison.task_regressions.len()
            ),
        });
    }
    EvalRunReport {
        run_id: input.run_id,
        generated_at_ms: Utc::now().timestamp_millis(),
        mode: input.mode,
        dataset_dir: input.dataset_dir,
        trajectory_root: input.trajectory_root,
        task_results: input.task_results,
        metrics,
        gate,
        baseline_comparison,
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
    pricing: Option<&EvalPricingConfig>,
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
        cost_estimate: compute_cost_estimate(trajectories, pricing),
    }
}

fn compute_cost_estimate(
    trajectories: &[TrajectorySummary],
    pricing: Option<&EvalPricingConfig>,
) -> Option<EvalCostEstimate> {
    let pricing = pricing?;
    let mut by_model = BTreeMap::<String, EvalModelCostEstimate>::new();
    for trajectory in trajectories {
        let Some(model_pricing) = pricing.models.get(&trajectory.model) else {
            continue;
        };
        let model_total =
            by_model
                .entry(trajectory.model.clone())
                .or_insert_with(|| EvalModelCostEstimate {
                    total: 0.0,
                    cache_hit_input_tokens: 0,
                    cache_miss_input_tokens: 0,
                    output_tokens: 0,
                    reasoning_output_tokens: 0,
                });
        for turn in &trajectory.turns {
            let Some(usage) = turn.token_usage.as_ref() else {
                continue;
            };
            model_total.cache_hit_input_tokens += usage.cached_input_tokens;
            model_total.cache_miss_input_tokens += usage.non_cached_input_tokens;
            model_total.output_tokens += usage.output_tokens;
            model_total.reasoning_output_tokens += usage.reasoning_output_tokens;
            model_total.total += cost_for_usage(
                usage.cached_input_tokens,
                usage.non_cached_input_tokens,
                usage.output_tokens,
                usage.reasoning_output_tokens,
                model_pricing,
            );
        }
    }
    if by_model.is_empty() {
        return None;
    }
    let total = by_model.values().map(|model| model.total).sum::<f64>();
    Some(EvalCostEstimate {
        currency: pricing.currency.clone(),
        total,
        by_model,
    })
}

fn cost_for_usage(
    cache_hit_input_tokens: i64,
    cache_miss_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    pricing: &EvalPricingModelConfig,
) -> f64 {
    let million = 1_000_000.0;
    (cache_hit_input_tokens as f64 / million) * pricing.cache_hit_input_per_million_tokens
        + (cache_miss_input_tokens as f64 / million) * pricing.cache_miss_input_per_million_tokens
        + (output_tokens as f64 / million) * pricing.output_per_million_tokens
        + (reasoning_output_tokens as f64 / million) * pricing.reasoning_output_per_million_tokens
}

fn compare_to_baseline(
    metrics: &EvalMetrics,
    task_results: &[EvalTaskResult],
    baseline: &EvalRunReport,
) -> EvalBaselineComparison {
    let current_status_by_task = task_results
        .iter()
        .map(|result| (result.task_id.as_str(), &result.status))
        .collect::<HashMap<_, _>>();
    let mut task_regressions = Vec::new();
    let mut task_recoveries = Vec::new();
    for baseline_task in &baseline.task_results {
        let current_status = current_status_by_task
            .get(baseline_task.task_id.as_str())
            .copied()
            .cloned()
            .unwrap_or(EvalTaskStatus::Skipped);
        if baseline_task.status == EvalTaskStatus::Passed
            && current_status != EvalTaskStatus::Passed
        {
            task_regressions.push(EvalTaskStatusChange {
                task_id: baseline_task.task_id.clone(),
                baseline_status: baseline_task.status.clone(),
                current_status,
            });
        } else if baseline_task.status != EvalTaskStatus::Passed
            && current_status == EvalTaskStatus::Passed
        {
            task_recoveries.push(EvalTaskStatusChange {
                task_id: baseline_task.task_id.clone(),
                baseline_status: baseline_task.status.clone(),
                current_status,
            });
        }
    }

    EvalBaselineComparison {
        baseline_run_id: baseline.run_id.clone(),
        metric_deltas: vec![
            metric_delta("pass_rate", baseline.metrics.pass_rate, metrics.pass_rate),
            metric_delta("avg_turns", baseline.metrics.avg_turns, metrics.avg_turns),
            metric_delta(
                "avg_total_tokens",
                baseline.metrics.avg_total_tokens,
                metrics.avg_total_tokens,
            ),
            metric_delta(
                "cache_hit_ratio",
                baseline.metrics.cache_hit_ratio,
                metrics.cache_hit_ratio,
            ),
            metric_delta(
                "tool_error_rate",
                baseline.metrics.tool_error_rate,
                metrics.tool_error_rate,
            ),
            metric_delta(
                "patch_accept_rate",
                baseline.metrics.patch_accept_rate,
                metrics.patch_accept_rate,
            ),
            metric_delta(
                "overthinking_task_count",
                Some(baseline.metrics.overthinking_task_count as f64),
                Some(metrics.overthinking_task_count as f64),
            ),
            metric_delta(
                "cost_estimate",
                baseline
                    .metrics
                    .cost_estimate
                    .as_ref()
                    .map(|cost| cost.total),
                metrics.cost_estimate.as_ref().map(|cost| cost.total),
            ),
        ],
        task_regressions,
        task_recoveries,
    }
}

fn metric_delta(name: &str, baseline: Option<f64>, current: Option<f64>) -> EvalMetricDelta {
    EvalMetricDelta {
        name: name.to_string(),
        baseline,
        current,
        delta: baseline
            .zip(current)
            .map(|(baseline, current)| current - baseline),
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
            pricing: None,
            enforce: false,
            baseline: None,
        });

        assert_eq!(report.metrics.pass_rate, Some(1.0));
        assert_eq!(report.metrics.cache_hit_ratio, Some(0.4));
        assert_eq!(report.metrics.patch_accept_rate, Some(1.0));
        assert_eq!(report.metrics.tool_error_rate, Some(0.5));
        assert_eq!(report.metrics.test_failure_recovery_rate, Some(1.0));
        assert_eq!(report.metrics.overthinking_task_count, 1);
        assert_eq!(report.metrics.cost_estimate, None);
        assert!(!report.gate.enforced);
    }

    #[test]
    fn pricing_estimates_cost_by_model() {
        let pricing = EvalPricingConfig {
            currency: "USD".to_string(),
            models: [(
                "deepseek-v4-flash".to_string(),
                crate::config::EvalPricingModelConfig {
                    cache_hit_input_per_million_tokens: 1.0,
                    cache_miss_input_per_million_tokens: 2.0,
                    output_per_million_tokens: 3.0,
                    reasoning_output_per_million_tokens: 4.0,
                },
            )]
            .into_iter()
            .collect(),
        };
        let trajectory = TrajectorySummary {
            trajectory_id: "tr1".to_string(),
            path: PathBuf::from("/tmp/trajectory.jsonl"),
            thread_id: "thread".to_string(),
            provider: "deepseek".to_string(),
            model: "deepseek-v4-flash".to_string(),
            reasoning_effort: None,
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
                    input_tokens: 10,
                    cached_input_tokens: 1_000_000,
                    non_cached_input_tokens: 2_000_000,
                    output_tokens: 3_000_000,
                    reasoning_output_tokens: 4_000_000,
                    total_tokens: 10_000_000,
                }),
                tool_calls: Vec::new(),
                diff: None,
            }],
            raw_trace_root: None,
        };

        let report = build_eval_report(EvalReportInput {
            run_id: "run".to_string(),
            mode: EvalMode::Offline,
            dataset_dir: None,
            trajectory_root: None,
            task_results: Vec::new(),
            trajectories: vec![trajectory],
            gates: &EvalGatesConfig::default(),
            pricing: Some(&pricing),
            enforce: false,
            baseline: None,
        });

        let cost = report.metrics.cost_estimate.expect("cost estimate");
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.total, 30.0);
        assert_eq!(
            cost.by_model
                .get("deepseek-v4-flash")
                .expect("flash cost")
                .total,
            30.0
        );
    }

    #[test]
    fn baseline_comparison_flags_task_regressions_and_metric_deltas() {
        let baseline = EvalRunReport {
            run_id: "baseline".to_string(),
            generated_at_ms: 0,
            mode: EvalMode::Live,
            dataset_dir: None,
            trajectory_root: None,
            task_results: vec![EvalTaskResult {
                task_id: "task_a".to_string(),
                title: "Task A".to_string(),
                tags: Vec::new(),
                status: EvalTaskStatus::Passed,
                workspace: None,
                trajectory_paths: Vec::new(),
                verify: None,
                error: None,
                skip_reason: None,
            }],
            metrics: EvalMetrics {
                task_count: 1,
                attempted_task_count: 1,
                passed_task_count: 1,
                failed_task_count: 0,
                skipped_task_count: 0,
                trajectory_count: 0,
                total_turns: 0,
                pass_rate: Some(1.0),
                avg_turns: Some(1.0),
                avg_total_tokens: Some(100.0),
                cache_hit_ratio: Some(0.5),
                patch_accept_rate: Some(1.0),
                test_failure_recovery_rate: None,
                tool_error_rate: Some(0.0),
                overthinking_task_count: 0,
                interrupt_abort_rollback_count: 0,
                total_tool_calls: 0,
                tool_error_count: 0,
                cost_estimate: None,
            },
            gate: EvalGateResult {
                enforced: true,
                passed: true,
                checks: Vec::new(),
            },
            baseline_comparison: None,
        };
        let report = build_eval_report(EvalReportInput {
            run_id: "run".to_string(),
            mode: EvalMode::Live,
            dataset_dir: None,
            trajectory_root: None,
            task_results: vec![EvalTaskResult {
                task_id: "task_a".to_string(),
                title: "Task A".to_string(),
                tags: Vec::new(),
                status: EvalTaskStatus::Failed,
                workspace: None,
                trajectory_paths: Vec::new(),
                verify: None,
                error: None,
                skip_reason: None,
            }],
            trajectories: Vec::new(),
            gates: &EvalGatesConfig::default(),
            pricing: None,
            enforce: false,
            baseline: Some(&baseline),
        });

        let comparison = report.baseline_comparison.expect("baseline comparison");
        assert_eq!(comparison.baseline_run_id, "baseline");
        assert_eq!(comparison.task_regressions.len(), 1);
        assert!(comparison.metric_deltas.iter().any(|delta| {
            delta.name == "pass_rate" && delta.baseline == Some(1.0) && delta.current == Some(0.0)
        }));
        assert!(report.gate.enforced);
        assert!(!report.gate.passed);
        assert!(
            report
                .gate
                .checks
                .iter()
                .any(|check| check.name == "baseline_task_regressions")
        );
    }

    #[test]
    fn old_report_json_with_null_cost_and_no_baseline_comparison_still_loads() {
        let report: EvalRunReport = serde_json::from_str(
            r#"
{
  "run_id": "old",
  "generated_at_ms": 0,
  "mode": "offline",
  "metrics": {
    "task_count": 0,
    "attempted_task_count": 0,
    "passed_task_count": 0,
    "failed_task_count": 0,
    "skipped_task_count": 0,
    "trajectory_count": 0,
    "total_turns": 0,
    "overthinking_task_count": 0,
    "interrupt_abort_rollback_count": 0,
    "total_tool_calls": 0,
    "tool_error_count": 0,
    "cost_estimate": null
  },
  "gate": {
    "enforced": false,
    "passed": true
  }
}
"#,
        )
        .expect("old report should deserialize");

        assert_eq!(report.metrics.cost_estimate, None);
        assert_eq!(report.baseline_comparison, None);
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
