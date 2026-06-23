use std::collections::HashMap;
use std::sync::Arc;

use crate::SkillsManager;
use crate::agent::AgentControl;
use crate::attestation::AttestationProvider;
use crate::client::ModelClient;
use crate::config::NetworkProxyAuditMetadata;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;
use crate::guardian::GuardianRejection;
use crate::guardian::GuardianRejectionCircuitBreaker;
use crate::mcp::McpManager;
use crate::tools::network_approval::NetworkApprovalService;
use crate::tools::sandboxing::ApprovalStore;
use crate::trajectory::TrajectoryRecorder;
use crate::unified_exec::UnifiedExecProcessManager;
use arc_swap::ArcSwap;
use arc_swap::ArcSwapOption;
use deepseek_vibing_analytics::AnalyticsEventsClient;
use deepseek_vibing_auth::AuthManager;
use deepseek_vibing_core_plugins::PluginsManager;
use deepseek_vibing_exec_server::EnvironmentManager;
use deepseek_vibing_extension_api::ExtensionData;
use deepseek_vibing_extension_api::ExtensionRegistry;
use deepseek_vibing_hooks::Hooks;
use deepseek_vibing_mcp::McpConnectionManager;
use deepseek_vibing_models_manager::manager::SharedModelsManager;
use deepseek_vibing_otel::SessionTelemetry;
use deepseek_vibing_rollout::state_db::StateDbHandle;
use deepseek_vibing_rollout_trace::ThreadTraceContext;
use deepseek_vibing_thread_store::LiveThread;
use deepseek_vibing_thread_store::ThreadStore;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub(crate) struct SessionServices {
    pub(crate) mcp_connection_manager: Arc<RwLock<McpConnectionManager>>,
    pub(crate) mcp_startup_cancellation_token: Mutex<CancellationToken>,
    pub(crate) unified_exec_manager: UnifiedExecProcessManager,
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(crate) shell_zsh_path: Option<PathBuf>,
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(crate) main_execve_wrapper_exe: Option<PathBuf>,
    pub(crate) analytics_events_client: AnalyticsEventsClient,
    pub(crate) hooks: ArcSwap<Hooks>,
    pub(crate) rollout_thread_trace: ThreadTraceContext,
    pub(crate) trajectory: TrajectoryRecorder,
    pub(crate) user_shell: Arc<crate::shell::Shell>,
    pub(crate) shell_snapshot_tx: watch::Sender<Option<Arc<crate::shell_snapshot::ShellSnapshot>>>,
    pub(crate) show_raw_agent_reasoning: bool,
    pub(crate) exec_policy: Arc<ExecPolicyManager>,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) models_manager: SharedModelsManager,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) tool_approvals: Mutex<ApprovalStore>,
    pub(crate) guardian_rejections: Mutex<HashMap<String, GuardianRejection>>,
    pub(crate) guardian_rejection_circuit_breaker: Mutex<GuardianRejectionCircuitBreaker>,
    pub(crate) runtime_handle: Handle,
    pub(crate) skills_manager: Arc<SkillsManager>,
    pub(crate) plugins_manager: Arc<PluginsManager>,
    pub(crate) mcp_manager: Arc<McpManager>,
    pub(crate) extensions: Arc<ExtensionRegistry<crate::config::Config>>,
    pub(crate) session_extension_data: ExtensionData,
    pub(crate) thread_extension_data: ExtensionData,
    pub(crate) agent_control: AgentControl,
    pub(crate) network_proxy: ArcSwapOption<StartedNetworkProxy>,
    pub(crate) network_proxy_audit_metadata: NetworkProxyAuditMetadata,
    pub(crate) managed_network_requirements_configured: bool,
    pub(crate) network_approval: Arc<NetworkApprovalService>,
    pub(crate) state_db: Option<StateDbHandle>,
    pub(crate) live_thread: Option<LiveThread>,
    pub(crate) thread_store: Arc<dyn ThreadStore>,
    pub(crate) attestation_provider: Option<Arc<dyn AttestationProvider>>,
    /// Session-scoped model client shared across turns.
    pub(crate) model_client: ModelClient,
    /// Shared process-level environment registry. Sessions carry an `Arc` handle so they can pass
    /// the same manager through child-thread spawn paths without reconstructing it.
    pub(crate) environment_manager: Arc<EnvironmentManager>,
}
