use std::sync::Arc;
use std::sync::Weak;

use async_trait::async_trait;
use deepseek_vibing_core::ThreadManager;
use deepseek_vibing_extension_api::ConfigContributor;
use deepseek_vibing_extension_api::ExtensionData;
use deepseek_vibing_extension_api::ExtensionEventSink;
use deepseek_vibing_extension_api::ExtensionRegistryBuilder;
use deepseek_vibing_extension_api::ThreadIdleInput;
use deepseek_vibing_extension_api::ThreadLifecycleContributor;
use deepseek_vibing_extension_api::ThreadResumeInput;
use deepseek_vibing_extension_api::ThreadStartInput;
use deepseek_vibing_extension_api::ThreadStopInput;
use deepseek_vibing_extension_api::TokenUsageContributor;
use deepseek_vibing_extension_api::ToolCallOutcome;
use deepseek_vibing_extension_api::ToolContributor;
use deepseek_vibing_extension_api::ToolFinishInput;
use deepseek_vibing_extension_api::ToolLifecycleContributor;
use deepseek_vibing_extension_api::ToolLifecycleFuture;
use deepseek_vibing_extension_api::TurnAbortInput;
use deepseek_vibing_extension_api::TurnErrorInput;
use deepseek_vibing_extension_api::TurnLifecycleContributor;
use deepseek_vibing_extension_api::TurnStartInput;
use deepseek_vibing_extension_api::TurnStopInput;
use deepseek_vibing_otel::MetricsClient;
use deepseek_vibing_protocol::ThreadId;
use deepseek_vibing_protocol::protocol::DeepSeekVibingErrorInfo;
use deepseek_vibing_protocol::protocol::SessionSource;
use deepseek_vibing_protocol::protocol::SubAgentSource;
use deepseek_vibing_protocol::protocol::ThreadGoalStatus;
use deepseek_vibing_protocol::protocol::TokenUsageInfo;

use crate::accounting::BudgetLimitedGoalDisposition;
use crate::accounting::GoalAccountingState;
use crate::api::GoalService;
use crate::events::GoalEventEmitter;
use crate::metrics::GoalMetrics;
use crate::runtime::GoalRuntimeConfig;
use crate::runtime::GoalRuntimeHandle;
use crate::spec::UPDATE_GOAL_TOOL_NAME;
use crate::steering::budget_limit_steering_item;
use crate::tool::GoalToolExecutor;

#[derive(Clone, Debug)]
pub struct GoalExtensionConfig {
    pub enabled: bool,
}

impl GoalExtensionConfig {
    fn from_enabled(enabled: bool) -> Self {
        Self { enabled }
    }
}

#[derive(Clone)]
pub struct GoalExtension<C> {
    state_dbs: Arc<deepseek_vibing_state::StateRuntime>,
    event_emitter: GoalEventEmitter,
    metrics: GoalMetrics,
    thread_manager: Weak<ThreadManager>,
    goal_service: Arc<GoalService>,
    goals_enabled: Arc<dyn Fn(&C) -> bool + Send + Sync>,
}

impl<C> std::fmt::Debug for GoalExtension<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoalExtension").finish_non_exhaustive()
    }
}

impl<C> GoalExtension<C> {
    pub(crate) fn new_with_host_capabilities(
        state_dbs: Arc<deepseek_vibing_state::StateRuntime>,
        event_sink: Arc<dyn ExtensionEventSink>,
        metrics_client: Option<MetricsClient>,
        thread_manager: Weak<ThreadManager>,
        goal_service: Arc<GoalService>,
        goals_enabled: impl Fn(&C) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            state_dbs,
            event_emitter: GoalEventEmitter::new(event_sink),
            metrics: GoalMetrics::new(metrics_client),
            thread_manager,
            goal_service,
            goals_enabled: Arc::new(goals_enabled),
        }
    }
}

#[async_trait]
impl<C> ThreadLifecycleContributor<C> for GoalExtension<C>
where
    C: Send + Sync + 'static,
{
    async fn on_thread_start(&self, input: ThreadStartInput<'_, C>) {
        let enabled = (self.goals_enabled)(input.config);
        let tools_available_for_thread = input.persistent_thread_state_available
            && !matches!(
                input.session_source,
                SessionSource::SubAgent(SubAgentSource::Review)
            );
        input
            .thread_store
            .insert(GoalExtensionConfig::from_enabled(enabled));
        let accounting_state = input
            .thread_store
            .get_or_init::<GoalAccountingState>(GoalAccountingState::default);
        let Ok(thread_id) = ThreadId::from_string(input.thread_store.level_id()) else {
            return;
        };
        let runtime = input.thread_store.get_or_init::<GoalRuntimeHandle>(|| {
            GoalRuntimeHandle::new(
                thread_id,
                Arc::clone(&self.state_dbs),
                self.event_emitter.clone(),
                self.metrics.clone(),
                self.thread_manager.clone(),
                accounting_state,
                GoalRuntimeConfig {
                    enabled,
                    tools_available_for_thread,
                },
            )
        });
        runtime.set_enabled(enabled);
        self.goal_service.register_runtime(&runtime);
    }

    async fn on_thread_resume(&self, input: ThreadResumeInput<'_>) {
        let Some(runtime) = goal_runtime_handle(input.thread_store) else {
            return;
        };

        if let Err(err) = runtime.restore_after_resume().await {
            tracing::warn!(
                "failed to restore goal runtime after thread resume for {}: {err}",
                runtime.thread_id()
            );
        }
    }

    async fn on_thread_idle(&self, input: ThreadIdleInput<'_>) {
        let Some(runtime) = goal_runtime_handle(input.thread_store) else {
            return;
        };

        if let Err(err) = runtime.continue_if_idle().await {
            tracing::warn!(
                "failed to continue active goal for idle thread {}: {err}",
                runtime.thread_id()
            );
        }
    }

    async fn on_thread_stop(&self, input: ThreadStopInput<'_>) {
        if let Some(runtime) = goal_runtime_handle(input.thread_store) {
            self.goal_service.unregister_runtime(&runtime);
        }
    }
}

impl<C> ConfigContributor<C> for GoalExtension<C>
where
    C: Send + Sync + 'static,
{
    fn on_config_changed(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
        _previous_config: &C,
        new_config: &C,
    ) {
        let enabled = (self.goals_enabled)(new_config);
        thread_store.insert(GoalExtensionConfig::from_enabled(enabled));
        if let Some(runtime) = goal_runtime_handle(thread_store) {
            runtime.set_enabled(enabled);
        }
    }
}

#[async_trait]
impl<C> TurnLifecycleContributor for GoalExtension<C>
where
    C: Send + Sync + 'static,
{
    async fn on_turn_start(&self, input: TurnStartInput<'_>) {
        let Some(runtime) = goal_runtime_handle(input.thread_store) else {
            return;
        };
        if !runtime.is_enabled() {
            return;
        }

        let accounting = runtime.accounting_state();
        accounting.start_turn(
            input.turn_id,
            input.collaboration_mode.mode,
            input.token_usage_at_turn_start,
        );
        if matches!(
            input.collaboration_mode.mode,
            deepseek_vibing_protocol::config_types::ModeKind::Plan
        ) {
            accounting.clear_current_turn_goal();
            return;
        }
        let Ok(goal) = self
            .state_dbs
            .thread_goals()
            .get_thread_goal(runtime.thread_id())
            .await
        else {
            return;
        };
        if let Some(goal) = goal
            && matches!(
                goal.status,
                deepseek_vibing_state::ThreadGoalStatus::Active
                    | deepseek_vibing_state::ThreadGoalStatus::BudgetLimited
            )
        {
            accounting.mark_turn_goal_active(input.turn_id, goal.goal_id);
        }
    }

    async fn on_turn_stop(&self, input: TurnStopInput<'_>) {
        let Some(runtime) = goal_runtime_handle(input.thread_store) else {
            return;
        };
        if !runtime.is_enabled() {
            return;
        }

        let turn_id = input.turn_store.level_id();
        if let Err(err) = runtime
            .account_active_goal_progress(
                turn_id,
                &format!("{turn_id}:turn-stop"),
                deepseek_vibing_state::GoalAccountingMode::ActiveOnly,
                BudgetLimitedGoalDisposition::ClearActive,
            )
            .await
        {
            tracing::warn!(
                "failed to account active goal progress at turn stop for {turn_id}: {err}"
            );
            return;
        }
        runtime.accounting_state().finish_turn(turn_id);
    }

    async fn on_turn_abort(&self, input: TurnAbortInput<'_>) {
        let Some(runtime) = goal_runtime_handle(input.thread_store) else {
            return;
        };
        if !runtime.is_enabled() {
            return;
        }

        let turn_id = input.turn_store.level_id();
        if let Err(err) = runtime
            .account_active_goal_progress(
                turn_id,
                &format!("{turn_id}:turn-abort"),
                deepseek_vibing_state::GoalAccountingMode::ActiveOnly,
                BudgetLimitedGoalDisposition::ClearActive,
            )
            .await
        {
            tracing::warn!(
                "failed to account active goal progress after turn abort for {turn_id}: {err}"
            );
            return;
        }
        runtime.accounting_state().finish_turn(turn_id);
    }

    async fn on_turn_error(&self, input: TurnErrorInput<'_>) {
        if input.error != DeepSeekVibingErrorInfo::UsageLimitExceeded {
            return;
        }
        let Some(runtime) = goal_runtime_handle(input.thread_store) else {
            return;
        };

        if let Err(err) = runtime
            .usage_limit_active_goal_for_turn(input.turn_id)
            .await
        {
            tracing::warn!("failed to usage-limit active goal after usage-limit error: {err}");
        }
    }
}

#[async_trait]
impl<C> TokenUsageContributor for GoalExtension<C>
where
    C: Send + Sync + 'static,
{
    async fn on_token_usage(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
        turn_store: &ExtensionData,
        token_usage: &TokenUsageInfo,
    ) {
        let Some(runtime) = goal_runtime_handle(thread_store) else {
            return;
        };
        if !runtime.is_enabled() {
            return;
        }

        let Some(_recorded) = runtime
            .accounting_state()
            .record_token_usage(turn_store.level_id(), &token_usage.total_token_usage)
        else {
            return;
        };
    }
}

impl<C> ToolLifecycleContributor for GoalExtension<C>
where
    C: Send + Sync + 'static,
{
    fn on_tool_finish<'a>(&'a self, input: ToolFinishInput<'a>) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            let Some(runtime) = goal_runtime_handle(input.thread_store) else {
                return;
            };
            let should_count_for_goal_progress = runtime.is_enabled()
                && tool_attempt_counts_for_goal_progress(input.outcome)
                && !(input.tool_name.namespace.is_none()
                    && input.tool_name.name == UPDATE_GOAL_TOOL_NAME);
            if !should_count_for_goal_progress {
                return;
            }
            let turn_id = input.turn_id;
            let progress = match runtime
                .account_active_goal_progress(
                    turn_id,
                    input.call_id,
                    deepseek_vibing_state::GoalAccountingMode::ActiveOnly,
                    BudgetLimitedGoalDisposition::KeepActive,
                )
                .await
            {
                Ok(Some(progress)) => progress,
                Ok(None) => return,
                Err(err) => {
                    tracing::warn!(
                        "failed to account active goal progress after tool finish for {turn_id}: {err}"
                    );
                    return;
                }
            };
            let goal = progress.goal;
            if goal.status != ThreadGoalStatus::BudgetLimited {
                return;
            }
            if !runtime
                .accounting_state()
                .mark_budget_limit_reported_if_new(progress.goal_id.as_str())
            {
                return;
            }
            let item = budget_limit_steering_item(&goal);
            runtime.inject_active_turn_steering(item).await;
        })
    }
}

impl<C> ToolContributor for GoalExtension<C>
where
    C: Send + Sync + 'static,
{
    fn tools(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
    ) -> Vec<
        Arc<
            dyn deepseek_vibing_extension_api::ToolExecutor<deepseek_vibing_extension_api::ToolCall>,
        >,
    > {
        let Some(runtime) = goal_runtime_handle(thread_store) else {
            return Vec::new();
        };
        if !runtime.tools_visible() {
            return Vec::new();
        }

        vec![
            Arc::new(GoalToolExecutor::get(
                runtime.thread_id(),
                Arc::clone(&self.state_dbs),
                runtime.accounting_state(),
                self.event_emitter.clone(),
                self.metrics.clone(),
            )),
            Arc::new(GoalToolExecutor::create(
                runtime.thread_id(),
                Arc::clone(&self.state_dbs),
                runtime.accounting_state(),
                self.event_emitter.clone(),
                self.metrics.clone(),
            )),
            Arc::new(GoalToolExecutor::update(
                runtime.thread_id(),
                Arc::clone(&self.state_dbs),
                runtime.accounting_state(),
                self.event_emitter.clone(),
                self.metrics.clone(),
            )),
        ]
    }
}

pub fn install_with_backend<C>(
    registry: &mut ExtensionRegistryBuilder<C>,
    state_dbs: Arc<deepseek_vibing_state::StateRuntime>,
    metrics_client: Option<MetricsClient>,
    thread_manager: Weak<ThreadManager>,
    goal_service: Arc<GoalService>,
    goals_enabled: impl Fn(&C) -> bool + Send + Sync + 'static,
) where
    C: Send + Sync + 'static,
{
    let extension = Arc::new(GoalExtension::new_with_host_capabilities(
        state_dbs,
        registry.event_sink(),
        metrics_client,
        thread_manager,
        Arc::clone(&goal_service),
        goals_enabled,
    ));
    registry.thread_lifecycle_contributor(extension.clone());
    registry.config_contributor(extension.clone());
    registry.turn_lifecycle_contributor(extension.clone());
    registry.token_usage_contributor(extension.clone());
    registry.tool_lifecycle_contributor(extension.clone());
    registry.tool_contributor(extension);
}

fn goal_runtime_handle(thread_store: &ExtensionData) -> Option<Arc<GoalRuntimeHandle>> {
    thread_store.get::<GoalRuntimeHandle>()
}

fn tool_attempt_counts_for_goal_progress(outcome: ToolCallOutcome) -> bool {
    match outcome {
        ToolCallOutcome::Completed { .. } => true,
        ToolCallOutcome::Failed {
            handler_executed: true,
        } => true,
        ToolCallOutcome::Blocked
        | ToolCallOutcome::Failed {
            handler_executed: false,
        }
        | ToolCallOutcome::Aborted => false,
    }
}
