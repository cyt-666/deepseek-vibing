use std::collections::BTreeSet;

use crate::config::DeepSeekConfig;
use crate::session::TurnInput;
use deepseek_vibing_model_provider_info::DEEPSEEK_BETA_PROVIDER_ID;
use deepseek_vibing_model_provider_info::DEEPSEEK_DEFAULT_MODEL_ID;
use deepseek_vibing_model_provider_info::DEEPSEEK_PRO_MODEL_ID;
use deepseek_vibing_model_provider_info::DEEPSEEK_PROVIDER_ID;
use deepseek_vibing_protocol::openai_models::ReasoningEffort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeepSeekRouteDecision {
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) reason: String,
    pub(crate) fim_reserved: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DeepSeekRouteRequest<'a> {
    pub(crate) provider_id: &'a str,
    pub(crate) current_model: &'a str,
    pub(crate) current_reasoning_effort: Option<ReasoningEffort>,
    pub(crate) explicit_model: bool,
    pub(crate) explicit_reasoning_effort: bool,
    pub(crate) config: &'a DeepSeekConfig,
    pub(crate) input: &'a [TurnInput],
}

pub(crate) fn route_deepseek_turn(
    request: DeepSeekRouteRequest<'_>,
) -> Option<DeepSeekRouteDecision> {
    if !request.config.router.enabled
        || !matches!(
            request.provider_id,
            DEEPSEEK_PROVIDER_ID | DEEPSEEK_BETA_PROVIDER_ID
        )
        || (request.explicit_model && request.explicit_reasoning_effort)
    {
        return None;
    }

    let task_text = collect_task_text(request.input);
    let candidate = classify_task(&task_text);
    let model = if request.explicit_model {
        request.current_model.to_string()
    } else {
        candidate.model.to_string()
    };
    let reasoning_effort = if request.explicit_reasoning_effort {
        request.current_reasoning_effort
    } else {
        candidate.reasoning_effort
    };

    if model == request.current_model && reasoning_effort == request.current_reasoning_effort {
        return None;
    }

    Some(DeepSeekRouteDecision {
        model,
        reasoning_effort,
        reason: candidate.reason.to_string(),
        fim_reserved: false,
    })
}

#[derive(Debug, Clone, Copy)]
struct RouteCandidate {
    model: &'static str,
    reasoning_effort: Option<ReasoningEffort>,
    reason: &'static str,
}

fn classify_task(task_text: &str) -> RouteCandidate {
    let normalized = task_text.to_ascii_lowercase();
    let path_count = count_path_like_mentions(task_text);

    if contains_any(&normalized, HIGH_RISK_KEYWORDS) {
        return RouteCandidate {
            model: DEEPSEEK_PRO_MODEL_ID,
            reasoning_effort: Some(ReasoningEffort::High),
            reason: "high_risk_keywords",
        };
    }

    if path_count > 1 || contains_any(&normalized, MULTI_FILE_KEYWORDS) {
        return RouteCandidate {
            model: DEEPSEEK_PRO_MODEL_ID,
            reasoning_effort: Some(ReasoningEffort::High),
            reason: "multi_file_or_complex_change",
        };
    }

    if contains_any(&normalized, TEST_FAILURE_KEYWORDS)
        || (path_count == 1 && contains_any(&normalized, RUST_COMPLEXITY_KEYWORDS))
    {
        return RouteCandidate {
            model: DEEPSEEK_PRO_MODEL_ID,
            reasoning_effort: Some(ReasoningEffort::High),
            reason: "test_failure_or_rust_complexity",
        };
    }

    if contains_any(&normalized, SIMPLE_READ_ONLY_KEYWORDS)
        && !contains_any(&normalized, EDIT_KEYWORDS)
    {
        return RouteCandidate {
            model: DEEPSEEK_DEFAULT_MODEL_ID,
            reasoning_effort: Some(ReasoningEffort::Low),
            reason: "simple_read_only_task",
        };
    }

    RouteCandidate {
        model: DEEPSEEK_DEFAULT_MODEL_ID,
        reasoning_effort: Some(ReasoningEffort::Medium),
        reason: "default_small_or_single_file_task",
    }
}

fn collect_task_text(input: &[TurnInput]) -> String {
    let mut text = String::new();
    for item in input {
        let TurnInput::UserInput { content, .. } = item else {
            continue;
        };
        for content_item in content {
            if let deepseek_vibing_protocol::user_input::UserInput::Text {
                text: item_text, ..
            } = content_item
            {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(item_text);
            }
        }
    }
    text
}

fn count_path_like_mentions(text: &str) -> usize {
    let mut paths = BTreeSet::new();
    for token in text.split_whitespace() {
        let token = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '\'' | '"' | ',' | '.' | ':' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        });
        if token.contains('/')
            || PATH_EXTENSIONS
                .iter()
                .any(|extension| token.ends_with(extension))
        {
            paths.insert(token.to_string());
        }
    }
    paths.len()
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

const PATH_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".c", ".cc", ".cpp", ".h", ".hpp", ".toml",
    ".json", ".yaml", ".yml",
];

const HIGH_RISK_KEYWORDS: &[&str] = &[
    "unsafe",
    "ffi",
    "cuda",
    "c++",
    "cxx",
    "abi",
    "sandbox",
    "permission",
    "permissions",
    "auth",
    "authentication",
    "security",
    "vulnerability",
    "race",
    "deadlock",
    "concurrency",
    "migration",
    "database migration",
    "加密",
    "认证",
    "权限",
    "并发",
    "安全",
    "迁移",
];

const MULTI_FILE_KEYWORDS: &[&str] = &[
    "multi-file",
    "multiple files",
    "several files",
    "refactor",
    "architecture",
    "cross-module",
    "多文件",
    "多个文件",
    "重构",
    "架构",
    "跨模块",
];

const TEST_FAILURE_KEYWORDS: &[&str] = &[
    "test failed",
    "tests failed",
    "failed test",
    "failing test",
    "cargo test",
    "pytest",
    "jest",
    "vitest",
    "测试失败",
    "失败的测试",
];

const RUST_COMPLEXITY_KEYWORDS: &[&str] = &[
    "borrow",
    "lifetime",
    "trait bound",
    "type inference",
    "rust",
    "生命周期",
    "借用",
    "类型",
];

const SIMPLE_READ_ONLY_KEYWORDS: &[&str] = &[
    "explain",
    "summarize",
    "summary",
    "log",
    "logs",
    "why",
    "what does",
    "describe",
    "解释",
    "总结",
    "日志",
    "说明",
];

const EDIT_KEYWORDS: &[&str] = &[
    "fix",
    "implement",
    "change",
    "edit",
    "patch",
    "update",
    "refactor",
    "修复",
    "实现",
    "修改",
    "更新",
    "改造",
];

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_vibing_protocol::user_input::UserInput;

    fn request(text: &str) -> Vec<TurnInput> {
        vec![TurnInput::UserInput {
            content: vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
            client_id: None,
        }]
    }

    fn route(text: &str) -> Option<DeepSeekRouteDecision> {
        route_deepseek_turn(DeepSeekRouteRequest {
            provider_id: DEEPSEEK_PROVIDER_ID,
            current_model: DEEPSEEK_DEFAULT_MODEL_ID,
            current_reasoning_effort: None,
            explicit_model: false,
            explicit_reasoning_effort: false,
            config: &DeepSeekConfig::default(),
            input: &request(text),
        })
    }

    #[test]
    fn router_uses_flash_low_for_simple_explanations() {
        let decision = route("explain this log output").expect("route");

        assert_eq!(decision.model, DEEPSEEK_DEFAULT_MODEL_ID);
        assert_eq!(decision.reasoning_effort, Some(ReasoningEffort::Low));
        assert_eq!(decision.reason, "simple_read_only_task");
        assert!(!decision.fim_reserved);
    }

    #[test]
    fn router_uses_pro_high_for_multi_file_tasks() {
        let decision =
            route("fix src/lib.rs and src/main.rs after cargo test failed").expect("route");

        assert_eq!(decision.model, DEEPSEEK_PRO_MODEL_ID);
        assert_eq!(decision.reasoning_effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn router_preserves_explicit_model_and_routes_effort() {
        let input = request("fix unsafe sandbox permissions");
        let decision = route_deepseek_turn(DeepSeekRouteRequest {
            provider_id: DEEPSEEK_PROVIDER_ID,
            current_model: DEEPSEEK_DEFAULT_MODEL_ID,
            current_reasoning_effort: None,
            explicit_model: true,
            explicit_reasoning_effort: false,
            config: &DeepSeekConfig::default(),
            input: &input,
        })
        .expect("route");

        assert_eq!(decision.model, DEEPSEEK_DEFAULT_MODEL_ID);
        assert_eq!(decision.reasoning_effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn router_preserves_explicit_reasoning_effort_and_routes_model() {
        let input = request("refactor src/a.rs and src/b.rs");
        let decision = route_deepseek_turn(DeepSeekRouteRequest {
            provider_id: DEEPSEEK_BETA_PROVIDER_ID,
            current_model: DEEPSEEK_DEFAULT_MODEL_ID,
            current_reasoning_effort: Some(ReasoningEffort::Low),
            explicit_model: false,
            explicit_reasoning_effort: true,
            config: &DeepSeekConfig::default(),
            input: &input,
        })
        .expect("route");

        assert_eq!(decision.model, DEEPSEEK_PRO_MODEL_ID);
        assert_eq!(decision.reasoning_effort, Some(ReasoningEffort::Low));
    }

    #[test]
    fn router_skips_non_deepseek_and_disabled_config() {
        let input = request("fix unsafe sandbox permissions");
        assert!(
            route_deepseek_turn(DeepSeekRouteRequest {
                provider_id: "openai",
                current_model: "gpt-5.4",
                current_reasoning_effort: None,
                explicit_model: false,
                explicit_reasoning_effort: false,
                config: &DeepSeekConfig::default(),
                input: &input,
            })
            .is_none()
        );

        let mut config = DeepSeekConfig::default();
        config.router.enabled = false;
        assert!(
            route_deepseek_turn(DeepSeekRouteRequest {
                provider_id: DEEPSEEK_PROVIDER_ID,
                current_model: DEEPSEEK_DEFAULT_MODEL_ID,
                current_reasoning_effort: None,
                explicit_model: false,
                explicit_reasoning_effort: false,
                config: &config,
                input: &input,
            })
            .is_none()
        );
    }
}
