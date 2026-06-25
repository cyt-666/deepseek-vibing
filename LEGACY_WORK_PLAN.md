# DeepSeek Vibing 遗留工作计划书

最后更新：2026-06-12

## 1. 文档目标

本文档用于承接阶段 0 到阶段 12 之后的遗留工作。阶段 0 到阶段 12 已经把 `deepseek-vibing` 推进到可编译、可测试、DeepSeek-first 的本地 coding agent 基线。后续工作的重点不再是“能否跑通基础构建”，而是完成真实接口验收、深层遗留清理、产品身份统一和发布级收口。

本文档不替代 `PROJECT_PLAN.md`。`PROJECT_PLAN.md` 继续作为阶段计划和历史进度的事实来源；本文档只管理阶段 12 之后尚未解决的工作项。

## 2. 当前项目状态

### 2.1 已完成能力

- 本地 CLI/TUI 基线可构建。
- 默认路径已切到 DeepSeek provider。
- DeepSeek Chat streaming、reasoning content、tool call、usage/cache 映射已接入。
- reasoning ledger、tool schema、结构化 tool error 已完成。
- stable repo prefix 和 DeepSeek model/effort router 已完成。
- Vision Bridge 已完成，DeepSeek 主模型不直接接收原图。
- FIM core API 和 `deepseek-vibing fim` 已完成。
- trajectory summary、offline replay、sandbox tier 记录已完成。
- eval runner、smoke dataset、quality gates 已完成。
- 阶段 12 已移除本地 MVP 不需要的 cloud-tasks、responses proxy、hosted web/image extension 和 realtime-webrtc 构建链依赖。

### 2.2 已验证状态

- `cargo check --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli` 通过。
- git/repo hygiene 已建立初始可追踪基线：`deepseek-vibing` 独立 git repo 已创建 root commit `f2c30d9`，`.gitignore` 已忽略本地 DeepSeek Vibing home、smoke SQLite/cache 和 Rust target。
- `deepseek-vibing-core` 的 `deepseek`、`vision`、`fim`、`trajectory`、`eval` 筛选测试通过。
- `deepseek-vibing-cli eval` 筛选测试通过。
- 使用真实 `~/.deepseek-vibing/config.toml` 和真实 DeepSeek API 的 P0 live smoke 已通过。
- stable live smoke 已完成文件读取、shell patch、`cargo test --quiet` 验证和 trajectory 写入。
- optional `deepseek-beta` tool smoke 已通过，能够创建文件、读取文件并完成 tool loop。
- live trajectory 已检查，不包含 `ReasoningRawContentDelta`、`AgentReasoningRawContent` 或 `reasoning_content` raw marker。
- 发布前 CLI/TUI 表面收口已完成：顶层 help 隐藏 `app-server`、`remote-control`、`exec-server` 和远端 TUI flags；常用 help 不再出现旧登录引导。
- TUI 插件弹窗默认不展示 OpenAI Curated / ChatGPT Marketplace 一级 tab；官方远端来源在默认列表中显示为中性 `Built-in`。
- `PluginsManager` 默认不再为 DeepSeek / `deepseek-beta` 会话预热远端 featured plugin ids。
- `deepseek-vibing --help` 不再展示 `login`、`logout`、`cloud`、`realtime`、`responses-api-proxy` 和默认 `--search`。
- `fim --help`、`trajectory --help`、`exec --help` 和 `eval list --dataset deepseek-vibing/evals/smoke` 可运行。
- 被移除的 cloud/proxy/realtime/hosted extension crate 已不在当前 Cargo package graph 中。
- `~/.deepseek-vibing/config.toml` 权限已验证为 `0600`。
- P1 auth 清理已进入实现：`deepseek-vibing-login`、`deepseek-vibing-chatgpt`、`deepseek-vibing-backend-client` 已从 `deepseek-vibing-cli` 构建图移除，旧登录源码目录已删除，并新增 `deepseek-vibing-auth` 作为 API-key-only 兼容层。
- TUI 默认启动不再读取 app-server account，不再展示 ChatGPT/API key 登录页、账户卡片、订阅/用量 account 状态或 rate-limit account 轮询。
- app-server account 运行路径已改为 unsupported/no-op，普通 DeepSeek `exec` / TUI 启动不再触发 ChatGPT account/backend token refresh。
- P1 runtime smoke 已通过：stable `exec` 完成读文件、patch、`cargo test --quiet`；optional `deepseek-beta` 完成 strict-tool 文件创建和读回。
- P1 remote plugin backend 清理已完成：ChatGPT backend catalog、featured、remote sync、remote bundle download、remote share/save/list/checkout/delete/updateTargets 路径已从默认构建和 app-server public surface 移除。
- P1 realtime protocol 清理已完成：app-server public JSON-RPC 的 `thread/realtime/*`、TUI realtime/audio UI、core realtime runtime 和 app-server README/schema 中的 realtime 公开说明已移除。
- app-server 长期取舍已收口：当前保留 app-server 作为本地 TUI/SDK/desktop 协议层和 app-server-client 测试边界；不恢复 ChatGPT account、remote plugin backend、realtime 或 Codex Cloud 语义。后续只做本地协议瘦身，除非先设计出替代 IPC/SDK 边界，否则不删除整个 app-server。
- app-server dead-code 第一轮轻量清理已完成：移除 app-server 中 account/backend 清理后遗留的未使用 import、字段、参数和 helper；`cargo check -p deepseek-vibing-app-server` 通过。
- P1 V8 / Code-Mode 清理已完成：`code-mode`、`v8-poc`、`v8`、`deno_core_icudata` 已从 workspace 和 `deepseek-vibing-cli` 构建链移除，DeepSeek Vibing 只保留直接工具调用主路径。
- P1 DeepSeek Tool Surface 收口已完成：DeepSeek / `deepseek-beta` provider 下注册 JSON function 形态的 `apply_patch` 和 `read_file`，并保留 hidden `read` alias；`read_file` 纯逻辑已下沉到轻量 `deepseek-vibing-tools` 测试基线，避免默认验收触发超重 core unit test binary。
- P1 内部命名与品牌统一已完成：Rust workspace、Cargo package、crate/import、npm/Python 包装层、脚本、schema、MCP/tool protocol 字符串和默认文档入口已统一到 DeepSeek Vibing。
- `/resume` 后按 Esc 崩溃已修复并验证：config TOML 反序列化不再触发深递归栈溢出，desktop runtime section 被 typed config 忽略，TUI resume picker Esc 返回不再刷新导致崩溃。
- P1 测试基线与品牌残留收口已完成：core 测试 target 已可完整 `--no-run` 编译，旧 `TestDeepSeekVibing { codex, .. }` / `DeepSeekVibingSpawnOk { codex, .. }` 字段解构已清理，默认可见的 `Codex App` / `Run codex` / `codex home` 启动文案已收口。

### 2.3 最新真实验收发现

- DeepSeek `/chat/completions` 不接受 Codex/OpenAI 内部的 `developer` role。已在 DeepSeek Chat adapter 中将 `developer` 和未知内部文本 role 映射为 `system`，并补充回归测试。
- 旧 live smoke 中曾出现 `chatgpt.com/backend-api/plugins/featured` 的 401 warning。本轮已切断 DeepSeek 默认会话的 featured plugin ids 预热路径，最新 P0 live smoke 未再出现该 warning。
- 旧 P0 live smoke 中，模型曾尝试直接调用 `apply_patch` tool 并收到 `unsupported call: apply_patch`，随后通过 shell patch 完成任务。本批已通过 DeepSeek JSON function `apply_patch` 收口；最新 live smoke 未再出现该 warning。
- 旧 P1 live smoke 中，模型曾尝试过 `read` 这个未注册工具名并收到 `unsupported call: read`。本批已通过 DeepSeek `read_file` function 和 hidden `read` alias 收口，并建立轻量测试基线；最新 live smoke 已通过，未再出现该 warning。
- stable smoke 的输入 token 较高问题已完成本批收口：DeepSeek repo context prefix 默认预算从 `160KB / 80 files` 降到 `48KB / 40 files`，source/lock anchor 改为项目地图式摘要，DeepSeek 静态 base instructions 改为短版专用提示；复验两次 stable input tokens 分别为 `31477` 和 `39845`，均明显低于优化前 `74739`，non-cached input 从优化前 `8947` 降到 `8437` / `8101`。
- Vision Bridge 已完成单元测试、隐私测试和真实 E2E smoke；`scripts/deepseek-vision-live-smoke.sh` 使用真实视觉 provider 将红色 PNG 转成 observation，DeepSeek 正确回答 dominant color 为 red，trajectory 检查未发现 raw image/base64/raw reasoning marker。
- FIM CLI/core 已完成单元测试、CLI 测试和真实 DeepSeek `/beta/completions` live smoke；`scripts/deepseek-fim-live-smoke.sh` 返回 `deepseek-v4-pro` 补全文本 `    +`，usage/cache 字段正常，未发现 API key 泄露。

## 3. 总体原则

后续工作按以下原则推进：

- 先验证真实 DeepSeek 路径，再做大规模 rename。
- 先清用户可见入口和依赖图，再清内部历史命名。
- 对 auth、config、history、resume、trajectory 等持久化路径保持迁移兼容。
- 不为了删除遗留代码牺牲可编译基线。
- 每批清理都必须有明确的验收命令和回滚边界。

## 4. 工作流优先级

### P0：单文件配置基线与真实 DeepSeek 验收（已完成）

目标是先建立 DeepSeek Vibing 的正式配置入口，再确认 mock 测试之外的真实 API 路径可用。

工作内容：

- 标准配置目录为 `~/.deepseek-vibing/`，可通过 `DEEPSEEK_VIBING_HOME` 覆盖。
- 标准配置文件为 `~/.deepseek-vibing/config.toml`。
- API key 与普通配置写在同一个配置文件中，使用 `[auth].deepseek_api_key` 和 `[auth].vision_api_key`。
- 阶段内忽略旧 `$DEEPSEEK_VIBING_HOME` 和 `~/.deepseek-vibing`，不做 fallback，不做迁移。
- `DEEPSEEK_API_KEY` 只作为 CI fallback，优先级低于 `[auth].deepseek_api_key`。
- 使用真实标准配置文件跑最小 `exec` live smoke。
- 验证 DeepSeek streaming text、reasoning delta、tool call、tool result 和 usage 映射。
- 验证多轮 tool loop 中 reasoning content 会回传给 DeepSeek。
- 验证无 key 时错误提示引导创建 `~/.deepseek-vibing/config.toml` 并写入 `[auth].deepseek_api_key`。
- 使用 `deepseek-beta` 验证 strict tool mode 和 `/beta` base URL 行为。
- 验证 `trajectory` 在 live task 中能记录 usage、cache hit/miss、tool summary 和 final diff。

验收标准：

- 无 key 场景不出现 ChatGPT/OpenAI 登录提示。
- 有 key 场景至少完成一次文件读取、一次 shell command、一次 apply_patch 或等价 patch 操作。
- live trajectory 不包含 raw reasoning 明文。
- strict beta 失败时能回退 stable tool calling，并记录 debug/warn 信息。
- 日志、错误、trajectory 和 debug 输出不得包含 API key 原文。

完成记录：

- 标准配置目录和单文件配置已落地到 `~/.deepseek-vibing/config.toml`。
- 真实 DeepSeek stable smoke 已通过文件读取、shell patch 和测试验证。
- 真实 `deepseek-beta` smoke 已通过最小 tool loop。
- trajectory 隐私检查通过，未发现 raw reasoning 明文。

### P0：发布前 CLI/TUI 表面收口（已完成）

目标是让用户第一次运行时看到的是 DeepSeek Vibing，而不是 Codex/OpenAI 产品。

工作内容：

- 清理 `--help`、错误提示、doctor 输出、TUI 状态栏、设置页中的 Codex/OpenAI/ChatGPT 主推文案。
- 默认用户文案只指向 `~/.deepseek-vibing/config.toml`；旧 `$DEEPSEEK_VIBING_HOME` / `~/.deepseek-vibing` 只保留为内部命名或后续显式迁移设计问题。
- 隐藏或标记 app-server、remote-control、plugin marketplace 等非 MVP 入口。
- 检查 `exec --help`、`fim --help`、`trajectory --help`、`eval --help`、`mcp --help`、`doctor --help`。
- 检查无 key `exec` 错误信息，确认不引导 ChatGPT 登录。
- 本轮采用“隐藏不删”策略：保留 app-server、remote-control、exec-server、远端插件等内部兼容路径，但从默认 help、TUI 默认入口和 DeepSeek 默认启动后台任务中移出。

验收标准：

- 顶层 help 不展示 OpenAI/ChatGPT/Codex Cloud/Realtime/hosted web/image。
- 常用子命令 help 不出现 `codex login`、`CODEX_API_KEY` 或 ChatGPT 订阅引导。
- 保留的内部兼容项有明确的后续清理计划。

完成记录：

- `app-server`、`remote-control`、`exec-server` 以及远端 TUI flags 已从顶层 help 隐藏，显式调用仍可用。
- `exec`、`fim`、`trajectory`、`eval`、`mcp`、`doctor` help 已检查，不再出现旧登录引导。
- TUI onboarding、doctor、人类可读诊断输出、插件弹窗、app-link view 已同步 DeepSeek Vibing 文案。
- DeepSeek 默认会话不再预热 ChatGPT featured plugin ids，最新 P0 live smoke 未出现 plugin backend 401 warning。

### P1：Auth 与配置身份整理（已完成第一刀）

目标是把鉴权和配置从 Codex 兼容状态迁移为 DeepSeek Vibing 自己的身份。

工作内容：

- 在 P0 单文件配置基线之上，继续清理内部 `deepseek_vibing_home` 命名和相关文案。
- 如未来需要兼容旧配置，另行设计显式迁移命令，不在默认启动路径中读取旧 `$DEEPSEEK_VIBING_HOME` / `~/.deepseek-vibing`。
- 删除 `deepseek-vibing-login` crate，改为 `deepseek-vibing-auth` API-key-only 兼容层。
- 删除 ChatGPT OAuth/device login、本地 login server、token refresh、revoke、Agent Identity 上层入口。
- 清理 subscription、account plan 等用户可见路径。
- 统一 provider API key 读取优先级：`[auth]` 配置优先，`DEEPSEEK_API_KEY` 仅作为 CI fallback。
- 更新 README、配置文档和错误提示。

验收标准：

- 新用户不需要了解 Codex/OpenAI 登录路径即可完成配置。
- DeepSeek provider 下所有鉴权错误都优先指向 `~/.deepseek-vibing/config.toml` 和 `[auth].deepseek_api_key`。
- `cargo tree -p deepseek-vibing-cli -i deepseek-vibing-login`、`deepseek-vibing-chatgpt`、`deepseek-vibing-backend-client` 无命中。

完成记录：

- `deepseek-vibing-rs/login`、`deepseek-vibing-rs/chatgpt`、`deepseek-vibing-rs/backend-client` 已从源码和 workspace 中移除。
- `deepseek-vibing-rs/agent-identity` 已从源码和 workspace 中移除，`deepseek-vibing-cli` 构建图不再包含 `deepseek-vibing-agent-identity`。
- `deepseek-vibing-auth` 已提供本地 API-key-only auth、secret redaction、测试 helper 和无状态兼容接口。
- 通用 reqwest client、User-Agent、originator、custom CA/proxy 支持已迁到 `deepseek-vibing-client`，不再通过登录系统间接获取。
- `doctor` 和常用 CLI 错误文案已统一指向 `~/.deepseek-vibing/config.toml` 与 `[auth].deepseek_api_key`。

### P1：app-server/backend 深层清理（account/auth、remote plugin backend、realtime protocol 已完成）

目标是继续拆除阶段 12 未动的 app-server、backend、account 和 ChatGPT 历史路径。

工作内容：

- 审计 app-server 中仍依赖 `deepseek-vibing-chatgpt`、backend-client、account、plugin backend 的路径。
- 继续审计 plugin manager、marketplace 或 app-server 的显式远端插件路径；默认 DeepSeek 会话访问 `chatgpt.com/backend-api/plugins/featured` 的问题已在 P0 表面收口中切断，本轮已进一步删除 ChatGPT plugin backend、remote share/sync/download/catalog 路径。
- 判断 app-server 在 DeepSeek 本地 MVP 中的保留范围。
- 删除或软禁用 ChatGPT plugin backend、remote plugin uninstall、account/subscription 逻辑。
- 清理 app-server README 中 OpenAI Compliance、Realtime、ChatGPT plugin 等说明。
- 保留 TUI/exec 仍需要的本地协议能力。
- 后续继续评估 app-server 是否作为长期本地协议层保留，并清理剩余非 MVP 文案、dead code 和内部命名。

验收标准：

- `deepseek-vibing-cli` 的默认构建链进一步减少 ChatGPT/backend 相关依赖。
- P0 live smoke 不再输出 ChatGPT backend、plugin backend 或 marketplace 401 warning。
- app-server 文档只描述 DeepSeek Vibing 当前支持的本地能力。
- 删除范围不破坏 TUI/exec、trajectory、eval 和 MCP。

完成记录：

- `deepseek-vibing-cli` 构建图已不再包含 `deepseek-vibing-login`、`deepseek-vibing-chatgpt`、`deepseek-vibing-backend-client`。
- app-server account request processor 已删除；旧 account/auth JSON-RPC method 现在走标准 method-not-found 路径。
- app-server protocol 已移除旧 account/auth status 公共方法、通知、DTO 和生成 schema；MCP server OAuth/login 协议保留。
- app-server protocol 已移除 `plugin/share/*` 和 `thread/realtime/*` 公共方法与通知；旧客户端调用这些方法走标准 method-not-found 路径。
- app-server-client 和 app-server-test-client 已删除 typed account/read、login、rate-limit helper 和 notification roundtrip 测试残留。
- TUI app-server bootstrap 已取消 account/read；account rate-limit fetch、add credits nudge、logout、login status 查询和 ChatGPT token refresh event plumbing 已删除。
- TUI realtime/audio 入口、事件 plumbing、测试快照和默认插件弹窗中的 ChatGPT/OpenAI marketplace 主推路径已清理。
- TUI auth onboarding 和 headless ChatGPT device-code 登录源码已删除，保留的 onboarding 只负责 welcome/trust directory。
- `cloud-config`、memories write guard、models/backend 相关路径已收敛为本地 no-op 或 DeepSeek 配置语义。
- core-plugins 中 ChatGPT backend catalog、featured、remote sync、remote bundle download、remote share 路径已删除；本地和 Git marketplace 的 list/install/uninstall/upgrade 测试保留并通过。
- app-server README 的旧 account/auth endpoint 和 realtime endpoint/event 章节已替换或删除，`core/config.schema.json` 不再暴露 realtime 配置项。
- V8/code-mode 删除后，`cargo test -p deepseek-vibing-app-server unknown_method`、`cargo test -p deepseek-vibing-core deepseek`、`cargo test -p deepseek-vibing-core trajectory`、`cargo test -p deepseek-vibing-cli eval` 和 P1 live smoke 已通过；此前由 `v8` crate 触发的 rustc ICE 阻塞已解除。
- 最新 P1 live smoke 未出现 ChatGPT backend、plugin featured、marketplace 401 warning 或 V8 编译链；stable `exec` 和 optional `deepseek-beta` smoke 均通过。

### P1：V8 / Code-Mode 逻辑硬删（已完成）

目标是移除原 Codex 的 V8-backed code-mode 能力，解决 `v8` crate 带来的编译链重量和 rustc ICE 阻塞。

完成记录：

- `code-mode`、`v8-poc` workspace member 已删除。
- `deepseek-vibing-code-mode`、`deepseek-vibing-v8-poc`、`v8`、`deno_core_icudata` 依赖已从 workspace 和下游 crate manifest 中移除。
- core tool registry、tool router、session lifecycle、tool spec plan、tool output 和 rollout trace 中的 code-mode 执行/等待/嵌套工具路径已删除。
- `ToolMode` 只保留 direct 兼容解析，旧 code-mode model metadata 会被当作未知工具模式忽略。
- `core/config.schema.json` 不再暴露 `code_mode`、`code_mode_only` 或 `non_code_mode_only` 配置项。
- `cargo check -p deepseek-vibing-cli`、DeepSeek/trajectory/eval/app-server unknown-method 回归和真实 P1 live smoke 已通过，均未再触发 `v8` 编译链。

### P1：DeepSeek Tool Surface 收口（已完成）

目标是让 DeepSeek Chat Completions 看到稳定、标准 JSON function 工具，而不是 OpenAI Responses custom/freeform tool。

完成记录：

- DeepSeek / `deepseek-beta` provider 下，只要当前 turn 有可用 environment，就注册 function 形态的 `apply_patch`。
- function `apply_patch` 参数为 `{ patch: string, environment_id?: string }`，`patch` 必须包含完整 `*** Begin Patch` / `*** End Patch` 文本。
- DeepSeek / `deepseek-beta` provider 下注册 function 形态的 `read_file(path, start_line?, limit?, environment_id?)`，返回结构化 JSON 文件片段。
- 注册 hidden `read` alias 走同一 handler，但不暴露在 DeepSeek request body 的 tool specs 中，避免模型幻觉旧短名时中断 tool loop。
- `read_file` 的 schema、参数解析、行范围校验、成功/失败 JSON 组装已下沉到 `deepseek-vibing-tools`，默认轻量验收使用 `cargo test -p deepseek-vibing-tools read_file`。
- `deepseek-vibing-core` 的 `ReadFileHandler` 只保留 environment 选择、sandbox filesystem 读取和 runtime wiring，避免默认开发循环编译超重 core unit test binary。
- OpenAI/GPT freeform `apply_patch` 路径保留；非 DeepSeek provider 仍按 `model_info.apply_patch_tool_type = freeform` 注册旧 freeform 工具。
- DeepSeek Chat request mapper 会序列化 function `apply_patch`，继续过滤 freeform/custom grammar 工具。
- `ApplyPatchHandler` 的 function/freeform 两种入口复用同一套 parse、verify、approval、sandbox、runtime、diff tracker 和 trajectory apply_patch summary 逻辑。
- `ReadFileHandler` 复用 environment filesystem 和 sandbox context，支持按行截取、上限裁剪、非 UTF-8/目录/缺文件/无权限等结构化错误。
- DeepSeek 静态模型 base instructions 已改为提示优先使用 `read_file` 读取文本文件、使用 JSON function `apply_patch` 编辑文件，避免继续给 DeepSeek 强调 shell/freeform 形态。
- 最新真实 `scripts/deepseek-p0-live-smoke.sh` 已通过：stable smoke 完成 patch + `cargo test --quiet`，optional `deepseek-beta` smoke 完成文件创建和读回；未再出现 `unsupported call: read` / `unsupported call: apply_patch`。

### P1：内部命名与品牌统一（已完成）

目标是把默认项目身份从 fork 过渡到 DeepSeek Vibing，只在许可证、历史归属和显式 legacy alias 中保留旧来源说明。

完成记录：

- Rust workspace 目录已从 `codex-rs` 改为 `deepseek-vibing-rs`。
- Cargo package、workspace dependency 和 Rust crate/import 已统一到 `deepseek-vibing-*` / `deepseek_vibing_*`。
- 顶层 npm wrapper 目录已从 `codex-cli` 改为 `deepseek-vibing-cli`，主命令保持 `deepseek-vibing`。
- `deepseek-auth` 已收敛为 `deepseek-vibing-auth`。
- Python SDK 和 runtime package 已统一为 `deepseek-vibing-sdk` / `deepseek-vibing-cli-bin`，导入包名使用 `deepseek_vibing`。
- helper binary、安装脚本、package build 脚本、Bazel/justfile 目标和常用文档命令已同步 `deepseek-vibing`。
- 官方插件 manifest 默认路径已改为 `.deepseek-vibing-plugin/plugin.json`。
- MCP tool/event/capability 字符串已从 `codex`/`codex/event` 收口到 `deepseek-vibing` / `deepseek-vibing/event`。
- app-server protocol 公开字段已使用 `deepseekVibingHome`、`deepseekVibingErrorInfo` 等新命名；`codexErrorInfo` 仅作为反序列化 alias 兼容旧数据读取。
- app-server JSON/TypeScript schema 已重新生成并通过 fixture 测试。
- 用户可见配置/home/env 文案继续以 `~/.deepseek-vibing/config.toml` 和 `DEEPSEEK_VIBING_HOME` 为准。
- 源码目录在机械检查 `codex-|codex_|Codex|CODEX_HOME|~/.codex|\\.codex-plugin` 下已无命中，发布面 URL/env/protocol 路径扫描也已无旧 OpenAI/Codex 默认入口命中。
- README、NOTICE、CLA、`PROJECT_PLAN.md`、本文件中的 Codex 命中仅作为 fork 历史、许可证归属或历史阶段说明保留。

验收标准：

- `cargo check --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli` 通过。
- `cargo metadata --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml --no-deps` 成功，package 列表不含 `codex-*`。
- `cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-app-server-protocol schema_fixtures -- --nocapture` 通过。
- `cargo tree --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli -i codex-core`、`codex-login`、`codex-chatgpt` 均为 package not found。

### P1：测试基线与品牌残留收口（已完成）

目标是在大规模 rename 后恢复可靠测试编译基线，并移除默认用户表面的旧 Codex 文案残留。

完成记录：

- core 测试中失效的 `TestDeepSeekVibing { codex, .. }` 和 `DeepSeekVibingSpawnOk { codex, .. }` 字段解构已改为 `deepseek_vibing`。
- 同一作用域内代表会话/thread 的局部变量已同步到 `deepseek_vibing`，保留的 `codex` 字符串仅用于 legacy model name、fixture 路径、历史协议 key 或兼容测试。
- TUI/exec 启动错误中的 `codex home` 已改为 DeepSeek Vibing home。
- TUI saved-session goal 快照中的 `Run codex` 已改为 `Run deepseek-vibing`。
- `/resume` + Esc 崩溃已完成修复和手工验证，历史 session 中的 desktop runtime section 不再导致 typed config 反序列化栈溢出。

验收标准：

- `cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core --no-run` 通过。
- `rg 'TestDeepSeekVibing \\{ codex|DeepSeekVibingSpawnOk \\{ codex' deepseek-vibing/deepseek-vibing-rs` 无命中。
- 默认 TUI/help/exec 启动路径不再出现 `Codex App`、`Run codex` 或 `Error finding codex home`。

### P2：Vision Bridge 产品化

目标是把阶段 8 的 Vision Bridge 从可用能力推进到稳定产品能力。

工作内容：

- 增加视觉 provider 配置示例。（已完成：README 中的 `[vision]` 示例改为要求真实 provider/model）
- 增加更完整的缓存清理和缓存目录配置。（已完成第一批：`cache_dir`、entry/bytes/TTL 上限和 best-effort prune）
- 支持不同 detail/task hint 的策略建议。（已完成第一批：README 说明 Vision Bridge 对截图、UI 错误图、仓库图片、data URL 的 observation-first 用法）
- 验证截图、UI 错误图、仓库图片、data URL 四类输入。（已完成第一批：补充 cache/data URL/privacy 相关测试，真实 E2E smoke 已覆盖图片 observation 主路径）
- 使用真实视觉 provider 跑一次端到端 live smoke，确认 `[auth].vision_api_key` 优先级、缓存、失败 observation 和 DeepSeek request 隐私边界。（已完成：`scripts/deepseek-vision-live-smoke.sh` 通过）
- 评估是否需要多视觉模型路由。（后续保留）

验收标准：

- 无视觉 key 时图片输入不会中断主 turn。
- 视觉错误 observation 结构稳定。
- DeepSeek request body 不包含原图、base64 或 raw image URL。

### P2：FIM 与 IDE 集成预留

目标是把阶段 9 的 FIM CLI/core API 向 IDE 或编辑器集成推进。

工作内容：

- 使用真实 DeepSeek `/beta/completions` 跑一次 `deepseek-vibing fim` live smoke，确认 `[auth].deepseek_api_key`、usage/cache 字段和错误脱敏路径。（已完成：`scripts/deepseek-fim-live-smoke.sh` 通过）
- 设计 FIM LSP 或轻量补全服务边界。
- 定义 prefix/suffix 截断策略和 token budget。
- 增加缓存、去重和 debounce 策略。
- 评估是否需要 streaming FIM。
- 保持 FIM 与主 agent loop 解耦。

验收标准：

- `deepseek-vibing fim` 仍保持稳定。
- IDE 预留接口不改变主 agent turn 行为。
- FIM 输出不自动 apply，仍由调用方决定如何使用。

### P2：Eval 与质量门槛增强

目标是让 eval 从 smoke MVP 变成持续回归工具。

工作内容：

- 建立固定 baseline report。（已完成：`evals/baselines/smoke-baseline.json`）
- 增加更多任务类型：多文件重构、测试失败恢复、权限/沙箱、配置变更、CLI 参数解析。（已完成：`evals/regression`）
- 增加可选 live task 集合。（已完成第一批：regression 数据集可用 `eval run --dataset evals/regression --mode live` 显式运行）
- 引入用户配置价格表，计算成本估算。（已完成：`[eval.pricing]` 可选配置，未配置时 `cost_estimate = null`）
- 增加趋势对比和回归摘要。（已完成第一批：baseline comparison、metric deltas、task regressions/recoveries）
- 后续可继续增加图片观察任务、更多语言/仓库形态任务和多次 report 趋势图。

验收标准：

- `eval run --baseline --enforce` 可用于本地 CI。（已完成：baseline 存在时自动 enforced，已通过固定 smoke baseline 设计）
- 报告能区分 offline operational metrics 和 live task pass rate。
- cost 默认仍可为空，不硬编码 DeepSeek 价格。（已完成）

### P3：Sandbox Runtime 深化

目标是把阶段 10 的 sandbox tier 记录推进到实际隔离能力。

工作内容：

- 明确 function、container、microVM、fullVM、external 各 tier 的语义。
- 评估本地 container runtime 方案。
- 设计网络开关、文件挂载、缓存目录和权限审批模型。
- 先实现 container tier，再考虑 microVM/fullVM。
- 将 runtime 选择写入 trajectory 和 eval 报告。

验收标准：

- 默认行为仍遵循现有审批和 sandbox 策略。
- 新 runtime 只在用户显式启用时使用。
- trajectory 能清晰记录实际 sandbox tier 和网络状态。

## 5. 推荐推进顺序

建议按以下顺序推进：

1. 完成 P0 live smoke 和 CLI/TUI 用户表面收口。（已完成）
2. 完成 app-server protocol account/auth、remote plugin backend、realtime protocol 硬删。（已完成）
3. 完成内部 rename、测试基线和品牌残留收口。（已完成）
4. 收敛 DeepSeek `read` / `read_file` tool alias，减少 live tool loop 中的 unsupported call。（已完成，最新 live smoke 复验通过）
5. 保留 app-server 作为长期本地协议层，持续清理云端/backend dead code。
6. 做 Vision Bridge、FIM、eval 和 sandbox 的产品化增强。

这个顺序的原因是：真实 API 验证和用户表面收口最容易暴露 MVP 是否可用；auth/config 迁移影响持久化路径，必须早定；内部 rename 应放在主路径稳定之后，避免把功能问题和命名问题混在一起。

## 6. 风险与决策点

### 6.1 配置目录迁移

`$DEEPSEEK_VIBING_HOME` 和 `~/.deepseek-vibing` 仍有大量历史引用。直接重命名可能破坏已有 session、auth、trajectory、eval report 和 config。需要先设计兼容读取和迁移提示。

### 6.2 app-server 是否进入 MVP

app-server 仍承担部分本地协议能力。account/auth protocol、remote plugin backend 和 realtime protocol 已完成硬删，但 app-server 仍有历史命名、部分 backend/no-op 兼容类型和较重的测试依赖。需要明确它是 DeepSeek Vibing 的长期本地协议层，还是只作为过渡兼容层。

### 6.3 ChatGPT/Auth 类型清理

`deepseek-vibing-login` 已删除，DeepSeek Vibing 现在通过 `deepseek-vibing-auth` 读取单文件配置。app-server account/auth protocol 已硬删；残留风险集中在历史命名、少量 auth/status 兼容文案或测试 fixture，以及后续内部 crate rename。

### 6.4 内部 rename 风险

大规模 rename 已完成，并已通过 CLI 构建、Cargo 元数据、协议 schema fixture 和旧品牌扫描。后续风险主要来自历史 session、外部脚本或第三方集成仍引用旧二进制/包名；如需要兼容，应以显式 migration alias 或文档说明处理，不恢复旧默认入口。

### 6.5 Live API 不稳定性

DeepSeek API 的 model name、strict beta、usage 字段和 pricing 可能变化。live smoke 应允许跳过，并在文档中注明依赖当前官方 API 行为。

## 7. 每批工作的标准验收

每批遗留工作完成后至少运行：

```shell
cargo fmt --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml --all
cargo check --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli
cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core deepseek -- --nocapture
cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core vision -- --nocapture
cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core fim -- --nocapture
cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core trajectory -- --nocapture
cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core eval -- --nocapture
cargo test --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli eval -- --nocapture
```

如果涉及 CLI 表面，还应运行：

```shell
deepseek-vibing --help
deepseek-vibing exec --help
deepseek-vibing fim --help
deepseek-vibing trajectory --help
deepseek-vibing eval list --dataset deepseek-vibing/evals/smoke
```

如果涉及依赖删减，还应运行：

```shell
cargo tree --manifest-path deepseek-vibing/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli
```

## 8. 暂不处理事项

以下事项不建议立即处理：

- 不立即删除整个 app-server。
- 不立即迁移所有历史 session 和 trajectory 文件格式。
- 不立即实现 microVM/fullVM sandbox runtime。
- 不立即把 FIM 接入主 agent loop。
- 不立即硬编码 DeepSeek 价格。

这些事项应在 P0/P1 收口后再单独拆计划。
