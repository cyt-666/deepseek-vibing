use clap::Args;
use clap::CommandFactory;
use clap::Parser;
use clap_complete::Shell;
use clap_complete::generate;
use deepseek_vibing_app_server_daemon::BootstrapOptions as AppServerBootstrapOptions;
use deepseek_vibing_app_server_daemon::LifecycleCommand as AppServerLifecycleCommand;
use deepseek_vibing_app_server_daemon::RemoteControlMode as AppServerRemoteControlMode;
use deepseek_vibing_arg0::Arg0DispatchPaths;
use deepseek_vibing_arg0::arg0_dispatch_or_else;
use deepseek_vibing_exec::Cli as ExecCli;
use deepseek_vibing_exec::Command as ExecCommand;
use deepseek_vibing_exec::ReviewArgs;
use deepseek_vibing_execpolicy::ExecPolicyCheckCommand;
use deepseek_vibing_rollout_trace::REDUCED_STATE_FILE_NAME;
use deepseek_vibing_rollout_trace::replay_bundle;
use deepseek_vibing_state::StateRuntime;
use deepseek_vibing_state::memories_db_path;
use deepseek_vibing_tui::AppExitInfo;
use deepseek_vibing_tui::Cli as TuiCli;
use deepseek_vibing_tui::ExitReason;
use deepseek_vibing_tui::UpdateAction;
use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;
use deepseek_vibing_utils_cli::CliConfigOverrides;
use deepseek_vibing_utils_cli::ProfileV2Name;
use deepseek_vibing_utils_cli::SharedCliOptions;
use deepseek_vibing_utils_cli::resume_hint;
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use supports_color::Stream;
use tokio::process::Command as TokioCommand;
use tokio::time::Duration;
use tokio::time::Instant;

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[allow(dead_code)]
mod app_cmd;
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[allow(dead_code)]
mod desktop_app;
mod doctor;
mod marketplace_cmd;
mod mcp_cmd;
mod plugin_cmd;
mod remote_control_cmd;
#[cfg(target_os = "windows")]
mod sandbox_setup;
mod state_db_recovery;
#[cfg(not(windows))]
mod wsl_paths;

use crate::mcp_cmd::McpCli;
use crate::plugin_cmd::PluginCli;
use crate::plugin_cmd::PluginSubcommand;
use crate::remote_control_cmd::RemoteControlCommand;
use doctor::DoctorCommand;
use state_db_recovery as local_state_db;

use deepseek_vibing_auth::AuthManager;
use deepseek_vibing_auth::DeepSeekVibingAuth;
use deepseek_vibing_config::LoaderOverrides;
use deepseek_vibing_core::build_models_manager;
use deepseek_vibing_core::config::Config;
use deepseek_vibing_core::config::ConfigBuilder;
use deepseek_vibing_core::config::ConfigOverrides;
use deepseek_vibing_core::config::DEEPSEEK_FIM_MAX_TOKENS_LIMIT;
use deepseek_vibing_core::config::EvalMode;
use deepseek_vibing_core::config::edit::ConfigEditsBuilder;
use deepseek_vibing_core::config::find_deepseek_vibing_home;
use deepseek_vibing_core::config::resolve_profile_v2_config_path;
use deepseek_vibing_core::deepseek_fim::FimCompletionOutput;
use deepseek_vibing_core::deepseek_fim::FimCompletionRequest;
use deepseek_vibing_core::deepseek_fim::complete_fim_with_auth;
use deepseek_vibing_core::eval::EvalCommandOutput;
use deepseek_vibing_core::eval::EvalReportInput;
use deepseek_vibing_core::eval::EvalRunReport;
use deepseek_vibing_core::eval::EvalTask;
use deepseek_vibing_core::eval::EvalTaskResult;
use deepseek_vibing_core::eval::EvalTaskStatus;
use deepseek_vibing_core::eval::build_eval_report;
use deepseek_vibing_core::eval::default_eval_work_dir;
use deepseek_vibing_core::eval::load_eval_tasks;
use deepseek_vibing_core::eval::materialize_task_workspace;
use deepseek_vibing_core::trajectory::TrajectoryReplayReport;
use deepseek_vibing_core::trajectory::TrajectorySummary;
use deepseek_vibing_core::trajectory::default_trajectory_root;
use deepseek_vibing_core::trajectory::list_trajectory_files;
use deepseek_vibing_core::trajectory::load_trajectory_summary;
use deepseek_vibing_core::trajectory::replay_trajectory;
use deepseek_vibing_core::trajectory::resolve_trajectory_path;
use deepseek_vibing_features::FEATURES;
use deepseek_vibing_features::Stage;
use deepseek_vibing_features::is_known_feature_key;
use deepseek_vibing_git_utils::ApplyGitRequest;
use deepseek_vibing_git_utils::apply_git_patch;
use deepseek_vibing_memories_write::clear_memory_roots_contents;
use deepseek_vibing_models_manager::bundled_models_response;
use deepseek_vibing_models_manager::manager::RefreshStrategy;
use deepseek_vibing_protocol::protocol::AskForApproval;
use deepseek_vibing_protocol::user_input::UserInput;
use deepseek_vibing_terminal_detection::TerminalName;

/// DeepSeek Vibing CLI
///
/// If no subcommand is specified, options will be forwarded to the interactive CLI.
#[derive(Debug, Parser)]
#[clap(
    author,
    name = "deepseek-vibing",
    version,
    // If a sub‑command is given, ignore requirements of the default args.
    subcommand_negates_reqs = true,
    // The executable is sometimes invoked via a platform-specific name, but the help output
    // should always use the generic command name that users run.
    bin_name = "deepseek-vibing",
    override_usage = "deepseek-vibing [OPTIONS] [PROMPT]\n       deepseek-vibing [OPTIONS] <COMMAND> [ARGS]"
)]
struct MultitoolCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    pub feature_toggles: FeatureToggles,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    interactive: TuiCli,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// Run DeepSeek Vibing non-interactively.
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// Run a code review non-interactively.
    Review(ReviewCommand),

    /// Manage external MCP servers for DeepSeek Vibing.
    Mcp(McpCli),

    /// Manage DeepSeek Vibing plugins.
    Plugin(PluginCli),

    /// Start DeepSeek Vibing as an MCP server (stdio).
    McpServer(McpServerCommand),

    /// [experimental] Run the app server or related tooling.
    #[clap(hide = true)]
    AppServer(AppServerCommand),

    /// [experimental] Manage the app-server daemon with remote control enabled.
    #[clap(hide = true)]
    RemoteControl(RemoteControlCommand),

    /// Launch the DeepSeek Vibing desktop app (opens the app installer if missing).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[clap(hide = true)]
    App(app_cmd::AppCommand),

    /// Generate shell completion scripts.
    Completion(CompletionCommand),

    /// Fill in code between a prefix and optional suffix using DeepSeek FIM.
    Fim(FimCommand),

    /// Inspect DeepSeek Vibing trajectory summaries.
    Trajectory(TrajectoryCommand),

    /// Run local DeepSeek Vibing evals and quality gates.
    Eval(EvalCommand),

    /// Update DeepSeekVibing to the latest version.
    #[clap(hide = true)]
    Update,

    /// Diagnose local DeepSeek Vibing installation, config, auth, and runtime health.
    Doctor(DoctorCommand),

    /// Run commands within a DeepSeek Vibing-provided sandbox.
    Sandbox(HostSandboxArgs),

    /// Debugging tools.
    Debug(DebugCommand),

    /// Execpolicy tooling.
    #[clap(hide = true)]
    Execpolicy(ExecpolicyCommand),

    /// Apply a local unified diff patch to your working tree.
    #[clap(visible_alias = "a")]
    Apply(ApplyCommand),

    /// Resume a previous interactive session (picker by default; use --last to continue the most recent).
    Resume(ResumeCommand),

    /// Archive a saved session by id or session name.
    Archive(SessionArchiveCommand),

    /// Unarchive a saved session by id or session name.
    Unarchive(SessionArchiveCommand),

    /// Fork a previous interactive session (picker by default; use --last to fork the most recent).
    Fork(ForkCommand),

    /// Internal: relay stdio to a Unix domain socket.
    #[clap(hide = true, name = "stdio-to-uds")]
    StdioToUds(StdioToUdsCommand),

    /// [EXPERIMENTAL] Run the standalone exec-server service.
    #[clap(hide = true)]
    ExecServer(ExecServerCommand),

    /// Inspect feature flags.
    Features(FeaturesCli),
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// Shell to generate completions for
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Debug, Parser)]
struct FimCommand {
    /// File containing the text before the cursor. Use '-' to read from stdin.
    #[arg(long = "prefix-file", value_name = "FILE")]
    prefix_file: PathBuf,

    /// File containing the text after the cursor. Use '-' to read from stdin.
    #[arg(long = "suffix-file", value_name = "FILE")]
    suffix_file: Option<PathBuf>,

    /// Override the DeepSeek FIM model.
    #[arg(long = "model", value_name = "MODEL")]
    model: Option<String>,

    /// Override the DeepSeek beta base URL.
    #[arg(long = "base-url", value_name = "URL")]
    base_url: Option<String>,

    /// Override the maximum completion tokens.
    #[arg(long = "max-tokens", value_parser = parse_fim_max_tokens)]
    max_tokens: Option<u16>,

    /// Override the sampling temperature.
    #[arg(long = "temperature")]
    temperature: Option<f32>,

    /// Print structured JSON instead of only the completion text.
    #[arg(long = "json", default_value_t = false)]
    json: bool,

    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
struct TrajectoryCommand {
    #[command(subcommand)]
    subcommand: TrajectorySubcommand,

    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, clap::Subcommand)]
enum TrajectorySubcommand {
    /// List recorded trajectory summary files.
    List,

    /// Show a trajectory summary.
    Show(TrajectoryShowCommand),

    /// Generate an offline replay report from a trajectory summary.
    Replay(TrajectoryReplayCommand),
}

#[derive(Debug, Args)]
struct TrajectoryShowCommand {
    /// Trajectory id fragment or path to a trajectory JSONL file.
    #[arg(value_name = "TRAJECTORY_ID_OR_PATH")]
    selector: String,

    /// Print structured JSON instead of a human-readable summary.
    #[arg(long = "json", default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct TrajectoryReplayCommand {
    /// Trajectory id fragment or path to a trajectory JSONL file.
    #[arg(value_name = "TRAJECTORY_ID_OR_PATH")]
    selector: String,

    /// Write the structured replay report JSON to this file.
    #[arg(long = "output", short = 'o', value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct EvalCommand {
    #[command(subcommand)]
    subcommand: EvalSubcommand,

    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, clap::Subcommand)]
enum EvalSubcommand {
    /// List tasks in an eval dataset.
    List(EvalListCommand),

    /// Run eval tasks and generate a quality report.
    Run(EvalRunCommand),

    /// Summarize existing trajectory files into an eval report.
    SummarizeTrajectories(EvalSummarizeTrajectoriesCommand),
}

#[derive(Debug, Args)]
struct EvalListCommand {
    /// Dataset directory or a single task TOML file.
    #[arg(long = "dataset", value_name = "DIR")]
    dataset: Option<PathBuf>,

    /// Print structured JSON.
    #[arg(long = "json", default_value_t = false)]
    json: bool,
}

#[derive(Debug, Args)]
struct EvalRunCommand {
    /// Dataset directory or a single task TOML file.
    #[arg(long = "dataset", value_name = "DIR")]
    dataset: Option<PathBuf>,

    /// Run only one task id.
    #[arg(long = "task", value_name = "ID")]
    task: Option<String>,

    /// Eval mode.
    #[arg(long = "mode", value_enum)]
    mode: Option<EvalModeArg>,

    /// Report output path. Defaults to the eval reports directory.
    #[arg(long = "report", value_name = "FILE")]
    report: Option<PathBuf>,

    /// Baseline report used for regression gate checks.
    #[arg(long = "baseline", value_name = "FILE")]
    baseline: Option<PathBuf>,

    /// Return non-zero when quality gates fail.
    #[arg(long = "enforce", default_value_t = false)]
    enforce: bool,

    /// Print structured JSON.
    #[arg(long = "json", default_value_t = false)]
    json: bool,

    /// Keep per-task workspaces after the run.
    #[arg(long = "keep-workspaces", default_value_t = false)]
    keep_workspaces: bool,
}

#[derive(Debug, Args)]
struct EvalSummarizeTrajectoriesCommand {
    /// Trajectory root to summarize. Defaults to the configured trajectory root.
    #[arg(long = "trajectory-root", value_name = "DIR")]
    trajectory_root: Option<PathBuf>,

    /// Report output path. Defaults to the eval reports directory.
    #[arg(long = "report", value_name = "FILE")]
    report: Option<PathBuf>,

    /// Print structured JSON.
    #[arg(long = "json", default_value_t = false)]
    json: bool,

    /// Return non-zero when quality gates fail.
    #[arg(long = "enforce", default_value_t = false)]
    enforce: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
enum EvalModeArg {
    Hybrid,
    Live,
    Offline,
}

impl From<EvalModeArg> for EvalMode {
    fn from(value: EvalModeArg) -> Self {
        match value {
            EvalModeArg::Hybrid => EvalMode::Hybrid,
            EvalModeArg::Live => EvalMode::Live,
            EvalModeArg::Offline => EvalMode::Offline,
        }
    }
}

#[derive(Debug, Parser)]
struct DebugCommand {
    #[command(subcommand)]
    subcommand: DebugSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugSubcommand {
    /// Render the raw model catalog as JSON.
    Models(DebugModelsCommand),

    /// Tooling: helps debug the app server.
    AppServer(DebugAppServerCommand),

    /// Render the model-visible prompt input list as JSON.
    PromptInput(DebugPromptInputCommand),

    /// Replay a rollout trace bundle and write reduced state JSON.
    #[clap(hide = true)]
    TraceReduce(DebugTraceReduceCommand),

    /// Internal: reset local memory state for a fresh start.
    #[clap(hide = true)]
    ClearMemories,
}

#[derive(Debug, Parser)]
struct DebugAppServerCommand {
    #[command(subcommand)]
    subcommand: DebugAppServerSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugAppServerSubcommand {
    // Send message to app server V2.
    SendMessageV2(DebugAppServerSendMessageV2Command),
}

#[derive(Debug, Parser)]
struct DebugAppServerSendMessageV2Command {
    #[arg(value_name = "USER_MESSAGE", required = true)]
    user_message: String,
}

#[derive(Debug, Parser)]
struct DebugPromptInputCommand {
    /// Optional user prompt to append after session context.
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    /// Optional image(s) to attach to the user prompt.
    #[arg(long = "image", short = 'i', value_name = "FILE", value_delimiter = ',', num_args = 1..)]
    images: Vec<PathBuf>,
}

#[derive(Debug, Parser)]
struct DebugModelsCommand {
    /// Skip refresh and dump only the bundled catalog shipped with this binary.
    #[arg(long = "bundled", default_value_t = false)]
    bundled: bool,
}

#[derive(Debug, Parser)]
struct ReviewCommand {
    /// Error out when config.toml contains fields that are not recognized by this version of DeepSeek Vibing.
    #[arg(long = "strict-config", default_value_t = false)]
    strict_config: bool,

    #[clap(flatten)]
    args: ReviewArgs,
}

#[derive(Debug, Parser)]
struct McpServerCommand {
    /// Error out when config.toml contains fields that are not recognized by this version of DeepSeek Vibing.
    #[arg(long = "strict-config", default_value_t = false)]
    strict_config: bool,
}

#[derive(Debug, Parser)]
struct DebugTraceReduceCommand {
    /// Trace bundle directory containing manifest.json and trace.jsonl.
    #[arg(value_name = "TRACE_BUNDLE")]
    trace_bundle: PathBuf,

    /// Output path for reduced RolloutTrace JSON. Defaults to TRACE_BUNDLE/state.json.
    #[arg(long = "output", short = 'o', value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct ResumeCommand {
    /// Session id (UUID) or session name. UUIDs take precedence if it parses.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// Continue the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false)]
    last: bool,

    /// Show all sessions (disables cwd filtering and shows CWD column).
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    /// Include non-interactive sessions in the resume picker and --last selection.
    #[arg(long = "include-non-interactive", default_value_t = false)]
    include_non_interactive: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct SessionArchiveCommand {
    /// Session id (UUID) or session name. UUIDs take precedence if it parses.
    #[arg(value_name = "SESSION")]
    target: String,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: SessionArchiveConfigOverrides,
}

#[derive(Debug, Args, Clone, Default)]
struct SessionArchiveConfigOverrides {
    #[clap(flatten)]
    shared: SharedCliOptions,

    /// Error out when config.toml contains fields that are not recognized by this version of DeepSeek Vibing.
    #[arg(long = "strict-config", default_value_t = false)]
    strict_config: bool,

    #[clap(flatten)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
struct ForkCommand {
    /// Conversation/session id (UUID). When provided, forks this session.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// Fork the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    last: bool,

    /// Show all sessions (disables cwd filtering and shows CWD column).
    #[arg(long = "all", default_value_t = false)]
    all: bool,

    #[clap(flatten)]
    remote: InteractiveRemoteOptions,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[cfg(target_os = "macos")]
type HostSandboxArgs = deepseek_vibing_cli::SeatbeltCommand;
#[cfg(target_os = "linux")]
type HostSandboxArgs = deepseek_vibing_cli::LandlockCommand;
#[cfg(target_os = "windows")]
type HostSandboxArgs = deepseek_vibing_cli::WindowsCommand;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
type HostSandboxArgs = UnsupportedSandboxArgs;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
#[derive(Debug, Parser)]
struct UnsupportedSandboxArgs {
    /// Layer $DEEPSEEK_VIBING_HOME/<name>.config.toml on top of the base user config.
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<ProfileV2Name>,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Full command args to run under the host sandbox.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Parser)]
struct ExecpolicyCommand {
    #[command(subcommand)]
    sub: ExecpolicySubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum ExecpolicySubcommand {
    /// Check execpolicy files against a command.
    #[clap(name = "check")]
    Check(ExecPolicyCheckCommand),
}

#[derive(Debug, Parser)]
struct ApplyCommand {
    /// Unified diff patch file to apply. Use '-' to read from stdin.
    #[arg(long = "patch-file", short = 'p', value_name = "FILE")]
    patch_file: PathBuf,

    /// Revert the patch instead of applying it.
    #[arg(long = "revert", default_value_t = false)]
    revert: bool,

    /// Check whether the patch applies without modifying files.
    #[arg(long = "check", default_value_t = false)]
    check: bool,

    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
struct AppServerCommand {
    /// Omit to run the app server; specify a subcommand for tooling.
    #[command(subcommand)]
    subcommand: Option<AppServerSubcommand>,

    /// Error out when config.toml contains fields that are not recognized by this version of DeepSeek Vibing.
    #[arg(long = "strict-config", default_value_t = false)]
    strict_config: bool,

    /// Transport endpoint URL. Supported values: `stdio://` (default),
    /// `unix://`, `unix://PATH`, `ws://IP:PORT`, `off`.
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = deepseek_vibing_app_server::AppServerTransport::DEFAULT_LISTEN_URL
    )]
    listen: deepseek_vibing_app_server::AppServerTransport,

    /// Use stdio as the transport (equivalent to `--listen stdio://`).
    #[arg(long = "stdio", conflicts_with = "listen")]
    stdio: bool,

    /// Enable remote control for this app-server process.
    #[arg(long = "remote-control", hide = true)]
    remote_control: bool,

    /// Controls whether analytics are enabled by default.
    ///
    /// Analytics are disabled by default for app-server. Users have to explicitly opt in
    /// via the `analytics` section in the config.toml file.
    ///
    /// However, for first-party use cases like the VSCode IDE extension, we default analytics
    /// to be enabled by default by setting this flag. Users can still opt out by setting this
    /// in their config.toml:
    ///
    /// ```toml
    /// [analytics]
    /// enabled = false
    /// ```
    ///
    /// See https://github.com/deepseek-vibing/deepseek-vibing/tree/main/docs for more details.
    #[arg(long = "analytics-default-enabled")]
    analytics_default_enabled: bool,

    #[command(flatten)]
    auth: deepseek_vibing_app_server::AppServerWebsocketAuthArgs,
}

#[derive(Debug, Parser)]
struct ExecServerCommand {
    /// Error out when config.toml contains fields that are not recognized by this version of DeepSeek Vibing.
    #[arg(long = "strict-config", default_value_t = false)]
    strict_config: bool,

    /// Transport endpoint URL. Supported values: `ws://IP:PORT` (default), `stdio`, `stdio://`.
    #[arg(long = "listen", value_name = "URL", conflicts_with = "remote")]
    listen: Option<String>,

    /// Register this exec-server as a remote environment using the given base URL.
    #[arg(long = "remote", value_name = "URL", requires = "environment_id")]
    remote: Option<String>,

    /// Environment id to attach to when registering remotely.
    #[arg(long = "environment-id", value_name = "ID")]
    environment_id: Option<String>,

    /// Human-readable environment name.
    #[arg(long = "name", value_name = "NAME")]
    name: Option<String>,

    /// Unsupported legacy Agent Identity auth mode.
    #[arg(long = "use-agent-identity-auth", requires = "remote")]
    use_agent_identity_auth: bool,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::enum_variant_names)]
enum AppServerSubcommand {
    /// Manage the local app-server daemon.
    Daemon(AppServerDaemonCommand),

    /// Proxy stdio bytes to the running app-server control socket.
    Proxy(AppServerProxyCommand),

    /// [experimental] Generate TypeScript bindings for the app server protocol.
    GenerateTs(GenerateTsCommand),

    /// [experimental] Generate JSON Schema for the app server protocol.
    GenerateJsonSchema(GenerateJsonSchemaCommand),

    /// [internal] Generate internal JSON Schema artifacts for DeepSeekVibing tooling.
    #[clap(hide = true)]
    GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand),
}

#[derive(Debug, Args)]
struct AppServerDaemonCommand {
    #[command(subcommand)]
    subcommand: AppServerDaemonSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum AppServerDaemonSubcommand {
    /// Install durable local app-server management for SSH-driven use.
    Bootstrap(AppServerBootstrapCommand),

    /// Start the local app server daemon if it is not already running.
    Start,

    /// Restart the local app server daemon.
    Restart,

    /// Enable remote control for future starts and a currently running managed daemon.
    EnableRemoteControl,

    /// Disable remote control for future starts and a currently running managed daemon.
    DisableRemoteControl,

    /// Stop the local app server daemon.
    Stop,

    /// Print local CLI and running app-server versions as JSON.
    Version,

    /// [internal] Run the detached pid-backed standalone updater loop.
    #[clap(hide = true)]
    PidUpdateLoop,
}

#[derive(Debug, Args)]
struct AppServerProxyCommand {
    /// Path to the app-server Unix domain socket to connect to.
    #[arg(long = "sock", value_name = "SOCKET_PATH", value_parser = parse_socket_path)]
    socket_path: Option<AbsolutePathBuf>,
}

#[derive(Debug, Args)]
struct AppServerBootstrapCommand {
    /// Launch the managed app-server with remote control enabled.
    #[arg(long = "remote-control")]
    remote_control: bool,
}

#[derive(Debug, Args)]
struct GenerateTsCommand {
    /// Output directory where .ts files will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// Optional path to the Prettier executable to format generated files
    #[arg(short = 'p', long = "prettier", value_name = "PRETTIER_BIN")]
    prettier: Option<PathBuf>,

    /// Include experimental methods and fields in the generated output
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateJsonSchemaCommand {
    /// Output directory where the schema bundle will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// Include experimental methods and fields in the generated output
    #[arg(long = "experimental", default_value_t = false)]
    experimental: bool,
}

#[derive(Debug, Args)]
struct GenerateInternalJsonSchemaCommand {
    /// Output directory where internal JSON Schema artifacts will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,
}

#[derive(Debug, Parser)]
struct StdioToUdsCommand {
    /// Path to the Unix domain socket to connect to.
    #[arg(value_name = "SOCKET_PATH", value_parser = parse_socket_path)]
    socket_path: AbsolutePathBuf,
}

fn parse_socket_path(raw: &str) -> Result<AbsolutePathBuf, String> {
    AbsolutePathBuf::relative_to_current_dir(raw)
        .map_err(|err| format!("failed to resolve socket path `{raw}`: {err}"))
}

fn format_exit_messages(exit_info: AppExitInfo, color_enabled: bool) -> Vec<String> {
    let AppExitInfo {
        token_usage,
        thread_id: conversation_id,
        thread_name,
        ..
    } = exit_info;

    let mut lines = Vec::new();
    if !token_usage.is_zero() {
        lines.push(token_usage.to_string());
    }

    if let Some(resume_cmd) = resume_hint(thread_name.as_deref(), conversation_id) {
        let command = if color_enabled {
            resume_cmd.cyan().to_string()
        } else {
            resume_cmd
        };
        lines.push(format!("To continue this session, run {command}"));
    }

    lines
}

/// Handle the app exit and print the results. Optionally run the update action.
fn handle_app_exit(exit_info: AppExitInfo) -> anyhow::Result<()> {
    match exit_info.exit_reason {
        ExitReason::Fatal(message) => {
            eprintln!("ERROR: {message}");
            std::process::exit(1);
        }
        ExitReason::UserRequested => { /* normal exit */ }
    }

    let update_action = exit_info.update_action;
    let color_enabled = supports_color::on(Stream::Stdout).is_some();
    for line in format_exit_messages(exit_info, color_enabled) {
        println!("{line}");
    }
    if let Some(action) = update_action {
        run_update_action(action)?;
    }
    Ok(())
}

/// Run the update action and print the result.
fn run_update_action(action: UpdateAction) -> anyhow::Result<()> {
    println!();
    let cmd_str = action.command_str();
    println!("Updating DeepSeekVibing via `{cmd_str}`...");

    let status = {
        #[cfg(windows)]
        {
            if action == UpdateAction::StandaloneWindows {
                let (cmd, args) = action.command_args();
                // Run the standalone PowerShell installer with PowerShell
                // itself. Routing this through `cmd.exe /C` would parse
                // PowerShell metacharacters like `|` before PowerShell sees
                // the installer command.
                std::process::Command::new(cmd).args(args).status()?
            } else {
                // On Windows, run via cmd.exe so .CMD/.BAT are correctly resolved (PATHEXT semantics).
                std::process::Command::new("cmd")
                    .args(["/C", &cmd_str])
                    .status()?
            }
        }
        #[cfg(not(windows))]
        {
            let (cmd, args) = action.command_args();
            let command_path = crate::wsl_paths::normalize_for_wsl(cmd);
            let normalized_args: Vec<String> = args
                .iter()
                .map(crate::wsl_paths::normalize_for_wsl)
                .collect();
            std::process::Command::new(&command_path)
                .args(&normalized_args)
                .status()?
        }
    };
    if !status.success() {
        anyhow::bail!("`{cmd_str}` failed with status {status}");
    }
    println!("\n🎉 Update ran successfully! Please restart DeepSeekVibing.");
    Ok(())
}

#[allow(dead_code)]
fn run_update_command() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    {
        anyhow::bail!(
            "`deepseek-vibing update` is not available in debug builds. Install a release build of DeepSeekVibing to use this command."
        );
    }

    #[cfg(not(debug_assertions))]
    {
        let Some(action) = deepseek_vibing_tui::get_update_action() else {
            anyhow::bail!(
                "Could not detect the DeepSeekVibing installation method. Please update manually: https://github.com/deepseek-vibing/deepseek-vibing/tree/main/docs"
            );
        };
        run_update_action(action)
    }
}

fn run_execpolicycheck(cmd: ExecPolicyCheckCommand) -> anyhow::Result<()> {
    cmd.run()
}

async fn run_session_archive_cli_command(
    action: deepseek_vibing_tui::SessionArchiveAction,
    cmd: SessionArchiveCommand,
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    root_remote: Option<String>,
    root_remote_auth_token_env: Option<String>,
    arg0_paths: Arg0DispatchPaths,
) -> anyhow::Result<String> {
    let SessionArchiveCommand {
        target,
        remote,
        config_overrides,
    } = cmd;
    interactive =
        finalize_session_archive_interactive(interactive, root_config_overrides, config_overrides);
    let explicit_remote_endpoint = resolve_remote_endpoint(
        remote.remote.or(root_remote),
        remote.remote_auth_token_env.or(root_remote_auth_token_env),
    )?;
    deepseek_vibing_tui::run_session_archive_command(
        action,
        target,
        deepseek_vibing_tui::SessionArchiveCommandOptions {
            cli: interactive,
            arg0_paths,
            explicit_remote_endpoint,
        },
    )
    .await
    .map_err(|err| anyhow::anyhow!("{err}"))
}

async fn run_debug_app_server_command(cmd: DebugAppServerCommand) -> anyhow::Result<()> {
    match cmd.subcommand {
        DebugAppServerSubcommand::SendMessageV2(cmd) => {
            let deepseek_vibing_bin = std::env::current_exe()?;
            deepseek_vibing_app_server_test_client::send_message_v2(
                &deepseek_vibing_bin,
                &[],
                cmd.user_message,
                &None,
            )
            .await
        }
    }
}

#[derive(Debug, Default, Parser, Clone)]
struct FeatureToggles {
    /// Enable a feature (repeatable). Equivalent to `-c features.<name>=true`.
    #[arg(long = "enable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    enable: Vec<String>,

    /// Disable a feature (repeatable). Equivalent to `-c features.<name>=false`.
    #[arg(long = "disable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    disable: Vec<String>,
}

#[derive(Debug, Default, Parser, Clone)]
struct InteractiveRemoteOptions {
    /// Connect the TUI to a remote app server endpoint.
    ///
    /// Accepted forms: `ws://host:port`, `wss://host:port`, `unix://`, or `unix://PATH`.
    #[arg(long = "remote", value_name = "ADDR", hide = true)]
    remote: Option<String>,

    /// Name of the environment variable containing the bearer token to send to
    /// a remote app server websocket.
    #[arg(long = "remote-auth-token-env", value_name = "ENV_VAR", hide = true)]
    remote_auth_token_env: Option<String>,
}

impl FeatureToggles {
    fn to_overrides(&self) -> anyhow::Result<Vec<String>> {
        let mut v = Vec::new();
        for feature in &self.enable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=true"));
        }
        for feature in &self.disable {
            Self::validate_feature(feature)?;
            v.push(format!("features.{feature}=false"));
        }
        Ok(v)
    }

    fn validate_feature(feature: &str) -> anyhow::Result<()> {
        if is_known_feature_key(feature) {
            Ok(())
        } else {
            anyhow::bail!("Unknown feature flag: {feature}")
        }
    }
}

#[derive(Debug, Parser)]
struct FeaturesCli {
    #[command(subcommand)]
    sub: FeaturesSubcommand,
}

#[derive(Debug, Parser)]
enum FeaturesSubcommand {
    /// List known features with their stage and effective state.
    List,
    /// Enable a feature in config.toml.
    Enable(FeatureSetArgs),
    /// Disable a feature in config.toml.
    Disable(FeatureSetArgs),
}

#[derive(Debug, Parser)]
struct FeatureSetArgs {
    /// Feature key to update (for example: unified_exec).
    feature: String,
}

fn stage_str(stage: Stage) -> &'static str {
    match stage {
        Stage::UnderDevelopment => "under development",
        Stage::Experimental { .. } => "experimental",
        Stage::Stable => "stable",
        Stage::Deprecated => "deprecated",
        Stage::Removed => "removed",
    }
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        cli_main(arg0_paths).await?;
        Ok(())
    })
}

async fn cli_main(arg0_paths: Arg0DispatchPaths) -> anyhow::Result<()> {
    let MultitoolCli {
        config_overrides: mut root_config_overrides,
        feature_toggles,
        remote,
        mut interactive,
        subcommand,
    } = MultitoolCli::parse();

    // Fold --enable/--disable into config overrides so they flow to all subcommands.
    let toggle_overrides = feature_toggles.to_overrides()?;
    root_config_overrides.raw_overrides.extend(toggle_overrides);
    let root_remote = remote.remote;
    let root_remote_auth_token_env = remote.remote_auth_token_env;
    let root_strict_config = interactive.strict_config;
    reject_root_strict_config_for_subcommand(root_strict_config, &subcommand)?;
    if let Some(subcommand) = subcommand.as_ref() {
        profile_v2_for_subcommand(&interactive, subcommand)?;
    }

    match subcommand {
        None => {
            prepend_config_flags(
                &mut interactive.config_overrides,
                root_config_overrides.clone(),
            );
            let exit_info = run_interactive_tui(
                interactive,
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "exec",
            )?;
            exec_cli
                .shared
                .inherit_exec_root_options(&interactive.shared);
            exec_cli.strict_config |= root_strict_config;
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            deepseek_vibing_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::Review(ReviewCommand {
            strict_config,
            args: review_args,
        })) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "review",
            )?;
            let mut exec_cli = ExecCli::try_parse_from(["deepseek-vibing", "exec"])?;
            exec_cli
                .shared
                .inherit_exec_root_options(&interactive.shared);
            exec_cli.command = Some(ExecCommand::Review(review_args));
            exec_cli.strict_config = strict_config || root_strict_config;
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            deepseek_vibing_exec::run_main(exec_cli, arg0_paths.clone()).await?;
        }
        Some(Subcommand::McpServer(McpServerCommand { strict_config })) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp-server",
            )?;
            deepseek_vibing_mcp_server::run_main(
                arg0_paths.clone(),
                root_config_overrides,
                strict_config || root_strict_config,
            )
            .await?;
        }
        Some(Subcommand::Mcp(mut mcp_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "mcp",
            )?;
            // Propagate any root-level config overrides (e.g. `-c key=value`).
            prepend_config_flags(&mut mcp_cli.config_overrides, root_config_overrides.clone());
            let loader_overrides =
                loader_overrides_for_profile(interactive.config_profile_v2.as_ref())?;
            mcp_cli.run(loader_overrides).await?;
        }
        Some(Subcommand::Plugin(plugin_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "plugin",
            )?;
            let PluginCli {
                mut config_overrides,
                subcommand,
            } = plugin_cli;
            prepend_config_flags(&mut config_overrides, root_config_overrides.clone());
            match subcommand {
                PluginSubcommand::Add(args) => {
                    let overrides = config_overrides
                        .parse_overrides()
                        .map_err(anyhow::Error::msg)?;
                    plugin_cmd::run_plugin_add(overrides, args).await?;
                }
                PluginSubcommand::List(args) => {
                    let overrides = config_overrides
                        .parse_overrides()
                        .map_err(anyhow::Error::msg)?;
                    plugin_cmd::run_plugin_list(overrides, args).await?;
                }
                PluginSubcommand::Marketplace(mut marketplace_cli) => {
                    prepend_config_flags(&mut marketplace_cli.config_overrides, config_overrides);
                    marketplace_cli.run().await?;
                }
                PluginSubcommand::Remove(args) => {
                    let overrides = config_overrides
                        .parse_overrides()
                        .map_err(anyhow::Error::msg)?;
                    plugin_cmd::run_plugin_remove(overrides, args).await?;
                }
            }
        }
        Some(Subcommand::AppServer(app_server_cli)) => {
            let AppServerCommand {
                subcommand,
                strict_config: app_server_strict_config,
                listen,
                stdio,
                remote_control,
                analytics_default_enabled,
                auth,
            } = app_server_cli;
            let strict_config = app_server_strict_config || root_strict_config;
            reject_strict_config_for_app_server_subcommand(strict_config, subcommand.as_ref())?;
            reject_remote_mode_for_app_server_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                subcommand.as_ref(),
            )?;
            match subcommand {
                None => {
                    let transport = if stdio {
                        deepseek_vibing_app_server::AppServerTransport::Stdio
                    } else {
                        listen
                    };
                    let auth = auth.try_into_settings()?;
                    let runtime_options = deepseek_vibing_app_server::AppServerRuntimeOptions {
                        remote_control_enabled: remote_control,
                        ..Default::default()
                    };
                    deepseek_vibing_app_server::run_main_with_transport_options(
                        arg0_paths.clone(),
                        root_config_overrides,
                        LoaderOverrides::default(),
                        strict_config,
                        analytics_default_enabled,
                        transport,
                        deepseek_vibing_protocol::protocol::SessionSource::VSCode,
                        auth,
                        runtime_options,
                    )
                    .await?;
                }
                Some(AppServerSubcommand::Daemon(daemon_cli)) => match daemon_cli.subcommand {
                    AppServerDaemonSubcommand::Start => {
                        print_app_server_daemon_output(AppServerLifecycleCommand::Start).await?;
                    }
                    AppServerDaemonSubcommand::Bootstrap(bootstrap_cli) => {
                        let output = deepseek_vibing_app_server_daemon::bootstrap(
                            AppServerBootstrapOptions {
                                remote_control_enabled: bootstrap_cli.remote_control,
                            },
                        )
                        .await?;
                        println!("{}", serde_json::to_string(&output)?);
                    }
                    AppServerDaemonSubcommand::Restart => {
                        print_app_server_daemon_output(AppServerLifecycleCommand::Restart).await?;
                    }
                    AppServerDaemonSubcommand::EnableRemoteControl => {
                        print_app_server_remote_control_output(AppServerRemoteControlMode::Enabled)
                            .await?;
                    }
                    AppServerDaemonSubcommand::DisableRemoteControl => {
                        print_app_server_remote_control_output(
                            AppServerRemoteControlMode::Disabled,
                        )
                        .await?;
                    }
                    AppServerDaemonSubcommand::Stop => {
                        print_app_server_daemon_output(AppServerLifecycleCommand::Stop).await?;
                    }
                    AppServerDaemonSubcommand::Version => {
                        print_app_server_daemon_output(AppServerLifecycleCommand::Version).await?;
                    }
                    AppServerDaemonSubcommand::PidUpdateLoop => {
                        deepseek_vibing_app_server_daemon::run_pid_update_loop().await?;
                    }
                },
                Some(AppServerSubcommand::Proxy(proxy_cli)) => {
                    let socket_path = match proxy_cli.socket_path {
                        Some(socket_path) => socket_path,
                        None => {
                            let deepseek_vibing_home = find_deepseek_vibing_home()?;
                            deepseek_vibing_app_server::app_server_control_socket_path(
                                &deepseek_vibing_home,
                            )?
                        }
                    };
                    deepseek_vibing_stdio_to_uds::run(socket_path.as_path()).await?;
                }
                Some(AppServerSubcommand::GenerateTs(gen_cli)) => {
                    let options = deepseek_vibing_app_server_protocol::GenerateTsOptions {
                        experimental_api: gen_cli.experimental,
                        ..Default::default()
                    };
                    deepseek_vibing_app_server_protocol::generate_ts_with_options(
                        &gen_cli.out_dir,
                        gen_cli.prettier.as_deref(),
                        options,
                    )?;
                }
                Some(AppServerSubcommand::GenerateJsonSchema(gen_cli)) => {
                    deepseek_vibing_app_server_protocol::generate_json_with_experimental(
                        &gen_cli.out_dir,
                        gen_cli.experimental,
                    )?;
                }
                Some(AppServerSubcommand::GenerateInternalJsonSchema(gen_cli)) => {
                    deepseek_vibing_app_server_protocol::generate_internal_json_schema(
                        &gen_cli.out_dir,
                    )?;
                }
            }
        }
        Some(Subcommand::RemoteControl(remote_control_cli)) => {
            let subcommand_name = remote_control_cli.subcommand_name();
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                subcommand_name,
            )?;
            remote_control_cmd::run(
                remote_control_cli,
                arg0_paths.clone(),
                root_config_overrides,
            )
            .await?;
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Some(Subcommand::App(_app_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "app",
            )?;
            anyhow::bail!(
                "`deepseek-vibing app` is not available in this DeepSeek-focused fork yet."
            );
        }
        Some(Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            include_non_interactive,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_resume_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                include_non_interactive,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Archive(cmd)) => {
            let output = run_session_archive_cli_command(
                deepseek_vibing_tui::SessionArchiveAction::Archive,
                cmd,
                interactive,
                root_config_overrides.clone(),
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
                arg0_paths.clone(),
            )
            .await?;
            println!("{output}");
        }
        Some(Subcommand::Unarchive(cmd)) => {
            let output = run_session_archive_cli_command(
                deepseek_vibing_tui::SessionArchiveAction::Unarchive,
                cmd,
                interactive,
                root_config_overrides.clone(),
                root_remote.clone(),
                root_remote_auth_token_env.clone(),
                arg0_paths.clone(),
            )
            .await?;
            println!("{output}");
        }
        Some(Subcommand::Fork(ForkCommand {
            session_id,
            last,
            all,
            remote,
            config_overrides,
        })) => {
            interactive = finalize_fork_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                all,
                config_overrides,
            );
            let exit_info = run_interactive_tui(
                interactive,
                remote.remote.or(root_remote.clone()),
                remote
                    .remote_auth_token_env
                    .or(root_remote_auth_token_env.clone()),
                arg0_paths.clone(),
            )
            .await?;
            handle_app_exit(exit_info)?;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "completion",
            )?;
            print_completion(completion_cli);
        }
        Some(Subcommand::Fim(mut fim_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "fim",
            )?;
            prepend_config_flags(&mut fim_cli.config_overrides, root_config_overrides.clone());
            run_fim_command(fim_cli).await?;
        }
        Some(Subcommand::Trajectory(mut trajectory_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "trajectory",
            )?;
            prepend_config_flags(
                &mut trajectory_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_trajectory_command(trajectory_cli).await?;
        }
        Some(Subcommand::Eval(mut eval_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "eval",
            )?;
            prepend_config_flags(
                &mut eval_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_eval_command(eval_cli).await?;
        }
        Some(Subcommand::Update) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "update",
            )?;
            anyhow::bail!(
                "`deepseek-vibing update` is not available in this DeepSeek-focused fork yet."
            );
        }
        Some(Subcommand::Doctor(doctor_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "doctor",
            )?;
            doctor::run_doctor(
                doctor_cli,
                root_config_overrides.clone(),
                &interactive,
                &arg0_paths,
            )
            .await?;
        }
        Some(Subcommand::Sandbox(mut sandbox_cli)) => {
            #[cfg(target_os = "windows")]
            if let Some(setup_cli) = sandbox_setup::parse_setup_command(&sandbox_cli.command)? {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "sandbox setup",
                )?;
                sandbox_setup::run(setup_cli).await?;
                return Ok(());
            }
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "sandbox",
            )?;
            let config_profile = sandbox_cli
                .config_profile
                .as_ref()
                .or(interactive.config_profile_v2.as_ref());
            let loader_overrides = loader_overrides_for_profile(config_profile)?;
            prepend_config_flags(
                &mut sandbox_cli.config_overrides,
                root_config_overrides.clone(),
            );
            #[cfg(target_os = "macos")]
            deepseek_vibing_cli::run_command_under_seatbelt(
                sandbox_cli,
                arg0_paths.deepseek_vibing_linux_sandbox_exe.clone(),
                loader_overrides,
            )
            .await?;
            #[cfg(target_os = "linux")]
            deepseek_vibing_cli::run_command_under_landlock(
                sandbox_cli,
                arg0_paths.deepseek_vibing_linux_sandbox_exe.clone(),
                loader_overrides,
            )
            .await?;
            #[cfg(target_os = "windows")]
            deepseek_vibing_cli::run_command_under_windows_sandbox(
                sandbox_cli,
                arg0_paths.deepseek_vibing_linux_sandbox_exe.clone(),
                loader_overrides,
            )
            .await?;
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                let _ = loader_overrides;
                anyhow::bail!(
                    "`deepseek-vibing sandbox` is not supported on this operating system"
                );
            }
        }
        Some(Subcommand::Debug(DebugCommand { subcommand })) => match subcommand {
            DebugSubcommand::Models(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug models",
                )?;
                run_debug_models_command(cmd, root_config_overrides).await?;
            }
            DebugSubcommand::AppServer(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug app-server",
                )?;
                run_debug_app_server_command(cmd).await?;
            }
            DebugSubcommand::PromptInput(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug prompt-input",
                )?;
                run_debug_prompt_input_command(
                    cmd,
                    root_config_overrides,
                    interactive,
                    arg0_paths.clone(),
                )
                .await?;
            }
            DebugSubcommand::TraceReduce(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug trace-reduce",
                )?;
                run_debug_trace_reduce_command(cmd).await?;
            }
            DebugSubcommand::ClearMemories => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "debug clear-memories",
                )?;
                run_debug_clear_memories_command(&root_config_overrides).await?;
            }
        },
        Some(Subcommand::Execpolicy(ExecpolicyCommand { sub })) => match sub {
            ExecpolicySubcommand::Check(cmd) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "execpolicy check",
                )?;
                run_execpolicycheck(cmd)?
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "apply",
            )?;
            prepend_config_flags(
                &mut apply_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_apply_command(apply_cli).await?;
        }
        Some(Subcommand::StdioToUds(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "stdio-to-uds",
            )?;
            let socket_path = cmd.socket_path;
            deepseek_vibing_stdio_to_uds::run(socket_path.as_path()).await?;
        }
        Some(Subcommand::ExecServer(cmd)) => {
            reject_remote_mode_for_subcommand(
                root_remote.as_deref(),
                root_remote_auth_token_env.as_deref(),
                "exec-server",
            )?;
            let strict_config = cmd.strict_config || root_strict_config;
            run_exec_server_command(cmd, &arg0_paths, &root_config_overrides, strict_config)
                .await?;
        }
        Some(Subcommand::Features(FeaturesCli { sub })) => match sub {
            FeaturesSubcommand::List => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features list",
                )?;
                let mut cli_kv_overrides = root_config_overrides
                    .parse_overrides()
                    .map_err(anyhow::Error::msg)?;

                // Honor `--search` via the canonical web_search mode.
                if interactive.web_search {
                    cli_kv_overrides.push((
                        "web_search".to_string(),
                        toml::Value::String("live".to_string()),
                    ));
                }

                let config = ConfigBuilder::default()
                    .cli_overrides(cli_kv_overrides)
                    .build()
                    .await?;
                let mut rows = Vec::with_capacity(FEATURES.len());
                let mut name_width = 0;
                let mut stage_width = 0;
                for def in FEATURES {
                    let name = def.key;
                    let stage = stage_str(def.stage);
                    let enabled = config.features.enabled(def.id);
                    name_width = name_width.max(name.len());
                    stage_width = stage_width.max(stage.len());
                    rows.push((name, stage, enabled));
                }
                rows.sort_unstable_by_key(|(name, _, _)| *name);

                for (name, stage, enabled) in rows {
                    println!("{name:<name_width$}  {stage:<stage_width$}  {enabled}");
                }
            }
            FeaturesSubcommand::Enable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features enable",
                )?;
                enable_feature_in_config(&feature).await?;
            }
            FeaturesSubcommand::Disable(FeatureSetArgs { feature }) => {
                reject_remote_mode_for_subcommand(
                    root_remote.as_deref(),
                    root_remote_auth_token_env.as_deref(),
                    "features disable",
                )?;
                disable_feature_in_config(&feature).await?;
            }
        },
    }

    Ok(())
}

fn profile_v2_for_subcommand<'a>(
    interactive: &'a TuiCli,
    subcommand: &Subcommand,
) -> anyhow::Result<Option<&'a ProfileV2Name>> {
    let Some(profile_v2) = interactive.config_profile_v2.as_ref() else {
        return Ok(None);
    };

    match subcommand {
        Subcommand::Exec(_)
        | Subcommand::Review(_)
        | Subcommand::Fim(_)
        | Subcommand::Trajectory(_)
        | Subcommand::Eval(_)
        | Subcommand::Resume(_)
        | Subcommand::Archive(_)
        | Subcommand::Unarchive(_)
        | Subcommand::Fork(_)
        | Subcommand::Mcp(_)
        | Subcommand::Sandbox(_)
        | Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::PromptInput(_),
        }) => Ok(Some(profile_v2)),
        _ => anyhow::bail!(
            "--profile only applies to runtime commands and `deepseek-vibing mcp`: `deepseek-vibing`, `deepseek-vibing exec`, `deepseek-vibing review`, `deepseek-vibing fim`, `deepseek-vibing trajectory`, `deepseek-vibing eval`, `deepseek-vibing resume`, `deepseek-vibing archive`, `deepseek-vibing unarchive`, `deepseek-vibing fork`, `deepseek-vibing mcp`, `deepseek-vibing sandbox`, and `deepseek-vibing debug prompt-input`."
        ),
    }
}

async fn run_exec_server_command(
    cmd: ExecServerCommand,
    arg0_paths: &Arg0DispatchPaths,
    root_config_overrides: &CliConfigOverrides,
    strict_config: bool,
) -> anyhow::Result<()> {
    let deepseek_vibing_self_exe = arg0_paths
        .deepseek_vibing_self_exe
        .clone()
        .ok_or_else(|| anyhow::anyhow!("DeepSeekVibing executable path is not configured"))?;
    let runtime_paths = deepseek_vibing_exec_server::ExecServerRuntimePaths::new(
        deepseek_vibing_self_exe,
        arg0_paths.deepseek_vibing_linux_sandbox_exe.clone(),
    )?;
    if let Some(base_url) = cmd.remote {
        let environment_id = cmd
            .environment_id
            .ok_or_else(|| anyhow::anyhow!("--environment-id is required when --remote is set"))?;
        let config = load_exec_server_config(root_config_overrides, strict_config).await?;
        let auth_provider =
            load_exec_server_remote_auth_provider(&config, &base_url, cmd.use_agent_identity_auth)
                .await?;
        let mut remote_config = deepseek_vibing_exec_server::RemoteEnvironmentConfig::new(
            base_url,
            environment_id,
            auth_provider,
        )?;
        if let Some(name) = cmd.name {
            remote_config.name = name;
        }
        deepseek_vibing_exec_server::run_remote_environment(remote_config, runtime_paths).await?;
        Ok(())
    } else {
        if strict_config {
            // Local exec-server startup does not consume Config, but strict
            // mode should still reject unknown fields before opening a listener.
            let _validated_config =
                load_exec_server_config(root_config_overrides, strict_config).await?;
        }
        let listen_url = cmd
            .listen
            .as_deref()
            .unwrap_or(deepseek_vibing_exec_server::DEFAULT_LISTEN_URL);
        deepseek_vibing_exec_server::run_main(listen_url, runtime_paths)
            .await
            .map_err(anyhow::Error::from_boxed)
    }
}

async fn load_exec_server_remote_auth_provider(
    config: &deepseek_vibing_core::config::Config,
    base_url: &str,
    use_agent_identity_auth: bool,
) -> anyhow::Result<deepseek_vibing_api::SharedAuthProvider> {
    if use_agent_identity_auth {
        anyhow::bail!(
            "Agent Identity auth has been removed from DeepSeek Vibing; use `[auth].deepseek_api_key` in config.toml"
        );
    }

    let auth = load_exec_server_remote_auth(
        config,
        "remote exec-server registration requires API key authentication; set `[auth].deepseek_api_key` in DeepSeek Vibing config.toml or configure a provider API key",
    )
    .await?;

    if !is_supported_exec_server_remote_auth(&auth) {
        anyhow::bail!(
            "remote exec-server registration requires API key authentication from DeepSeek Vibing config.toml"
        );
    }

    if auth.is_api_key_auth() {
        validate_api_key_remote_host(base_url)?;
    }

    Ok(deepseek_vibing_model_provider::auth_provider_from_auth(
        &auth,
    ))
}

fn is_supported_exec_server_remote_auth(auth: &DeepSeekVibingAuth) -> bool {
    auth.is_chatgpt_auth() || auth.is_api_key_auth()
}

fn validate_api_key_remote_host(base_url: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(base_url)
        .map_err(|err| anyhow::anyhow!("invalid remote exec-server registration URL: {err}"))?;
    let host = url.host().ok_or_else(|| {
        anyhow::anyhow!("remote exec-server registration URL must include a host")
    })?;

    let is_loopback = match &host {
        url::Host::Domain(host) => host.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    };
    let is_openai_host = match &host {
        url::Host::Domain(host) => ["openai.com", "openai.org"].into_iter().any(|domain| {
            host.eq_ignore_ascii_case(domain)
                || host.to_ascii_lowercase().ends_with(&format!(".{domain}"))
        }),
        _ => false,
    };
    let is_allowed = match url.scheme() {
        "https" => is_loopback || is_openai_host,
        "http" => is_loopback,
        _ => false,
    };

    if !is_allowed {
        anyhow::bail!(
            "remote exec-server API-key authentication is restricted to HTTPS openai.com and openai.org hosts and subdomains or loopback hosts"
        );
    }

    Ok(())
}

async fn load_exec_server_config(
    root_config_overrides: &CliConfigOverrides,
    strict_config: bool,
) -> anyhow::Result<deepseek_vibing_core::config::Config> {
    let cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    Ok(ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides)
        .strict_config(strict_config)
        .build()
        .await?)
}

async fn load_exec_server_remote_auth(
    config: &deepseek_vibing_core::config::Config,
    missing_auth_error: &'static str,
) -> anyhow::Result<deepseek_vibing_auth::DeepSeekVibingAuth> {
    let auth_manager =
        AuthManager::shared_from_config(config, /*enable_deepseek_vibing_api_key_env*/ true).await;

    let auth = match auth_manager.auth().await {
        Some(auth) => auth,
        None => {
            auth_manager.reload().await;
            auth_manager
                .auth()
                .await
                .ok_or_else(|| anyhow::anyhow!(missing_auth_error))?
        }
    };

    Ok(auth)
}

async fn enable_feature_in_config(feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let deepseek_vibing_home = find_deepseek_vibing_home()?;
    ConfigEditsBuilder::new(&deepseek_vibing_home)
        .set_feature_enabled(feature, /*enabled*/ true)
        .apply()
        .await?;
    println!("Enabled feature `{feature}` in config.toml.");
    maybe_print_under_development_feature_warning(&deepseek_vibing_home, feature);
    Ok(())
}

async fn disable_feature_in_config(feature: &str) -> anyhow::Result<()> {
    FeatureToggles::validate_feature(feature)?;
    let deepseek_vibing_home = find_deepseek_vibing_home()?;
    ConfigEditsBuilder::new(&deepseek_vibing_home)
        .set_feature_enabled(feature, /*enabled*/ false)
        .apply()
        .await?;
    println!("Disabled feature `{feature}` in config.toml.");
    Ok(())
}

fn loader_overrides_for_profile(
    profile_v2: Option<&ProfileV2Name>,
) -> anyhow::Result<LoaderOverrides> {
    match profile_v2 {
        Some(profile_v2) => {
            let deepseek_vibing_home = find_deepseek_vibing_home()?;
            Ok(LoaderOverrides {
                user_config_path: Some(resolve_profile_v2_config_path(
                    &deepseek_vibing_home,
                    profile_v2,
                )),
                user_config_profile: Some(profile_v2.clone()),
                ..Default::default()
            })
        }
        None => Ok(LoaderOverrides::default()),
    }
}

fn maybe_print_under_development_feature_warning(
    deepseek_vibing_home: &std::path::Path,
    feature: &str,
) {
    let Some(spec) = FEATURES.iter().find(|spec| spec.key == feature) else {
        return;
    };
    if !matches!(spec.stage, Stage::UnderDevelopment) {
        return;
    }

    let config_path = deepseek_vibing_home.join(deepseek_vibing_config::CONFIG_TOML_FILE);
    eprintln!(
        "Under-development features enabled: {feature}. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in {}.",
        config_path.display()
    );
}

async fn run_debug_trace_reduce_command(cmd: DebugTraceReduceCommand) -> anyhow::Result<()> {
    let output = cmd
        .output
        .unwrap_or_else(|| cmd.trace_bundle.join(REDUCED_STATE_FILE_NAME));

    let trace = replay_bundle(&cmd.trace_bundle)?;
    let reduced_json = serde_json::to_vec_pretty(&trace)?;
    tokio::fs::write(&output, reduced_json).await?;
    println!("{}", output.display());

    Ok(())
}

async fn run_debug_prompt_input_command(
    cmd: DebugPromptInputCommand,
    root_config_overrides: CliConfigOverrides,
    interactive: TuiCli,
    arg0_paths: Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let loader_overrides = loader_overrides_for_profile(interactive.config_profile_v2.as_ref())?;
    let shared = interactive.shared.into_inner();
    let mut cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    if interactive.web_search {
        cli_kv_overrides.push((
            "web_search".to_string(),
            toml::Value::String("live".to_string()),
        ));
    }

    let approval_policy = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(AskForApproval::Never)
    } else {
        interactive.approval_policy.map(Into::into)
    };
    let sandbox_mode = if shared.dangerously_bypass_approvals_and_sandbox {
        Some(deepseek_vibing_protocol::config_types::SandboxMode::DangerFullAccess)
    } else {
        shared.sandbox_mode.map(Into::into)
    };
    let overrides = ConfigOverrides {
        model: shared.model,
        approval_policy,
        sandbox_mode,
        cwd: shared.cwd,
        deepseek_vibing_self_exe: arg0_paths.deepseek_vibing_self_exe,
        deepseek_vibing_linux_sandbox_exe: arg0_paths.deepseek_vibing_linux_sandbox_exe,
        main_execve_wrapper_exe: arg0_paths.main_execve_wrapper_exe,
        show_raw_agent_reasoning: shared.oss.then_some(true),
        ephemeral: Some(true),
        bypass_hook_trust: shared.bypass_hook_trust.then_some(true),
        additional_writable_roots: shared.add_dir,
        ..Default::default()
    };
    let config = ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides)
        .harness_overrides(overrides)
        .loader_overrides(loader_overrides)
        .build()
        .await?;

    let mut input = shared
        .images
        .into_iter()
        .chain(cmd.images)
        .map(|path| UserInput::LocalImage { path, detail: None })
        .collect::<Vec<_>>();
    if let Some(prompt) = cmd.prompt.or(interactive.prompt) {
        input.push(UserInput::Text {
            text: prompt.replace("\r\n", "\n").replace('\r', "\n"),
            text_elements: Vec::new(),
        });
    }

    let prompt_input =
        deepseek_vibing_core::build_prompt_input(config, input, /*state_db*/ None).await?;
    println!("{}", serde_json::to_string_pretty(&prompt_input)?);

    Ok(())
}

async fn run_debug_models_command(
    cmd: DebugModelsCommand,
    root_config_overrides: CliConfigOverrides,
) -> anyhow::Result<()> {
    let catalog = if cmd.bundled {
        bundled_models_response()?
    } else {
        let cli_overrides = root_config_overrides
            .parse_overrides()
            .map_err(anyhow::Error::msg)?;
        let config = ConfigBuilder::default()
            .cli_overrides(cli_overrides)
            .build()
            .await?;
        let auth_manager = AuthManager::shared_from_config(
            &config, /*enable_deepseek_vibing_api_key_env*/ true,
        )
        .await;
        let models_manager = build_models_manager(&config, auth_manager);
        models_manager
            .raw_model_catalog(RefreshStrategy::OnlineIfUncached)
            .await
    };

    serde_json::to_writer(std::io::stdout(), &catalog)?;
    println!();
    Ok(())
}

async fn run_debug_clear_memories_command(
    root_config_overrides: &CliConfigOverrides,
) -> anyhow::Result<()> {
    let cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let config = ConfigBuilder::default()
        .cli_overrides(cli_kv_overrides)
        .build()
        .await?;

    let memories_path = memories_db_path(config.sqlite_home.as_path());
    let cleared_memories_db =
        StateRuntime::clear_memory_data_in_sqlite_home(config.sqlite_home.as_path()).await?;

    clear_memory_roots_contents(&config.deepseek_vibing_home).await?;

    let mut message = if cleared_memories_db {
        format!("Cleared memory state from {}.", memories_path.display())
    } else {
        format!("No memories db found at {}.", memories_path.display())
    };
    message.push_str(&format!(
        " Cleared memory directories under {}.",
        config.deepseek_vibing_home.display()
    ));

    println!("{message}");

    Ok(())
}

/// Prepend root-level overrides so they have lower precedence than
/// CLI-specific ones specified after the subcommand (if any).
fn prepend_config_flags(
    subcommand_config_overrides: &mut CliConfigOverrides,
    cli_config_overrides: CliConfigOverrides,
) {
    subcommand_config_overrides.prepend_root_overrides(cli_config_overrides);
}

fn reject_remote_mode_for_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: &str,
) -> anyhow::Result<()> {
    if let Some(remote) = remote {
        anyhow::bail!(
            "`--remote {remote}` is only supported for interactive TUI commands, not `deepseek-vibing {subcommand}`"
        );
    }
    if remote_auth_token_env.is_some() {
        anyhow::bail!(
            "`--remote-auth-token-env` is only supported for interactive TUI commands, not `deepseek-vibing {subcommand}`"
        );
    }
    Ok(())
}

fn reject_root_strict_config_for_subcommand(
    strict_config: bool,
    subcommand: &Option<Subcommand>,
) -> anyhow::Result<()> {
    if !strict_config {
        return Ok(());
    }

    match unsupported_subcommand_name_for_strict_config(subcommand) {
        Some(subcommand_name) => {
            reject_strict_config_for_unsupported_subcommand(strict_config, subcommand_name)
        }
        None => Ok(()),
    }
}

/// Return the selected subcommand name when a root-level `--strict-config`
/// flag should be rejected after parsing.
///
/// `--strict-config` is parsed on the root interactive CLI so commands like
/// `deepseek-vibing --strict-config` continue to work for the TUI and for wrappers that
/// forward root options into another command shape. Clap will still accept that
/// root flag before the dispatcher knows which subcommand the user selected, so
/// unsupported subcommands need an explicit post-parse reject path.
///
/// `Some(...)` returns the user-facing command name fragment to embed in the
/// rejection error, such as `cloud` or `app-server proxy`. `None` means the
/// selected command is allowed to inherit root `--strict-config`.
fn unsupported_subcommand_name_for_strict_config(
    subcommand: &Option<Subcommand>,
) -> Option<&'static str> {
    match subcommand {
        None
        | Some(Subcommand::Exec(_))
        | Some(Subcommand::Review(_))
        | Some(Subcommand::Fim(_))
        | Some(Subcommand::Trajectory(_))
        | Some(Subcommand::Eval(_))
        | Some(Subcommand::McpServer(_))
        | Some(Subcommand::ExecServer(_))
        | Some(Subcommand::Resume(_))
        | Some(Subcommand::Archive(_))
        | Some(Subcommand::Unarchive(_))
        | Some(Subcommand::Fork(_))
        | Some(Subcommand::Doctor(_)) => None,
        Some(Subcommand::AppServer(app_server)) if app_server.subcommand.is_none() => None,
        Some(Subcommand::AppServer(app_server)) => {
            Some(app_server_subcommand_name(app_server.subcommand.as_ref()))
        }
        Some(Subcommand::RemoteControl(remote_control)) => Some(remote_control.subcommand_name()),
        Some(Subcommand::Mcp(_)) => Some("mcp"),
        Some(Subcommand::Plugin(_)) => Some("plugin"),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Some(Subcommand::App(_)) => Some("app"),
        Some(Subcommand::Completion(_)) => Some("completion"),
        Some(Subcommand::Update) => Some("update"),
        Some(Subcommand::Sandbox(_)) => Some("sandbox"),
        Some(Subcommand::Debug(_)) => Some("debug"),
        Some(Subcommand::Execpolicy(_)) => Some("execpolicy"),
        Some(Subcommand::Apply(_)) => Some("apply"),
        Some(Subcommand::StdioToUds(_)) => Some("stdio-to-uds"),
        Some(Subcommand::Features(_)) => Some("features"),
    }
}

fn reject_strict_config_for_app_server_subcommand(
    strict_config: bool,
    subcommand: Option<&AppServerSubcommand>,
) -> anyhow::Result<()> {
    if subcommand.is_none() {
        return Ok(());
    }
    reject_strict_config_for_unsupported_subcommand(
        strict_config,
        app_server_subcommand_name(subcommand),
    )
}

fn reject_strict_config_for_unsupported_subcommand(
    strict_config: bool,
    subcommand: &str,
) -> anyhow::Result<()> {
    if strict_config {
        anyhow::bail!("`--strict-config` is not supported for `deepseek-vibing {subcommand}`");
    }
    Ok(())
}

fn reject_remote_mode_for_app_server_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: Option<&AppServerSubcommand>,
) -> anyhow::Result<()> {
    let subcommand_name = app_server_subcommand_name(subcommand);
    reject_remote_mode_for_subcommand(remote, remote_auth_token_env, subcommand_name)
}

fn app_server_subcommand_name(subcommand: Option<&AppServerSubcommand>) -> &'static str {
    match subcommand {
        None => "app-server",
        Some(AppServerSubcommand::Daemon(daemon)) => match daemon.subcommand {
            AppServerDaemonSubcommand::Bootstrap(_) => "app-server daemon bootstrap",
            AppServerDaemonSubcommand::Start => "app-server daemon start",
            AppServerDaemonSubcommand::Restart => "app-server daemon restart",
            AppServerDaemonSubcommand::EnableRemoteControl => {
                "app-server daemon enable-remote-control"
            }
            AppServerDaemonSubcommand::DisableRemoteControl => {
                "app-server daemon disable-remote-control"
            }
            AppServerDaemonSubcommand::Stop => "app-server daemon stop",
            AppServerDaemonSubcommand::Version => "app-server daemon version",
            AppServerDaemonSubcommand::PidUpdateLoop => "app-server daemon pid-update-loop",
        },
        Some(AppServerSubcommand::Proxy(_)) => "app-server proxy",
        Some(AppServerSubcommand::GenerateTs(_)) => "app-server generate-ts",
        Some(AppServerSubcommand::GenerateJsonSchema(_)) => "app-server generate-json-schema",
        Some(AppServerSubcommand::GenerateInternalJsonSchema(_)) => {
            "app-server generate-internal-json-schema"
        }
    }
}

async fn print_app_server_daemon_output(command: AppServerLifecycleCommand) -> anyhow::Result<()> {
    let output = deepseek_vibing_app_server_daemon::run(command).await?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

async fn print_app_server_remote_control_output(
    mode: AppServerRemoteControlMode,
) -> anyhow::Result<()> {
    let output = deepseek_vibing_app_server_daemon::set_remote_control(mode).await?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn read_remote_auth_token_from_env_var_with<F>(
    env_var_name: &str,
    get_var: F,
) -> anyhow::Result<String>
where
    F: FnOnce(&str) -> Result<String, std::env::VarError>,
{
    let auth_token = get_var(env_var_name)
        .map_err(|_| anyhow::anyhow!("environment variable `{env_var_name}` is not set"))?;
    let auth_token = auth_token.trim().to_string();
    if auth_token.is_empty() {
        anyhow::bail!("environment variable `{env_var_name}` is empty");
    }
    Ok(auth_token)
}

fn read_remote_auth_token_from_env_var(env_var_name: &str) -> anyhow::Result<String> {
    read_remote_auth_token_from_env_var_with(env_var_name, |name| std::env::var(name))
}

async fn run_interactive_tui(
    mut interactive: TuiCli,
    remote: Option<String>,
    remote_auth_token_env: Option<String>,
    arg0_paths: Arg0DispatchPaths,
) -> std::io::Result<AppExitInfo> {
    if let Some(prompt) = interactive.prompt.take() {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    let terminal_info = deepseek_vibing_terminal_detection::terminal_info();
    if terminal_info.name == TerminalName::Dumb {
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            return Ok(AppExitInfo::fatal(
                "TERM is set to \"dumb\". Refusing to start the interactive TUI because no terminal is available for a confirmation prompt (stdin/stderr is not a TTY). Run in a supported terminal or unset TERM.",
            ));
        }

        eprintln!(
            "WARNING: TERM is set to \"dumb\". DeepSeekVibing's interactive TUI may not work in this terminal."
        );
        if !confirm("Continue anyway? [y/N]: ")? {
            return Ok(AppExitInfo::fatal(
                "Refusing to start the interactive TUI because TERM is set to \"dumb\". Run in a supported terminal or unset TERM.",
            ));
        }
    }

    let remote_endpoint = match resolve_remote_endpoint(remote, remote_auth_token_env) {
        Ok(remote_endpoint) => remote_endpoint,
        Err(err) if is_remote_auth_usage_error(&err) => {
            return Ok(AppExitInfo::fatal(err.to_string()));
        }
        Err(err) => return Err(err),
    };
    let start_tui = || {
        deepseek_vibing_tui::run_main(
            interactive.clone(),
            arg0_paths.clone(),
            deepseek_vibing_config::LoaderOverrides::default(),
            remote_endpoint.clone(),
        )
    };
    let mut attempted_repair = false;
    loop {
        let err = match start_tui().await {
            Ok(exit_info) => return Ok(exit_info),
            Err(err) => err,
        };
        let Some(startup_error) = local_state_db::startup_error(&err) else {
            return Err(err);
        };
        if local_state_db::is_locked(startup_error.detail()) {
            local_state_db::print_locked_guidance(startup_error);
            return Ok(AppExitInfo::fatal(startup_error.to_string()));
        }
        if attempted_repair {
            local_state_db::print_diagnostic_guidance(startup_error);
            return Ok(AppExitInfo::fatal(startup_error.to_string()));
        }
        if !local_state_db::confirm_repair(startup_error)? {
            local_state_db::print_diagnostic_guidance(startup_error);
            return Ok(AppExitInfo::fatal(startup_error.to_string()));
        }

        match local_state_db::repair_files(startup_error).await {
            Ok(backups) => local_state_db::print_repair_backups(&backups),
            Err(repair_err) => {
                local_state_db::print_diagnostic_guidance(startup_error);
                return Ok(AppExitInfo::fatal(format!(
                    "failed to repair DeepSeekVibing local data automatically: {repair_err}"
                )));
            }
        }
        attempted_repair = true;
    }
}

fn resolve_remote_endpoint(
    remote: Option<String>,
    remote_auth_token_env: Option<String>,
) -> std::io::Result<Option<deepseek_vibing_tui::RemoteAppServerEndpoint>> {
    let mut remote_endpoint = remote
        .as_deref()
        .map(deepseek_vibing_tui::resolve_remote_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    if let Some(remote_auth_token_env) = remote_auth_token_env {
        let Some(endpoint) = remote_endpoint.as_mut() else {
            return Err(std::io::Error::other(
                "`--remote-auth-token-env` requires `--remote`.",
            ));
        };
        if !deepseek_vibing_tui::remote_addr_supports_auth_token(endpoint) {
            return Err(std::io::Error::other(
                "`--remote-auth-token-env` requires a `wss://` or loopback `ws://` remote.",
            ));
        }
        let auth_token = read_remote_auth_token_from_env_var(&remote_auth_token_env)
            .map_err(std::io::Error::other)?;
        let deepseek_vibing_tui::RemoteAppServerEndpoint::WebSocket {
            auth_token: slot, ..
        } = endpoint
        else {
            return Err(std::io::Error::other(
                "`--remote-auth-token-env` requires a `wss://` or loopback `ws://` remote.",
            ));
        };
        *slot = Some(auth_token);
    }
    Ok(remote_endpoint)
}

fn is_remote_auth_usage_error(err: &std::io::Error) -> bool {
    err.to_string()
        .starts_with("`--remote-auth-token-env` requires")
}

fn confirm(prompt: &str) -> std::io::Result<bool> {
    eprintln!("{prompt}");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// Build the final `TuiCli` for a `deepseek-vibing resume` invocation.
fn finalize_resume_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    show_all: bool,
    include_non_interactive: bool,
    resume_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so resume shares the same
    // configuration surface area as `codex` without additional flags.
    let resume_session_id = session_id;
    interactive.resume_picker = resume_session_id.is_none() && !last;
    interactive.resume_last = last;
    interactive.resume_session_id = resume_session_id;
    interactive.resume_show_all = show_all;
    interactive.resume_include_non_interactive = include_non_interactive;

    // Merge resume-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, resume_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

/// Build the final `TuiCli` for a `deepseek-vibing fork` invocation.
fn finalize_fork_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    show_all: bool,
    fork_cli: TuiCli,
) -> TuiCli {
    // Start with the parsed interactive CLI so fork shares the same
    // configuration surface area as `codex` without additional flags.
    let fork_session_id = session_id;
    interactive.fork_picker = fork_session_id.is_none() && !last;
    interactive.fork_last = last;
    interactive.fork_session_id = fork_session_id;
    interactive.fork_show_all = show_all;

    // Merge fork-scoped flags and overrides with highest precedence.
    merge_interactive_cli_flags(&mut interactive, fork_cli);

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

fn finalize_session_archive_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    archive_cli: SessionArchiveConfigOverrides,
) -> TuiCli {
    let SessionArchiveConfigOverrides {
        shared,
        strict_config,
        config_overrides,
    } = archive_cli;
    interactive.shared.apply_subcommand_overrides(shared);
    if strict_config {
        interactive.strict_config = true;
    }
    interactive
        .config_overrides
        .raw_overrides
        .extend(config_overrides.raw_overrides);
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);
    interactive
}

/// Merge flags provided to runtime wrapper commands so they take precedence over any root-level
/// flags. Only overrides fields explicitly set on the subcommand-scoped CLI. Also appends
/// `-c key=value` overrides with highest precedence.
fn merge_interactive_cli_flags(interactive: &mut TuiCli, subcommand_cli: TuiCli) {
    let TuiCli {
        shared,
        strict_config,
        approval_policy,
        web_search,
        prompt,
        config_overrides,
        ..
    } = subcommand_cli;
    interactive
        .shared
        .apply_subcommand_overrides(shared.into_inner());
    if let Some(approval) = approval_policy {
        interactive.approval_policy = Some(approval);
    }
    if web_search {
        interactive.web_search = true;
    }
    if strict_config {
        interactive.strict_config = true;
    }
    if let Some(prompt) = prompt {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    interactive
        .config_overrides
        .raw_overrides
        .extend(config_overrides.raw_overrides);
}

async fn run_apply_command(command: ApplyCommand) -> anyhow::Result<()> {
    let diff = read_apply_patch_arg(&command.patch_file)?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    let req = ApplyGitRequest {
        cwd,
        diff,
        revert: command.revert,
        preflight: command.check,
    };
    let result = apply_git_patch(&req)?;
    if result.exit_code != 0 {
        anyhow::bail!(
            "git apply failed (applied={}, skipped={}, conflicts={})\nstdout:\n{}\nstderr:\n{}",
            result.applied_paths.len(),
            result.skipped_paths.len(),
            result.conflicted_paths.len(),
            result.stdout,
            result.stderr
        );
    }
    if command.check {
        println!("Patch applies cleanly");
    } else if command.revert {
        println!("Successfully reverted patch");
    } else {
        println!("Successfully applied patch");
    }
    Ok(())
}

fn read_apply_patch_arg(path: &PathBuf) -> anyhow::Result<String> {
    if is_stdin_path(path) {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| anyhow::anyhow!("failed to read patch from stdin: {err}"))?;
        return Ok(input);
    }
    std::fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("failed to read patch file `{}`: {err}", path.display()))
}

async fn run_fim_command(command: FimCommand) -> anyhow::Result<()> {
    validate_fim_command_inputs(&command)?;
    let json = command.json;
    let cli_overrides = command
        .config_overrides
        .clone()
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let config = Config::load_with_cli_overrides(cli_overrides).await?;
    let prefix = read_fim_file_arg(&command.prefix_file, "prefix")?;
    let suffix = command
        .suffix_file
        .as_ref()
        .map(|path| read_fim_file_arg(path, "suffix"))
        .transpose()?;
    let (fim_config, request) = build_fim_request_from_command(&config, command, prefix, suffix)?;
    let output =
        complete_fim_with_auth(&fim_config, config.auth.deepseek_api_key(), request).await?;
    let rendered = render_fim_output(&output, json)?;
    print!("{rendered}");
    Ok(())
}

async fn run_trajectory_command(command: TrajectoryCommand) -> anyhow::Result<()> {
    let config = load_trajectory_config(&command.config_overrides).await?;
    let root = trajectory_root_from_config(&config);
    match command.subcommand {
        TrajectorySubcommand::List => {
            for path in list_trajectory_files(&root)? {
                let id = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<unknown>");
                println!("{id}\t{}", path.display());
            }
        }
        TrajectorySubcommand::Show(show) => {
            let path = resolve_trajectory_path(&root, &show.selector)?;
            let summary = load_trajectory_summary(&path)?;
            if show.json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                print!("{}", render_trajectory_summary(&summary));
            }
        }
        TrajectorySubcommand::Replay(replay) => {
            let path = resolve_trajectory_path(&root, &replay.selector)?;
            let report = replay_trajectory(&path)?;
            if let Some(output) = replay.output {
                write_json_file(&output, &report)?;
                println!("Wrote replay report to {}", output.display());
            } else {
                print!("{}", render_trajectory_replay_report(&report));
            }
        }
    }
    Ok(())
}

async fn run_eval_command(command: EvalCommand) -> anyhow::Result<()> {
    let config = load_eval_config(&command.config_overrides).await?;
    match command.subcommand {
        EvalSubcommand::List(list) => {
            let dataset = eval_dataset_dir(&config, list.dataset)?;
            let tasks = load_eval_tasks(&dataset)?;
            if list.json {
                println!("{}", serde_json::to_string_pretty(&tasks)?);
            } else {
                print!("{}", render_eval_task_list(&tasks));
            }
        }
        EvalSubcommand::Run(run) => {
            run_eval_dataset(&config, run).await?;
        }
        EvalSubcommand::SummarizeTrajectories(summarize) => {
            let trajectory_root = summarize
                .trajectory_root
                .unwrap_or_else(|| trajectory_root_from_config(&config));
            let trajectories = load_eval_trajectory_summaries(&trajectory_root)?;
            let report = build_eval_report(EvalReportInput {
                run_id: new_eval_run_id(),
                mode: EvalMode::Offline,
                dataset_dir: None,
                trajectory_root: Some(trajectory_root),
                task_results: Vec::new(),
                trajectories,
                gates: &config.eval.gates,
                enforce: summarize.enforce,
                baseline: None,
            });
            let report_path = report_path_for_report(&config, &report, summarize.report);
            emit_eval_report(&report, Some(report_path), summarize.json, None)?;
        }
    }
    Ok(())
}

async fn run_eval_dataset(
    config: &Config,
    command: EvalRunCommand,
) -> anyhow::Result<EvalRunReport> {
    let mode = command
        .mode
        .map(EvalMode::from)
        .unwrap_or(config.eval.default_mode);
    let dataset = eval_dataset_dir(config, command.dataset)?;
    let mut tasks = load_eval_tasks(&dataset)?;
    if let Some(task_id) = command.task.as_ref() {
        tasks.retain(|task| task.id == *task_id);
        if tasks.is_empty() {
            anyhow::bail!(
                "eval task `{task_id}` was not found in {}",
                dataset.display()
            );
        }
    }

    let has_api_key =
        config.auth.deepseek_api_key().is_some() || std::env::var_os("DEEPSEEK_API_KEY").is_some();
    let live_enabled = match mode {
        EvalMode::Live => {
            if !has_api_key {
                anyhow::bail!(
                    "`[auth].deepseek_api_key` in DeepSeek Vibing config.toml or DEEPSEEK_API_KEY is required for `eval run --mode live`"
                );
            }
            true
        }
        EvalMode::Hybrid => has_api_key,
        EvalMode::Offline => false,
    };

    let run_id = new_eval_run_id();
    let work_root = eval_work_dir(config).join(&run_id);
    let live_trajectory_root = work_root.join("trajectories");
    let trajectory_root = if live_enabled {
        live_trajectory_root.clone()
    } else {
        trajectory_root_from_config(config)
    };
    let keep_workspaces = command.keep_workspaces || config.eval.keep_workspaces;
    let mut task_results = Vec::new();

    if live_enabled {
        let workspaces_root = work_root.join("workspaces");
        std::fs::create_dir_all(&workspaces_root)?;
        for task in &tasks {
            task_results.push(
                run_eval_live_task(
                    task,
                    &workspaces_root,
                    &live_trajectory_root,
                    config.eval.task_timeout_ms,
                    keep_workspaces,
                )
                .await?,
            );
        }
    } else {
        let skip_reason = match mode {
            EvalMode::Hybrid => {
                "DeepSeek API key is not configured in `[auth].deepseek_api_key` or DEEPSEEK_API_KEY; hybrid mode used offline metrics only"
            }
            EvalMode::Offline => "offline mode does not run live tasks",
            EvalMode::Live => unreachable!("live mode without key is rejected above"),
        }
        .to_string();
        task_results.extend(tasks.iter().map(|task| EvalTaskResult {
            task_id: task.id.clone(),
            title: task.title.clone(),
            tags: task.tags.clone(),
            status: EvalTaskStatus::Skipped,
            workspace: None,
            trajectory_paths: Vec::new(),
            verify: None,
            error: None,
            skip_reason: Some(skip_reason.clone()),
        }));
    }

    let trajectories = load_eval_trajectory_summaries(&trajectory_root)?;
    let baseline = command
        .baseline
        .as_ref()
        .map(read_eval_report)
        .transpose()?;
    let report = build_eval_report(EvalReportInput {
        run_id,
        mode,
        dataset_dir: Some(dataset),
        trajectory_root: Some(trajectory_root),
        task_results,
        trajectories,
        gates: &config.eval.gates,
        enforce: command.enforce,
        baseline: baseline.as_ref(),
    });

    let report_path = report_path_for_report(config, &report, command.report);
    emit_eval_report(&report, Some(report_path), command.json, baseline.as_ref())?;
    Ok(report)
}

async fn run_eval_live_task(
    task: &EvalTask,
    workspaces_root: &Path,
    trajectory_root: &Path,
    timeout_ms: u64,
    keep_workspace: bool,
) -> anyhow::Result<EvalTaskResult> {
    let workspace = workspaces_root.join(&task.id);
    materialize_task_workspace(task, &workspace)?;
    let before = trajectory_file_set(trajectory_root)?;
    let exec_output =
        run_deepseek_vibing_eval_exec(&workspace, trajectory_root, &task.prompt, timeout_ms).await;
    let after = trajectory_file_set(trajectory_root)?;
    let trajectory_paths = after.difference(&before).cloned().collect::<Vec<PathBuf>>();

    let (status, verify, error) = match exec_output {
        Ok(output) if output.exit_code == Some(0) => {
            let verify =
                run_eval_process_capture(&task.verify.command, &workspace, timeout_ms).await?;
            let passed = verify.exit_code == Some(0);
            (
                if passed {
                    EvalTaskStatus::Passed
                } else {
                    EvalTaskStatus::Failed
                },
                Some(verify),
                None,
            )
        }
        Ok(output) => (
            EvalTaskStatus::Error,
            None,
            Some(format!(
                "agent command exited with {}; stderr: {}",
                output
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "no exit code".to_string()),
                compact_single_line(&output.stderr)
            )),
        ),
        Err(err) => (EvalTaskStatus::Error, None, Some(err.to_string())),
    };

    let workspace_for_report = keep_workspace.then(|| workspace.clone());
    if !keep_workspace {
        let _ = std::fs::remove_dir_all(&workspace);
    }

    Ok(EvalTaskResult {
        task_id: task.id.clone(),
        title: task.title.clone(),
        tags: task.tags.clone(),
        status,
        workspace: workspace_for_report,
        trajectory_paths,
        verify,
        error,
        skip_reason: None,
    })
}

async fn run_deepseek_vibing_eval_exec(
    workspace: &Path,
    trajectory_root: &Path,
    prompt: &str,
    timeout_ms: u64,
) -> anyhow::Result<EvalCommandOutput> {
    let current_exe = std::env::current_exe()?;
    let args = build_eval_exec_args(workspace, trajectory_root, prompt);
    run_eval_process_capture_with_executable(&current_exe, &args, workspace, timeout_ms).await
}

async fn run_eval_process_capture(
    command: &[String],
    cwd: &Path,
    timeout_ms: u64,
) -> anyhow::Result<EvalCommandOutput> {
    let Some((executable, args)) = command.split_first() else {
        anyhow::bail!("verify.command must not be empty");
    };
    run_eval_process_capture_with_executable(Path::new(executable.as_str()), args, cwd, timeout_ms)
        .await
}

async fn run_eval_process_capture_with_executable<S: AsRef<str>>(
    executable: &Path,
    args: &[S],
    cwd: &Path,
    timeout_ms: u64,
) -> anyhow::Result<EvalCommandOutput> {
    let mut command = TokioCommand::new(executable);
    command.current_dir(cwd);
    for arg in args {
        command.arg(arg.as_ref());
    }
    let rendered_command = std::iter::once(executable.display().to_string())
        .chain(args.iter().map(|arg| arg.as_ref().to_string()))
        .collect::<Vec<String>>();
    let started = Instant::now();
    let output = tokio::time::timeout(Duration::from_millis(timeout_ms), command.output()).await;
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    match output {
        Ok(Ok(output)) => Ok(EvalCommandOutput {
            command: rendered_command,
            exit_code: output.status.code(),
            stdout: truncate_eval_output(&String::from_utf8_lossy(&output.stdout), 20_000),
            stderr: truncate_eval_output(&String::from_utf8_lossy(&output.stderr), 20_000),
            duration_ms,
        }),
        Ok(Err(err)) => Err(anyhow::anyhow!(
            "failed to run eval command `{}`: {err}",
            rendered_command.join(" ")
        )),
        Err(_) => Ok(EvalCommandOutput {
            command: rendered_command,
            exit_code: None,
            stdout: String::new(),
            stderr: format!("command timed out after {timeout_ms} ms"),
            duration_ms,
        }),
    }
}

fn build_eval_exec_args(workspace: &Path, trajectory_root: &Path, prompt: &str) -> Vec<String> {
    vec![
        "-c".to_string(),
        "trajectory.enabled=\"always\"".to_string(),
        "-c".to_string(),
        "trajectory.detail=\"summary\"".to_string(),
        "-c".to_string(),
        format!(
            "trajectory.root_dir={}",
            toml::Value::String(trajectory_root.display().to_string())
        ),
        "--ask-for-approval".to_string(),
        "never".to_string(),
        "exec".to_string(),
        "--json".to_string(),
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        "workspace-write".to_string(),
        "--cd".to_string(),
        workspace.display().to_string(),
        prompt.to_string(),
    ]
}

fn load_eval_trajectory_summaries(root: &Path) -> anyhow::Result<Vec<TrajectorySummary>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    list_trajectory_files(root)?
        .into_iter()
        .map(|path| load_trajectory_summary(&path))
        .collect()
}

fn trajectory_file_set(root: &Path) -> anyhow::Result<std::collections::HashSet<PathBuf>> {
    if !root.exists() {
        return Ok(std::collections::HashSet::new());
    }
    Ok(list_trajectory_files(root)?.into_iter().collect())
}

async fn load_eval_config(config_overrides: &CliConfigOverrides) -> anyhow::Result<Config> {
    let cli_overrides = config_overrides
        .clone()
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    Config::load_with_cli_overrides(cli_overrides)
        .await
        .map_err(Into::into)
}

fn eval_dataset_dir(config: &Config, override_path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path);
    }
    if let Some(path) = config.eval.dataset_dir.as_ref() {
        return Ok(path.to_path_buf());
    }
    Ok(default_eval_dataset_dir())
}

fn default_eval_dataset_dir() -> PathBuf {
    let cwd_default = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("evals/smoke");
    if cwd_default.exists() {
        return cwd_default;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|root| root.join("evals/smoke"))
        .unwrap_or(cwd_default)
}

fn eval_reports_dir(config: &Config) -> PathBuf {
    config
        .eval
        .reports_dir
        .as_ref()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| {
            config
                .deepseek_vibing_home
                .join("eval-reports")
                .to_path_buf()
        })
}

fn eval_work_dir(config: &Config) -> PathBuf {
    config
        .eval
        .work_dir
        .as_ref()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(default_eval_work_dir)
}

fn report_path_for_report(
    config: &Config,
    report: &EvalRunReport,
    override_path: Option<PathBuf>,
) -> PathBuf {
    override_path
        .unwrap_or_else(|| eval_reports_dir(config).join(format!("{}.json", report.run_id)))
}

fn read_eval_report(path: &PathBuf) -> anyhow::Result<EvalRunReport> {
    let contents = std::fs::read_to_string(path).map_err(|err| {
        anyhow::anyhow!("failed to read baseline report `{}`: {err}", path.display())
    })?;
    serde_json::from_str(&contents).map_err(|err| {
        anyhow::anyhow!(
            "failed to parse baseline report `{}`: {err}",
            path.display()
        )
    })
}

fn emit_eval_report(
    report: &EvalRunReport,
    report_path: Option<PathBuf>,
    json: bool,
    _baseline: Option<&EvalRunReport>,
) -> anyhow::Result<()> {
    if let Some(path) = report_path.as_ref() {
        write_json_file(path, report)?;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        print!("{}", render_eval_report(report, report_path.as_deref()));
    }
    if report.gate.enforced && !report.gate.passed {
        anyhow::bail!("eval quality gate failed");
    }
    Ok(())
}

fn render_eval_task_list(tasks: &[EvalTask]) -> String {
    let mut output = String::new();
    for task in tasks {
        let tags = if task.tags.is_empty() {
            "-".to_string()
        } else {
            task.tags.join(",")
        };
        output.push_str(&format!("{}\t{}\t{}\n", task.id, task.title, tags));
    }
    output
}

fn render_eval_report(report: &EvalRunReport, path: Option<&Path>) -> String {
    let mut output = String::new();
    output.push_str(&format!("Eval report: {}\n", report.run_id));
    output.push_str(&format!("Mode: {:?}\n", report.mode));
    if let Some(path) = path {
        output.push_str(&format!("Report: {}\n", path.display()));
    }
    if let Some(dataset_dir) = report.dataset_dir.as_ref() {
        output.push_str(&format!("Dataset: {}\n", dataset_dir.display()));
    }
    if let Some(trajectory_root) = report.trajectory_root.as_ref() {
        output.push_str(&format!("Trajectories: {}\n", trajectory_root.display()));
    }
    output.push_str(&format!(
        "Tasks: {} attempted, {} passed, {} failed/error, {} skipped\n",
        report.metrics.attempted_task_count,
        report.metrics.passed_task_count,
        report.metrics.failed_task_count,
        report.metrics.skipped_task_count
    ));
    output.push_str(&format!(
        "Pass rate: {}\n",
        format_optional_percent(report.metrics.pass_rate)
    ));
    output.push_str(&format!(
        "Avg turns: {}\n",
        format_optional_float(report.metrics.avg_turns)
    ));
    output.push_str(&format!(
        "Cache hit: {}\n",
        format_optional_percent(report.metrics.cache_hit_ratio)
    ));
    output.push_str(&format!(
        "Tool errors: {} of {} ({})\n",
        report.metrics.tool_error_count,
        report.metrics.total_tool_calls,
        format_optional_percent(report.metrics.tool_error_rate)
    ));
    output.push_str(&format!(
        "Patch accept rate: {}\n",
        format_optional_percent(report.metrics.patch_accept_rate)
    ));
    output.push_str(&format!(
        "Over-thinking tasks: {}\n",
        report.metrics.overthinking_task_count
    ));
    output.push_str(&format!(
        "Gate: {}{}\n",
        if report.gate.passed {
            "passed"
        } else {
            "failed"
        },
        if report.gate.enforced {
            " (enforced)"
        } else {
            " (report-only)"
        }
    ));
    for check in &report.gate.checks {
        output.push_str(&format!(
            "- {}: {}\n",
            check.name,
            if check.passed { "passed" } else { "failed" }
        ));
    }
    if !report.task_results.is_empty() {
        output.push_str("Tasks:\n");
        for task in &report.task_results {
            output.push_str(&format!(
                "- {} [{:?}] {}\n",
                task.task_id, task.status, task.title
            ));
            if let Some(reason) = task.skip_reason.as_ref() {
                output.push_str(&format!("  skipped: {reason}\n"));
            }
            if let Some(error) = task.error.as_ref() {
                output.push_str(&format!("  error: {error}\n"));
            }
        }
    }
    output
}

fn format_optional_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.1}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_optional_float(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn compact_single_line(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<&str>>().join(" ");
    truncate_eval_output(&compact, 512)
}

fn truncate_eval_output(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = 0;
    for (idx, _) in value.char_indices() {
        if idx > max_bytes {
            break;
        }
        end = idx;
    }
    format!("{}...[truncated]", &value[..end])
}

fn new_eval_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("eval-{millis}")
}

async fn load_trajectory_config(config_overrides: &CliConfigOverrides) -> anyhow::Result<Config> {
    let cli_overrides = config_overrides
        .clone()
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    Config::load_with_cli_overrides(cli_overrides)
        .await
        .map_err(Into::into)
}

fn trajectory_root_from_config(config: &Config) -> PathBuf {
    config
        .trajectory
        .root_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_trajectory_root(config.deepseek_vibing_home.as_ref()))
}

fn render_trajectory_summary(summary: &TrajectorySummary) -> String {
    let mut output = String::new();
    output.push_str(&format!("Trajectory: {}\n", summary.trajectory_id));
    output.push_str(&format!("Path: {}\n", summary.path.display()));
    output.push_str(&format!(
        "Provider/model: {}/{}\n",
        summary.provider, summary.model
    ));
    if let Some(effort) = summary.reasoning_effort.as_ref() {
        output.push_str(&format!("Reasoning effort: {effort}\n"));
    }
    output.push_str(&format!(
        "Sandbox: {:?}, network: {:?}\n",
        summary.sandbox_tier, summary.network
    ));
    output.push_str(&format!("Turns: {}\n", summary.turns.len()));
    for turn in &summary.turns {
        output.push_str(&format!(
            "- {} [{}], tools: {}\n",
            turn.turn_id,
            turn.status,
            turn.tool_calls.len()
        ));
        if let Some(usage) = turn.token_usage.as_ref() {
            output.push_str(&format!(
                "  usage input={} cached={} miss={} output={} total={}\n",
                usage.input_tokens,
                usage.cached_input_tokens,
                usage.non_cached_input_tokens,
                usage.output_tokens,
                usage.total_tokens
            ));
        }
        for tool in &turn.tool_calls {
            let exit = tool
                .exit_code
                .map(|code| format!(" exit={code}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "  tool {} {}{} call={}\n",
                tool.tool, tool.status, exit, tool.call_id
            ));
        }
    }
    output
}

fn render_trajectory_replay_report(report: &TrajectoryReplayReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("Replay report: {}\n", report.trajectory_id));
    output.push_str(&format!("Path: {}\n", report.path.display()));
    output.push_str(&format!("Turns: {}\n", report.turn_count));
    output.push_str(&format!("Failure turns: {}\n", report.failure_turns.len()));
    output.push_str(&format!(
        "Non-zero exits: {}\n",
        report.non_zero_exits.len()
    ));
    output.push_str(&format!(
        "Patch failures: {}\n",
        report.patch_failures.len()
    ));
    output.push_str(&format!(
        "Approval denials: {}\n",
        report.approval_denials.len()
    ));
    if let Some(diff) = report.final_diff.as_ref() {
        output.push_str(&format!(
            "Final diff bytes: {}{}\n",
            diff.original_bytes,
            if diff.truncated { " (truncated)" } else { "" }
        ));
    }
    if let Some(raw_root) = report.raw_trace_root.as_ref() {
        output.push_str(&format!("Raw trace root: {}\n", raw_root.display()));
    }
    if let Some(reduced) = report.raw_reduced_state.as_ref() {
        output.push_str(&format!("Reduced raw state: {}\n", reduced.display()));
    }
    output
}

fn write_json_file(path: &Path, value: &impl serde::Serialize) -> anyhow::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, format!("{json}\n"))?;
    Ok(())
}

fn build_fim_request_from_command(
    config: &Config,
    command: FimCommand,
    prefix: String,
    suffix: Option<String>,
) -> anyhow::Result<(
    deepseek_vibing_core::config::DeepSeekFimConfig,
    FimCompletionRequest,
)> {
    let mut fim_config = config.deepseek.fim.clone();
    if let Some(model) = command.model {
        fim_config.model = model;
    }
    if let Some(base_url) = command.base_url {
        fim_config.base_url = base_url;
    }
    if let Some(temperature) = command.temperature {
        fim_config.temperature = temperature;
    }
    if let Some(max_tokens) = command.max_tokens {
        fim_config.max_tokens = max_tokens;
    }
    validate_fim_max_tokens_value(fim_config.max_tokens)?;

    let request = FimCompletionRequest {
        prefix,
        suffix,
        model: fim_config.model.clone(),
        max_tokens: fim_config.max_tokens,
        temperature: fim_config.temperature,
        stop: None,
    };
    Ok((fim_config, request))
}

fn validate_fim_command_inputs(command: &FimCommand) -> anyhow::Result<()> {
    if is_stdin_path(&command.prefix_file)
        && command
            .suffix_file
            .as_ref()
            .is_some_and(|path| is_stdin_path(path))
    {
        anyhow::bail!("--prefix-file and --suffix-file cannot both be '-'");
    }
    Ok(())
}

fn read_fim_file_arg(path: &PathBuf, label: &str) -> anyhow::Result<String> {
    if is_stdin_path(path) {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| anyhow::anyhow!("failed to read {label} from stdin: {err}"))?;
        return Ok(input);
    }

    std::fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("failed to read {label} file `{}`: {err}", path.display()))
}

fn render_fim_output(output: &FimCompletionOutput, json: bool) -> anyhow::Result<String> {
    if json {
        serde_json::to_string_pretty(output).map_err(Into::into)
    } else {
        Ok(output.text.clone())
    }
}

fn parse_fim_max_tokens(raw: &str) -> Result<u16, String> {
    let value: u16 = raw
        .parse()
        .map_err(|_| format!("max tokens must be between 1 and {DEEPSEEK_FIM_MAX_TOKENS_LIMIT}"))?;
    validate_fim_max_tokens_value(value).map_err(|err| err.to_string())?;
    Ok(value)
}

fn validate_fim_max_tokens_value(value: u16) -> anyhow::Result<()> {
    if value == 0 || value > DEEPSEEK_FIM_MAX_TOKENS_LIMIT {
        anyhow::bail!(
            "max tokens must be between 1 and {DEEPSEEK_FIM_MAX_TOKENS_LIMIT}; got {value}"
        );
    }
    Ok(())
}

fn is_stdin_path(path: &PathBuf) -> bool {
    path == &PathBuf::from("-")
}

fn print_completion(cmd: CompletionCommand) {
    let mut app = MultitoolCli::command();
    let name = "deepseek-vibing";
    generate(cmd.shell, &mut app, name, &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use deepseek_vibing_protocol::ThreadId;
    use deepseek_vibing_tui::TokenUsage;
    use pretty_assertions::assert_eq;

    #[test]
    fn exec_server_remote_auth_accepts_api_key_auth() {
        let auth = DeepSeekVibingAuth::from_api_key("sk-test");

        assert!(is_supported_exec_server_remote_auth(&auth));
    }

    fn fim_from_args(args: &[&str]) -> FimCommand {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Some(Subcommand::Fim(command)) = cli.subcommand else {
            panic!("expected fim subcommand");
        };
        command
    }

    fn trajectory_from_args(args: &[&str]) -> TrajectoryCommand {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Some(Subcommand::Trajectory(command)) = cli.subcommand else {
            panic!("expected trajectory subcommand");
        };
        command
    }

    fn eval_from_args(args: &[&str]) -> EvalCommand {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Some(Subcommand::Eval(command)) = cli.subcommand else {
            panic!("expected eval subcommand");
        };
        command
    }

    #[test]
    fn fim_requires_prefix_file() {
        let err = MultitoolCli::try_parse_from(["deepseek-vibing", "fim"])
            .expect_err("prefix-file is required");

        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn fim_parses_prefix_suffix_and_overrides() {
        let command = fim_from_args(&[
            "deepseek-vibing",
            "fim",
            "--prefix-file",
            "prefix.rs",
            "--suffix-file",
            "suffix.rs",
            "--model",
            "deepseek-v4-pro",
            "--base-url",
            "https://api.deepseek.com/beta",
            "--max-tokens",
            "256",
            "--temperature",
            "0.3",
            "--json",
        ]);

        assert_eq!(command.prefix_file, PathBuf::from("prefix.rs"));
        assert_eq!(command.suffix_file, Some(PathBuf::from("suffix.rs")));
        assert_eq!(command.model.as_deref(), Some("deepseek-v4-pro"));
        assert_eq!(
            command.base_url.as_deref(),
            Some("https://api.deepseek.com/beta")
        );
        assert_eq!(command.max_tokens, Some(256));
        assert_eq!(command.temperature, Some(0.3));
        assert!(command.json);
    }

    #[test]
    fn fim_rejects_prefix_and_suffix_stdin_together() {
        let command = fim_from_args(&[
            "deepseek-vibing",
            "fim",
            "--prefix-file",
            "-",
            "--suffix-file",
            "-",
        ]);
        let err = validate_fim_command_inputs(&command).expect_err("dual stdin should fail");

        assert_eq!(
            err.to_string(),
            "--prefix-file and --suffix-file cannot both be '-'"
        );
    }

    #[test]
    fn fim_max_tokens_parser_rejects_out_of_range() {
        assert!(parse_fim_max_tokens("1").is_ok());
        assert!(parse_fim_max_tokens("4096").is_ok());
        assert!(parse_fim_max_tokens("0").is_err());
        assert!(parse_fim_max_tokens("4097").is_err());
    }

    #[test]
    fn fim_text_output_is_completion_only() {
        let output = FimCompletionOutput {
            text: "middle".to_string(),
            model: "deepseek-v4-pro".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: None,
        };

        assert_eq!(
            render_fim_output(&output, /*json*/ false).expect("render text"),
            "middle"
        );
    }

    #[test]
    fn fim_json_output_is_structured() {
        let output = FimCompletionOutput {
            text: "middle".to_string(),
            model: "deepseek-v4-pro".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: None,
        };
        let rendered = render_fim_output(&output, /*json*/ true).expect("render json");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");

        assert_eq!(value["text"], "middle");
        assert_eq!(value["model"], "deepseek-v4-pro");
        assert_eq!(value["finish_reason"], "stop");
    }

    #[test]
    fn trajectory_show_parses_json_flag() {
        let command =
            trajectory_from_args(&["deepseek-vibing", "trajectory", "show", "abc123", "--json"]);
        let TrajectorySubcommand::Show(show) = command.subcommand else {
            panic!("expected trajectory show");
        };

        assert_eq!(show.selector, "abc123");
        assert!(show.json);
    }

    #[test]
    fn trajectory_replay_parses_output_file() {
        let command = trajectory_from_args(&[
            "deepseek-vibing",
            "trajectory",
            "replay",
            "abc123",
            "--output",
            "report.json",
        ]);
        let TrajectorySubcommand::Replay(replay) = command.subcommand else {
            panic!("expected trajectory replay");
        };

        assert_eq!(replay.selector, "abc123");
        assert_eq!(replay.output, Some(PathBuf::from("report.json")));
    }

    #[test]
    fn eval_list_parses_dataset_and_json() {
        let command = eval_from_args(&[
            "deepseek-vibing",
            "eval",
            "list",
            "--dataset",
            "evals/smoke",
            "--json",
        ]);
        let EvalSubcommand::List(list) = command.subcommand else {
            panic!("expected eval list");
        };

        assert_eq!(list.dataset, Some(PathBuf::from("evals/smoke")));
        assert!(list.json);
    }

    #[test]
    fn eval_run_parses_mode_task_baseline_and_gate() {
        let command = eval_from_args(&[
            "deepseek-vibing",
            "eval",
            "run",
            "--dataset",
            "evals/smoke",
            "--task",
            "rust_add_function",
            "--mode",
            "offline",
            "--report",
            "report.json",
            "--baseline",
            "baseline.json",
            "--enforce",
            "--json",
            "--keep-workspaces",
        ]);
        let EvalSubcommand::Run(run) = command.subcommand else {
            panic!("expected eval run");
        };

        assert_eq!(run.dataset, Some(PathBuf::from("evals/smoke")));
        assert_eq!(run.task.as_deref(), Some("rust_add_function"));
        assert!(matches!(run.mode, Some(EvalModeArg::Offline)));
        assert_eq!(run.report, Some(PathBuf::from("report.json")));
        assert_eq!(run.baseline, Some(PathBuf::from("baseline.json")));
        assert!(run.enforce);
        assert!(run.json);
        assert!(run.keep_workspaces);
    }

    #[test]
    fn eval_summarize_trajectories_parses_flags() {
        let command = eval_from_args(&[
            "deepseek-vibing",
            "eval",
            "summarize-trajectories",
            "--trajectory-root",
            "traj",
            "--report",
            "report.json",
            "--json",
            "--enforce",
        ]);
        let EvalSubcommand::SummarizeTrajectories(summarize) = command.subcommand else {
            panic!("expected summarize-trajectories");
        };

        assert_eq!(summarize.trajectory_root, Some(PathBuf::from("traj")));
        assert_eq!(summarize.report, Some(PathBuf::from("report.json")));
        assert!(summarize.json);
        assert!(summarize.enforce);
    }

    #[test]
    fn eval_exec_args_put_root_options_before_exec() {
        let args = build_eval_exec_args(
            Path::new("/tmp/workspace"),
            Path::new("/tmp/trajectories"),
            "fix the tests",
        );
        let exec_index = args.iter().position(|arg| arg == "exec").expect("exec arg");
        let approval_index = args
            .iter()
            .position(|arg| arg == "--ask-for-approval")
            .expect("approval arg");

        assert!(approval_index < exec_index);
        assert!(args.contains(&"trajectory.enabled=\"always\"".to_string()));
        assert!(args.contains(&"trajectory.detail=\"summary\"".to_string()));
        assert!(
            args.iter()
                .any(|arg| arg.starts_with("trajectory.root_dir="))
        );
        assert!(args[exec_index + 1..].contains(&"--json".to_string()));
        assert!(args[exec_index + 1..].contains(&"--skip-git-repo-check".to_string()));
        assert!(args[exec_index + 1..].contains(&"--sandbox".to_string()));
        assert!(args[exec_index + 1..].contains(&"workspace-write".to_string()));
        assert!(args[exec_index + 1..].contains(&"--cd".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("fix the tests"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn eval_process_capture_runs_fake_executable() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let script = temp.path().join("fake-deepseek-vibing");
        let args_file = temp.path().join("args.txt");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nprintf 'ok\\n'\n",
                args_file.display()
            ),
        )
        .expect("write fake executable");
        let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod");

        let output = run_eval_process_capture_with_executable(
            &script,
            &["exec", "--json", "prompt"],
            temp.path(),
            1000,
        )
        .await
        .expect("run fake executable");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "ok\n");
        assert_eq!(
            std::fs::read_to_string(args_file).expect("args file"),
            "exec\n--json\nprompt\n"
        );
    }

    #[test]
    fn exec_server_remote_api_key_auth_accepts_https_openai_domains() {
        for base_url in [
            "https://openai.com/api",
            "https://service.openai.com/api",
            "https://openai.org/api",
            "https://service.openai.org/api",
        ] {
            assert!(validate_api_key_remote_host(base_url).is_ok());
        }
    }

    #[test]
    fn exec_server_remote_api_key_auth_accepts_http_loopback() {
        for base_url in [
            "http://localhost:8098/api",
            "http://127.0.0.1:8098/api",
            "http://[::1]:8098/api",
        ] {
            assert!(validate_api_key_remote_host(base_url).is_ok());
        }
    }

    #[test]
    fn exec_server_remote_api_key_auth_rejects_http_openai_domain() {
        for base_url in [
            "http://service.openai.com/api",
            "http://service.openai.org/api",
        ] {
            let error = validate_api_key_remote_host(base_url)
                .expect_err("reject plaintext OpenAI destination");

            assert_eq!(
                error.to_string(),
                "remote exec-server API-key authentication is restricted to HTTPS openai.com and openai.org hosts and subdomains or loopback hosts"
            );
        }
    }

    #[test]
    fn exec_server_remote_api_key_auth_rejects_suffix_spoof() {
        let error = validate_api_key_remote_host("https://service.openai.org.evil.example/api")
            .expect_err("reject suffix spoof");

        assert_eq!(
            error.to_string(),
            "remote exec-server API-key authentication is restricted to HTTPS openai.com and openai.org hosts and subdomains or loopback hosts"
        );
    }

    fn finalize_resume_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            all,
            include_non_interactive,
            remote: _,
            config_overrides: resume_cli,
        }) = subcommand.expect("resume present")
        else {
            unreachable!()
        };

        finalize_resume_interactive(
            interactive,
            root_overrides,
            session_id,
            last,
            all,
            include_non_interactive,
            resume_cli,
        )
    }

    fn finalize_fork_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Fork(ForkCommand {
            session_id,
            last,
            all,
            remote: _,
            config_overrides: fork_cli,
        }) = subcommand.expect("fork present")
        else {
            unreachable!()
        };

        finalize_fork_interactive(interactive, root_overrides, session_id, last, all, fork_cli)
    }

    fn finalize_archive_from_args(args: &[&str]) -> (String, TuiCli, InteractiveRemoteOptions) {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
            feature_toggles: _,
            remote: _,
        } = cli;

        let Subcommand::Archive(SessionArchiveCommand {
            target,
            remote,
            config_overrides: archive_cli,
        }) = subcommand.expect("archive present")
        else {
            unreachable!()
        };

        (
            target,
            finalize_session_archive_interactive(interactive, root_overrides, archive_cli),
            remote,
        )
    }

    fn profile_v2_for_args(args: &[&str]) -> anyhow::Result<Option<String>> {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Some(subcommand) = cli.subcommand.as_ref() else {
            return Ok(cli
                .interactive
                .config_profile_v2
                .as_ref()
                .map(std::string::ToString::to_string));
        };
        Ok(profile_v2_for_subcommand(&cli.interactive, subcommand)?.map(ToString::to_string))
    }

    #[test]
    fn profile_v2_is_rejected_for_config_management_subcommands() {
        assert!(
            profile_v2_for_args(&["deepseek-vibing", "--profile", "work", "features", "list"])
                .is_err()
        );
    }

    #[test]
    fn profile_v2_is_allowed_for_runtime_subcommands() {
        assert_eq!(
            profile_v2_for_args(&["deepseek-vibing", "--profile", "work", "resume"])
                .expect("resume supports profile-v2")
                .as_deref(),
            Some("work")
        );
        assert_eq!(
            profile_v2_for_args(&[
                "deepseek-vibing",
                "--profile",
                "work",
                "debug",
                "prompt-input"
            ])
            .expect("debug prompt-input supports profile-v2")
            .as_deref(),
            Some("work")
        );
        assert_eq!(
            profile_v2_for_args(&["deepseek-vibing", "--profile", "work", "mcp", "list"])
                .expect("mcp supports profile-v2")
                .as_deref(),
            Some("work")
        );
        assert_eq!(
            profile_v2_for_args(&["deepseek-vibing", "--profile", "work", "sandbox"])
                .expect("sandbox supports config profile")
                .as_deref(),
            Some("work")
        );
    }

    #[test]
    fn profile_v2_rejects_non_plain_names_at_parse_time() {
        assert!(
            MultitoolCli::try_parse_from(["deepseek-vibing", "--profile", "nested/work", "resume"])
                .is_err()
        );
    }

    #[test]
    fn exec_resume_last_accepts_prompt_positional() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "exec",
            "--json",
            "resume",
            "--last",
            "2+2",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };
        let Some(deepseek_vibing_exec::Command::Resume(args)) = exec.command else {
            panic!("expected exec resume");
        };

        assert!(args.last);
        assert_eq!(args.session_id, None);
        assert_eq!(args.prompt.as_deref(), Some("2+2"));
    }

    #[test]
    fn exec_resume_accepts_output_flags_after_subcommand() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "exec",
            "resume",
            "session-123",
            "-o",
            "/tmp/resume-output.md",
            "--output-schema",
            "/tmp/schema.json",
            "re-review",
        ])
        .expect("parse should succeed");

        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };
        let Some(deepseek_vibing_exec::Command::Resume(args)) = exec.command else {
            panic!("expected exec resume");
        };

        assert_eq!(
            exec.last_message_file,
            Some(std::path::PathBuf::from("/tmp/resume-output.md"))
        );
        assert_eq!(
            exec.output_schema,
            Some(std::path::PathBuf::from("/tmp/schema.json"))
        );
        assert_eq!(args.session_id.as_deref(), Some("session-123"));
        assert_eq!(args.prompt.as_deref(), Some("re-review"));
    }

    #[test]
    fn dangerous_bypass_conflicts_with_approval_policy() {
        let err = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "--dangerously-bypass-approvals-and-sandbox",
            "--ask-for-approval",
            "on-request",
        ])
        .expect_err("conflicting permission flags should be rejected");

        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    fn app_server_from_args(args: &[&str]) -> AppServerCommand {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let Subcommand::AppServer(app_server) = cli.subcommand.expect("app-server present") else {
            unreachable!()
        };
        app_server
    }

    fn default_app_server_socket_path() -> AbsolutePathBuf {
        let deepseek_vibing_home = find_deepseek_vibing_home().expect("codex home");
        deepseek_vibing_app_server::app_server_control_socket_path(&deepseek_vibing_home)
            .expect("default app-server socket path")
    }

    #[test]
    fn debug_prompt_input_parses_prompt_and_images() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "debug",
            "prompt-input",
            "hello",
            "--image",
            "/tmp/a.png,/tmp/b.png",
        ])
        .expect("parse");

        let Some(Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::PromptInput(cmd),
        })) = cli.subcommand
        else {
            panic!("expected debug prompt-input subcommand");
        };

        assert_eq!(cmd.prompt.as_deref(), Some("hello"));
        assert_eq!(
            cmd.images,
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]
        );
    }

    #[test]
    fn debug_models_parses_bundled_flag() {
        let cli = MultitoolCli::try_parse_from(["deepseek-vibing", "debug", "models", "--bundled"])
            .expect("parse");

        let Some(Subcommand::Debug(DebugCommand {
            subcommand: DebugSubcommand::Models(cmd),
        })) = cli.subcommand
        else {
            panic!("expected debug models subcommand");
        };

        assert!(cmd.bundled);
    }

    #[test]
    fn responses_subcommand_is_not_registered() {
        let command = MultitoolCli::command();
        assert!(
            command
                .get_subcommands()
                .all(|subcommand| subcommand.get_name() != "responses")
        );
    }

    fn help_from_args(args: &[&str]) -> String {
        let err = MultitoolCli::try_parse_from(args).expect_err("help should short-circuit");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
        err.to_string()
    }

    #[test]
    fn plugin_marketplace_help_uses_plugin_namespace() {
        let help = help_from_args(&["deepseek-vibing", "plugin", "marketplace", "--help"]);
        assert!(
            help.contains("Usage: deepseek-vibing plugin marketplace [OPTIONS] <COMMAND>"),
            "{help}"
        );

        for (subcommand, usage) in [
            ("add", "Usage: deepseek-vibing plugin marketplace add"),
            ("list", "Usage: deepseek-vibing plugin marketplace list"),
            (
                "upgrade",
                "Usage: deepseek-vibing plugin marketplace upgrade",
            ),
            ("remove", "Usage: deepseek-vibing plugin marketplace remove"),
        ] {
            let help = help_from_args(&[
                "deepseek-vibing",
                "plugin",
                "marketplace",
                subcommand,
                "--help",
            ]);
            assert!(help.contains(usage), "{help}");
        }
    }

    #[test]
    fn plugin_marketplace_add_parses_under_plugin() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "plugin",
            "marketplace",
            "add",
            "owner/repo",
        ])
        .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_marketplace_upgrade_parses_under_plugin() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "plugin",
            "marketplace",
            "upgrade",
            "debug",
        ])
        .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_add_parses_under_plugin() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "plugin",
            "add",
            "sample",
            "--marketplace",
            "debug",
        ])
        .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_list_parses_under_plugin() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "plugin",
            "list",
            "--marketplace",
            "debug",
        ])
        .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn plugin_remove_parses_under_plugin() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "plugin",
            "remove",
            "sample",
            "--marketplace",
            "debug",
        ])
        .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn update_parses_as_update_subcommand() {
        let cli = MultitoolCli::try_parse_from(["deepseek-vibing", "update"]).expect("parse");
        assert!(matches!(cli.subcommand, Some(Subcommand::Update)));
    }

    #[test]
    fn archive_merges_scoped_tui_flags() {
        let (target, interactive, remote) = finalize_archive_from_args(
            [
                "deepseek-vibing",
                "-C",
                "/root",
                "archive",
                "--remote",
                "unix://archive.sock",
                "--strict-config",
                "--dangerously-bypass-hook-trust",
                "-m",
                "gpt-5.1-test",
                "-p",
                "work",
                "-C",
                "/archive",
                "my-thread",
            ]
            .as_ref(),
        );

        assert_eq!(target, "my-thread");
        assert_eq!(remote.remote.as_deref(), Some("unix://archive.sock"));
        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert_eq!(interactive.config_profile_v2.as_deref(), Some("work"));
        assert_eq!(
            interactive.cwd.as_deref(),
            Some(std::path::Path::new("/archive"))
        );
        assert!(interactive.strict_config);
        assert!(interactive.bypass_hook_trust);
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    #[test]
    fn sandbox_parses_permissions_profile() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "sandbox",
            "--permissions-profile",
            ":workspace",
            "--",
            "echo",
        ])
        .expect("parse");

        let Some(Subcommand::Sandbox(command)) = cli.subcommand else {
            panic!("expected sandbox command");
        };

        assert_eq!(command.permissions_profile.as_deref(), Some(":workspace"));
        assert_eq!(command.command, vec!["echo"]);
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    #[test]
    fn sandbox_parses_config_profile() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "sandbox",
            "--profile",
            "work",
            "--",
            "echo",
        ])
        .expect("parse");

        let Some(Subcommand::Sandbox(command)) = cli.subcommand else {
            panic!("expected sandbox command");
        };

        assert_eq!(command.config_profile.as_deref(), Some("work"));
        assert_eq!(command.command, vec!["echo"]);
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    #[test]
    fn sandbox_rejects_explicit_profile_controls_without_profile() {
        let err = MultitoolCli::try_parse_from(["deepseek-vibing", "sandbox", "-C", "/tmp"])
            .expect_err("parse should fail");

        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn plugin_marketplace_remove_parses_under_plugin() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "plugin",
            "marketplace",
            "remove",
            "debug",
        ])
        .expect("parse");

        assert!(matches!(cli.subcommand, Some(Subcommand::Plugin(_))));
    }

    #[test]
    fn marketplace_no_longer_parses_at_top_level() {
        let add_result =
            MultitoolCli::try_parse_from(["deepseek-vibing", "marketplace", "add", "owner/repo"]);
        assert!(add_result.is_err());

        let upgrade_result =
            MultitoolCli::try_parse_from(["deepseek-vibing", "marketplace", "upgrade", "debug"]);
        assert!(upgrade_result.is_err());

        let remove_result =
            MultitoolCli::try_parse_from(["deepseek-vibing", "marketplace", "remove", "debug"]);
        assert!(remove_result.is_err());
    }

    #[test]
    fn full_auto_no_longer_parses_at_top_level() {
        let result = MultitoolCli::try_parse_from(["deepseek-vibing", "--full-auto"]);

        assert!(result.is_err());
    }

    #[test]
    fn exec_full_auto_reports_migration_path() {
        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "exec", "--full-auto", "summarize"])
                .expect("exec should accept removed flag long enough to report a migration path");
        let Some(Subcommand::Exec(exec)) = cli.subcommand else {
            panic!("expected exec subcommand");
        };

        assert_eq!(
            exec.removed_full_auto_warning(),
            Some("warning: `--full-auto` is deprecated; use `--sandbox workspace-write` instead.")
        );
    }

    #[test]
    fn sandbox_full_auto_no_longer_parses() {
        let result =
            MultitoolCli::try_parse_from(["deepseek-vibing", "sandbox", "--full-auto", "--"]);

        assert!(result.is_err());
    }

    fn sample_exit_info(conversation_id: Option<&str>, thread_name: Option<&str>) -> AppExitInfo {
        let token_usage = TokenUsage {
            output_tokens: 2,
            total_tokens: 2,
            ..Default::default()
        };
        AppExitInfo {
            token_usage,
            thread_id: conversation_id
                .map(ThreadId::from_string)
                .map(Result::unwrap),
            thread_name: thread_name.map(str::to_string),
            update_action: None,
            exit_reason: ExitReason::UserRequested,
        }
    }

    #[test]
    fn format_exit_messages_skips_zero_usage() {
        let exit_info = AppExitInfo {
            token_usage: TokenUsage::default(),
            thread_id: None,
            thread_name: None,
            update_action: None,
            exit_reason: ExitReason::UserRequested,
        };
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_exit_messages_includes_resume_hint_without_color() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            /*thread_name*/ None,
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert_eq!(
            lines,
            vec![
                "Token usage: total=2 input=0 output=2".to_string(),
                "To continue this session, run deepseek-vibing resume 123e4567-e89b-12d3-a456-426614174000"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn format_exit_messages_applies_color_when_enabled() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            /*thread_name*/ None,
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ true);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("\u{1b}[36m"));
    }

    #[test]
    fn format_exit_messages_names_picker_item_when_thread_has_name() {
        let exit_info = sample_exit_info(
            Some("123e4567-e89b-12d3-a456-426614174000"),
            Some("my-thread"),
        );
        let lines = format_exit_messages(exit_info, /*color_enabled*/ false);
        assert_eq!(
            lines,
            vec![
                "Token usage: total=2 input=0 output=2".to_string(),
                "To continue this session, run deepseek-vibing resume, then select my-thread (123e4567-e89b-12d3-a456-426614174000)".to_string(),
            ]
        );
    }

    #[test]
    fn resume_model_flag_applies_when_no_root_flags() {
        let interactive =
            finalize_resume_from_args(["deepseek-vibing", "resume", "-m", "gpt-5.1-test"].as_ref());

        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_picker_logic_none_and_not_last() {
        let interactive = finalize_resume_from_args(["deepseek-vibing", "resume"].as_ref());
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_picker_logic_last() {
        let interactive =
            finalize_resume_from_args(["deepseek-vibing", "resume", "--last"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_picker_logic_with_session_id() {
        let interactive = finalize_resume_from_args(["deepseek-vibing", "resume", "1234"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("1234"));
        assert!(!interactive.resume_show_all);
    }

    #[test]
    fn resume_all_flag_sets_show_all() {
        let interactive =
            finalize_resume_from_args(["deepseek-vibing", "resume", "--all"].as_ref());
        assert!(interactive.resume_picker);
        assert!(interactive.resume_show_all);
    }

    #[test]
    fn resume_include_non_interactive_flag_sets_source_filter_override() {
        let interactive = finalize_resume_from_args(
            ["deepseek-vibing", "resume", "--include-non-interactive"].as_ref(),
        );

        assert!(interactive.resume_picker);
        assert!(interactive.resume_include_non_interactive);
    }

    #[test]
    fn resume_merges_option_flags() {
        let interactive = finalize_resume_from_args(
            [
                "deepseek-vibing",
                "resume",
                "sid",
                "--oss",
                "--search",
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request",
                "-m",
                "gpt-5.1-test",
                "-p",
                "my-config",
                "-C",
                "/tmp",
                "--strict-config",
                "-i",
                "/tmp/a.png,/tmp/b.png",
            ]
            .as_ref(),
        );

        assert_eq!(interactive.model.as_deref(), Some("gpt-5.1-test"));
        assert!(interactive.oss);
        assert_eq!(interactive.config_profile_v2.as_deref(), Some("my-config"));
        assert_matches!(
            interactive.sandbox_mode,
            Some(deepseek_vibing_utils_cli::SandboxModeCliArg::WorkspaceWrite)
        );
        assert_matches!(
            interactive.approval_policy,
            Some(deepseek_vibing_utils_cli::ApprovalModeCliArg::OnRequest)
        );
        assert_eq!(
            interactive.cwd.as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert!(interactive.web_search);
        assert!(interactive.strict_config);
        let has_a = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/a.png"));
        let has_b = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/b.png"));
        assert!(has_a && has_b);
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("sid"));
    }

    #[test]
    fn resume_merges_dangerously_bypass_flag() {
        let interactive = finalize_resume_from_args(
            [
                "deepseek-vibing",
                "resume",
                "--dangerously-bypass-approvals-and-sandbox",
            ]
            .as_ref(),
        );
        assert!(interactive.dangerously_bypass_approvals_and_sandbox);
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_merges_bypass_hook_trust_flag() {
        let interactive = finalize_resume_from_args(
            [
                "deepseek-vibing",
                "resume",
                "--dangerously-bypass-hook-trust",
            ]
            .as_ref(),
        );

        assert!(interactive.bypass_hook_trust);
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn fork_picker_logic_none_and_not_last() {
        let interactive = finalize_fork_from_args(["deepseek-vibing", "fork"].as_ref());
        assert!(interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id, None);
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_picker_logic_last() {
        let interactive = finalize_fork_from_args(["deepseek-vibing", "fork", "--last"].as_ref());
        assert!(!interactive.fork_picker);
        assert!(interactive.fork_last);
        assert_eq!(interactive.fork_session_id, None);
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_picker_logic_with_session_id() {
        let interactive = finalize_fork_from_args(["deepseek-vibing", "fork", "1234"].as_ref());
        assert!(!interactive.fork_picker);
        assert!(!interactive.fork_last);
        assert_eq!(interactive.fork_session_id.as_deref(), Some("1234"));
        assert!(!interactive.fork_show_all);
    }

    #[test]
    fn fork_all_flag_sets_show_all() {
        let interactive = finalize_fork_from_args(["deepseek-vibing", "fork", "--all"].as_ref());
        assert!(interactive.fork_picker);
        assert!(interactive.fork_show_all);
    }

    #[test]
    fn app_server_analytics_default_disabled_without_flag() {
        let app_server = app_server_from_args(["deepseek-vibing", "app-server"].as_ref());
        assert!(!app_server.analytics_default_enabled);
        assert!(!app_server.remote_control);
        assert_eq!(
            app_server.listen,
            deepseek_vibing_app_server::AppServerTransport::Stdio
        );
    }

    #[test]
    fn app_server_analytics_default_enabled_with_flag() {
        let app_server = app_server_from_args(
            [
                "deepseek-vibing",
                "app-server",
                "--analytics-default-enabled",
            ]
            .as_ref(),
        );
        assert!(app_server.analytics_default_enabled);
    }

    #[test]
    fn strict_config_parses_for_supported_commands() {
        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "--strict-config"]).expect("parse");
        assert!(cli.interactive.strict_config);

        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "mcp-server", "--strict-config"])
                .expect("parse");
        assert_matches!(
            cli.subcommand,
            Some(Subcommand::McpServer(McpServerCommand {
                strict_config: true,
            }))
        );

        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "review",
            "--strict-config",
            "--uncommitted",
        ])
        .expect("parse");
        assert_matches!(
            cli.subcommand,
            Some(Subcommand::Review(ReviewCommand {
                strict_config: true,
                ..
            }))
        );

        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "exec-server", "--strict-config"])
                .expect("parse");
        assert_matches!(
            cli.subcommand,
            Some(Subcommand::ExecServer(ExecServerCommand {
                strict_config: true,
                ..
            }))
        );
    }

    #[test]
    fn root_strict_config_is_supported_for_exec_server() {
        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "--strict-config", "exec-server"])
                .expect("parse");

        reject_root_strict_config_for_subcommand(cli.interactive.strict_config, &cli.subcommand)
            .expect("exec-server should support root --strict-config");
    }

    #[test]
    fn root_strict_config_is_rejected_for_unsupported_subcommands() {
        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "--strict-config", "mcp", "list"])
                .expect("parse");
        let err = reject_root_strict_config_for_subcommand(
            cli.interactive.strict_config,
            &cli.subcommand,
        )
        .expect_err("mcp should not support root --strict-config");

        assert_eq!(
            err.to_string(),
            "`--strict-config` is not supported for `deepseek-vibing mcp`"
        );

        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "--strict-config", "remote-control"])
                .expect("parse");
        let err = reject_root_strict_config_for_subcommand(
            cli.interactive.strict_config,
            &cli.subcommand,
        )
        .expect_err("remote-control should not support root --strict-config");

        assert_eq!(
            err.to_string(),
            "`--strict-config` is not supported for `deepseek-vibing remote-control`"
        );
    }

    #[test]
    fn app_server_subcommands_reject_strict_config() {
        let app_server = app_server_from_args(
            ["deepseek-vibing", "app-server", "--strict-config", "proxy"].as_ref(),
        );
        let err = reject_strict_config_for_app_server_subcommand(
            app_server.strict_config,
            app_server.subcommand.as_ref(),
        )
        .expect_err("app-server proxy should not support --strict-config");

        assert_eq!(
            err.to_string(),
            "`--strict-config` is not supported for `deepseek-vibing app-server proxy`"
        );
    }

    #[test]
    fn reject_remote_flag_for_remote_control() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "--remote",
            "unix://",
            "remote-control",
        ])
        .expect("parse");
        let Some(Subcommand::RemoteControl(remote_control)) = &cli.subcommand else {
            panic!("expected remote-control subcommand");
        };
        assert_eq!(remote_control.subcommand_name(), "remote-control");

        let err = reject_remote_mode_for_subcommand(
            cli.remote.remote.as_deref(),
            cli.remote.remote_auth_token_env.as_deref(),
            "remote-control",
        )
        .expect_err("remote-control should reject root --remote");

        assert!(err.to_string().contains("remote-control"));
    }

    #[test]
    fn remote_flag_parses_for_interactive_root() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "--remote",
            "unix://deepseek-vibing.sock",
        ])
        .expect("parse");
        assert_eq!(
            cli.remote.remote.as_deref(),
            Some("unix://deepseek-vibing.sock")
        );
    }

    #[test]
    fn remote_auth_token_env_flag_parses_for_interactive_root() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "--remote-auth-token-env",
            "DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN",
            "--remote",
            "ws://127.0.0.1:4500",
        ])
        .expect("parse");
        assert_eq!(
            cli.remote.remote_auth_token_env.as_deref(),
            Some("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN")
        );
    }

    #[test]
    fn remote_flag_parses_for_resume_subcommand() {
        let cli = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "resume",
            "--remote",
            "unix://deepseek-vibing.sock",
        ])
        .expect("parse");
        let Subcommand::Resume(ResumeCommand { remote, .. }) =
            cli.subcommand.expect("resume present")
        else {
            panic!("expected resume subcommand");
        };
        assert_eq!(
            remote.remote.as_deref(),
            Some("unix://deepseek-vibing.sock")
        );
    }

    #[test]
    fn reject_remote_mode_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            Some("127.0.0.1:4500"),
            /*remote_auth_token_env*/ None,
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote");
        assert!(
            err.to_string()
                .contains("only supported for interactive TUI commands")
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_non_interactive_subcommands() {
        let err = reject_remote_mode_for_subcommand(
            /*remote*/ None,
            Some("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN"),
            "exec",
        )
        .expect_err("non-interactive subcommands should reject --remote-auth-token-env");
        assert!(
            err.to_string()
                .contains("only supported for interactive TUI commands")
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_app_server_generate_internal_json_schema() {
        let subcommand =
            AppServerSubcommand::GenerateInternalJsonSchema(GenerateInternalJsonSchemaCommand {
                out_dir: PathBuf::from("/tmp/out"),
            });
        let err = reject_remote_mode_for_app_server_subcommand(
            /*remote*/ None,
            Some("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN"),
            Some(&subcommand),
        )
        .expect_err("non-interactive app-server subcommands should reject --remote-auth-token-env");
        assert!(err.to_string().contains("generate-internal-json-schema"));
    }

    #[test]
    fn read_remote_auth_token_from_env_var_reports_missing_values() {
        let err =
            read_remote_auth_token_from_env_var_with("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN", |_| {
                Err(std::env::VarError::NotPresent)
            })
            .expect_err("missing env vars should be rejected");
        assert!(err.to_string().contains("is not set"));
    }

    #[test]
    fn read_remote_auth_token_from_env_var_trims_values() {
        let auth_token =
            read_remote_auth_token_from_env_var_with("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN", |_| {
                Ok("  bearer-token  ".to_string())
            })
            .expect("env var should parse");
        assert_eq!(auth_token, "bearer-token");
    }

    #[test]
    fn read_remote_auth_token_from_env_var_rejects_empty_values() {
        let err =
            read_remote_auth_token_from_env_var_with("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN", |_| {
                Ok(" \n\t ".to_string())
            })
            .expect_err("empty env vars should be rejected");
        assert!(err.to_string().contains("is empty"));
    }

    #[test]
    fn app_server_listen_websocket_url_parses() {
        let app_server = app_server_from_args(
            [
                "deepseek-vibing",
                "app-server",
                "--listen",
                "ws://127.0.0.1:4500",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.listen,
            deepseek_vibing_app_server::AppServerTransport::WebSocket {
                bind_address: "127.0.0.1:4500".parse().expect("valid socket address"),
            }
        );
    }

    #[test]
    fn app_server_listen_stdio_url_parses() {
        let app_server = app_server_from_args(
            ["deepseek-vibing", "app-server", "--listen", "stdio://"].as_ref(),
        );
        assert_eq!(
            app_server.listen,
            deepseek_vibing_app_server::AppServerTransport::Stdio
        );
    }

    #[test]
    fn app_server_stdio_flag_parses() {
        let app_server =
            app_server_from_args(["deepseek-vibing", "app-server", "--stdio"].as_ref());
        assert!(app_server.stdio);
    }

    #[test]
    fn app_server_stdio_flag_conflicts_with_listen() {
        let err = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "app-server",
            "--stdio",
            "--listen",
            "stdio://",
        ])
        .expect_err("--stdio and --listen should be rejected together");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn app_server_listen_unix_socket_url_parses() {
        let app_server =
            app_server_from_args(["deepseek-vibing", "app-server", "--listen", "unix://"].as_ref());
        assert_eq!(
            app_server.listen,
            deepseek_vibing_app_server::AppServerTransport::UnixSocket {
                socket_path: default_app_server_socket_path()
            }
        );
    }

    #[test]
    fn app_server_listen_unix_socket_path_parses() {
        let app_server = app_server_from_args(
            [
                "deepseek-vibing",
                "app-server",
                "--listen",
                "unix:///tmp/deepseek-vibing.sock",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.listen,
            deepseek_vibing_app_server::AppServerTransport::UnixSocket {
                socket_path: AbsolutePathBuf::from_absolute_path("/tmp/deepseek-vibing.sock")
                    .expect("absolute path should parse")
            }
        );
    }

    #[test]
    fn app_server_listen_off_parses() {
        let app_server =
            app_server_from_args(["deepseek-vibing", "app-server", "--listen", "off"].as_ref());
        assert_eq!(
            app_server.listen,
            deepseek_vibing_app_server::AppServerTransport::Off
        );
    }

    #[test]
    fn app_server_listen_invalid_url_fails_to_parse() {
        let parse_result = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "app-server",
            "--listen",
            "http://foo",
        ]);
        assert!(parse_result.is_err());
    }

    #[test]
    fn app_server_proxy_subcommand_parses() {
        let app_server = app_server_from_args(["deepseek-vibing", "app-server", "proxy"].as_ref());
        assert!(matches!(
            app_server.subcommand,
            Some(AppServerSubcommand::Proxy(AppServerProxyCommand {
                socket_path: None
            }))
        ));
    }

    #[test]
    fn app_server_daemon_subcommands_parse() {
        assert!(matches!(
            app_server_from_args(
                [
                    "deepseek-vibing",
                    "app-server",
                    "daemon",
                    "bootstrap",
                    "--remote-control"
                ]
                .as_ref()
            )
            .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::Bootstrap(AppServerBootstrapCommand {
                    remote_control: true
                })
            }))
        ));
        assert!(matches!(
            app_server_from_args(["deepseek-vibing", "app-server", "daemon", "start"].as_ref())
                .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::Start
            }))
        ));
        assert!(matches!(
            app_server_from_args(["deepseek-vibing", "app-server", "daemon", "restart"].as_ref())
                .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::Restart
            }))
        ));
        assert!(matches!(
            app_server_from_args(
                [
                    "deepseek-vibing",
                    "app-server",
                    "daemon",
                    "enable-remote-control"
                ]
                .as_ref()
            )
            .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::EnableRemoteControl
            }))
        ));
        assert!(matches!(
            app_server_from_args(
                [
                    "deepseek-vibing",
                    "app-server",
                    "daemon",
                    "disable-remote-control"
                ]
                .as_ref()
            )
            .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::DisableRemoteControl
            }))
        ));
        assert!(matches!(
            app_server_from_args(["deepseek-vibing", "app-server", "daemon", "stop"].as_ref())
                .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::Stop
            }))
        ));
        assert!(matches!(
            app_server_from_args(["deepseek-vibing", "app-server", "daemon", "version"].as_ref())
                .subcommand,
            Some(AppServerSubcommand::Daemon(AppServerDaemonCommand {
                subcommand: AppServerDaemonSubcommand::Version
            }))
        ));
    }

    #[test]
    fn app_server_proxy_sock_path_parses() {
        let app_server = app_server_from_args(
            [
                "deepseek-vibing",
                "app-server",
                "proxy",
                "--sock",
                "deepseek-vibing.sock",
            ]
            .as_ref(),
        );
        let Some(AppServerSubcommand::Proxy(proxy)) = app_server.subcommand else {
            panic!("expected proxy subcommand");
        };
        assert_eq!(
            proxy.socket_path,
            Some(
                AbsolutePathBuf::relative_to_current_dir("deepseek-vibing.sock")
                    .expect("relative path should resolve")
            )
        );
    }

    #[test]
    fn reject_remote_auth_token_env_for_app_server_proxy() {
        let subcommand = AppServerSubcommand::Proxy(AppServerProxyCommand { socket_path: None });
        let err = reject_remote_mode_for_app_server_subcommand(
            /*remote*/ None,
            Some("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN"),
            Some(&subcommand),
        )
        .expect_err("app-server proxy should reject --remote-auth-token-env");
        assert!(err.to_string().contains("app-server proxy"));
    }

    #[test]
    fn reject_remote_auth_token_env_for_app_server_version() {
        let subcommand = AppServerSubcommand::Daemon(AppServerDaemonCommand {
            subcommand: AppServerDaemonSubcommand::Version,
        });
        let err = reject_remote_mode_for_app_server_subcommand(
            /*remote*/ None,
            Some("DEEPSEEK_VIBING_REMOTE_AUTH_TOKEN"),
            Some(&subcommand),
        )
        .expect_err("app-server daemon version should reject --remote-auth-token-env");
        assert!(err.to_string().contains("app-server daemon version"));
    }

    #[test]
    fn app_server_capability_token_flags_parse() {
        let app_server = app_server_from_args(
            [
                "deepseek-vibing",
                "app-server",
                "--ws-auth",
                "capability-token",
                "--ws-token-file",
                "/tmp/deepseek-vibing-token",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.auth.ws_auth,
            Some(deepseek_vibing_app_server::WebsocketAuthCliMode::CapabilityToken)
        );
        assert_eq!(
            app_server.auth.ws_token_file,
            Some(PathBuf::from("/tmp/deepseek-vibing-token"))
        );
    }

    #[test]
    fn app_server_signed_bearer_flags_parse() {
        let app_server = app_server_from_args(
            [
                "deepseek-vibing",
                "app-server",
                "--ws-auth",
                "signed-bearer-token",
                "--ws-shared-secret-file",
                "/tmp/deepseek-vibing-secret",
                "--ws-issuer",
                "issuer",
                "--ws-audience",
                "audience",
                "--ws-max-clock-skew-seconds",
                "9",
            ]
            .as_ref(),
        );
        assert_eq!(
            app_server.auth.ws_auth,
            Some(deepseek_vibing_app_server::WebsocketAuthCliMode::SignedBearerToken)
        );
        assert_eq!(
            app_server.auth.ws_shared_secret_file,
            Some(PathBuf::from("/tmp/deepseek-vibing-secret"))
        );
        assert_eq!(app_server.auth.ws_issuer.as_deref(), Some("issuer"));
        assert_eq!(app_server.auth.ws_audience.as_deref(), Some("audience"));
        assert_eq!(app_server.auth.ws_max_clock_skew_seconds, Some(9));
    }

    #[test]
    fn app_server_rejects_removed_insecure_non_loopback_flag() {
        let parse_result = MultitoolCli::try_parse_from([
            "deepseek-vibing",
            "app-server",
            "--allow-unauthenticated-non-loopback-ws",
        ]);
        assert!(parse_result.is_err());
    }

    #[test]
    fn features_enable_parses_feature_name() {
        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "features", "enable", "unified_exec"])
                .expect("parse should succeed");
        let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
            panic!("expected features subcommand");
        };
        let FeaturesSubcommand::Enable(FeatureSetArgs { feature }) = sub else {
            panic!("expected features enable");
        };
        assert_eq!(feature, "unified_exec");
    }

    #[test]
    fn features_disable_parses_feature_name() {
        let cli =
            MultitoolCli::try_parse_from(["deepseek-vibing", "features", "disable", "shell_tool"])
                .expect("parse should succeed");
        let Some(Subcommand::Features(FeaturesCli { sub })) = cli.subcommand else {
            panic!("expected features subcommand");
        };
        let FeaturesSubcommand::Disable(FeatureSetArgs { feature }) = sub else {
            panic!("expected features disable");
        };
        assert_eq!(feature, "shell_tool");
    }

    #[test]
    fn feature_toggles_known_features_generate_overrides() {
        let toggles = FeatureToggles {
            enable: vec!["web_search_request".to_string()],
            disable: vec!["unified_exec".to_string()],
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec![
                "features.web_search_request=true".to_string(),
                "features.unified_exec=false".to_string(),
            ]
        );
    }

    #[test]
    fn feature_toggles_accept_legacy_linux_sandbox_flag() {
        let toggles = FeatureToggles {
            enable: vec!["use_linux_sandbox_bwrap".to_string()],
            disable: Vec::new(),
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec!["features.use_linux_sandbox_bwrap=true".to_string(),]
        );
    }

    #[test]
    fn feature_toggles_accept_removed_image_detail_original_flag() {
        let toggles = FeatureToggles {
            enable: vec!["image_detail_original".to_string()],
            disable: Vec::new(),
        };
        let overrides = toggles.to_overrides().expect("valid features");
        assert_eq!(
            overrides,
            vec!["features.image_detail_original=true".to_string(),]
        );
    }

    #[test]
    fn feature_toggles_unknown_feature_errors() {
        let toggles = FeatureToggles {
            enable: vec!["does_not_exist".to_string()],
            disable: Vec::new(),
        };
        let err = toggles
            .to_overrides()
            .expect_err("feature should be rejected");
        assert_eq!(err.to_string(), "Unknown feature flag: does_not_exist");
    }

    #[test]
    fn strict_config_with_unknown_enable_errors() {
        let err = strict_config_feature_toggle_error(["--enable", "does_not_exist"].as_ref());
        assert_eq!(err.to_string(), "Unknown feature flag: does_not_exist");
    }

    #[test]
    fn strict_config_with_unknown_disable_errors() {
        let err = strict_config_feature_toggle_error(["--disable", "does_not_exist"].as_ref());
        assert_eq!(err.to_string(), "Unknown feature flag: does_not_exist");
    }

    #[test]
    fn strict_config_with_compound_enable_errors() {
        let err = strict_config_feature_toggle_error(
            ["--enable", "multi_agent_v2.subagent_usage_hint_text"].as_ref(),
        );
        assert_eq!(
            err.to_string(),
            "Unknown feature flag: multi_agent_v2.subagent_usage_hint_text"
        );
    }

    fn strict_config_feature_toggle_error(args: &[&str]) -> anyhow::Error {
        let cli_args = std::iter::once("deepseek-vibing")
            .chain(std::iter::once("--strict-config"))
            .chain(args.iter().copied());
        let cli = MultitoolCli::try_parse_from(cli_args).expect("parse should succeed");
        assert!(cli.interactive.strict_config);
        cli.feature_toggles
            .to_overrides()
            .expect_err("feature should be rejected")
    }
}
