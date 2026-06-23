# DeepSeek Vibing 项目整体计划

最后更新：2026-06-06

## 1. 项目目标

`deepseek-vibing` 是一款基于开源 Codex 改造的本地 coding agent。项目目标是保留 Codex 中成熟的本地工具循环、文件编辑、命令执行、沙箱、CLI 和 TUI 能力，同时移除 OpenAI、ChatGPT 订阅和 Codex 云端专属功能，并面向 DeepSeek V4 系列模型重新设计模型调用、上下文组织、工具协议和执行轨迹。

第一版聚焦本地开发场景。用户通过 `DEEPSEEK_API_KEY` 或配置文件接入 DeepSeek API，不需要 OpenAI 账户、ChatGPT 登录或 OpenAI 订阅。

## 2. 设计原则

### 2.1 DeepSeek 优先

项目不是简单替换 API base URL，而是围绕 DeepSeek V4 的能力重做 agent 核心路径。重点利用以下能力：

- 1M token 长上下文。
- context caching。
- thinking 和 non-thinking 模式。
- tool call 场景下的 reasoning content 连续保留。
- strict tool schema。
- V4-Pro 和 V4-Flash 的成本与能力分层。
- FIM（Fill-in-the-Middle）补全能力。

### 2.2 本地优先

第一版只保留本地 CLI/TUI 工作流。云任务、Codex backend、ChatGPT OAuth、OpenAI subscription 检查等功能不进入 MVP。

### 2.3 可验证优先

agent 的可靠性来自工具、测试和审查，而不是一次性长回答。每个 coding 任务都应尽量形成可验证轨迹，包括文件 diff、测试命令、失败日志、修复尝试和最终总结。

### 2.4 可审计优先

写文件、运行命令、安装依赖、访问网络等操作必须有明确边界。默认保留 Codex 的审批和沙箱思路，并逐步扩展为更适合 DeepSeek agent 的分层 sandbox 和 trajectory log。

## 3. MVP 范围

### 3.1 首版包含

- 本地 CLI 和 TUI。
- DeepSeek API key 配置。
- DeepSeek chat/tool provider。
- 文件读取、搜索和编辑。
- `apply_patch` 写入路径。
- shell command 和 approval。
- sandbox 基础能力。
- 多轮 tool loop。
- reasoning content ledger。
- 基础模型路由。
- 最终任务总结。

### 3.2 首版不包含

- ChatGPT 登录。
- OpenAI 订阅检查。
- Codex 云任务。
- OpenAI hosted web search。
- OpenAI image generation。
- Realtime/WebRTC。
- Codex backend。
- GitHub bot 或云端任务队列。
- 完整 IDE 插件。

## 4. 总体架构

```text
CLI / TUI / 后续 IDE 集成
        |
        v
Task Router
        |
        +-- Context Plane
        |     +-- Stable Repo Prefix
        |     +-- Symbol / Dependency Index
        |     +-- Cache Telemetry
        |     +-- Attention-aware Packing
        |
        +-- Model Router
        |     +-- DeepSeek V4-Pro
        |     +-- DeepSeek V4-Flash
        |     +-- DeepSeek FIM
        |     +-- Vision Bridge Provider
        |
        +-- Tool Loop
        |     +-- Strict Tool Schema
        |     +-- Reasoning Ledger
        |     +-- apply_patch / shell / tests / git diff
        |
        +-- Sandbox Runtime
        |     +-- function
        |     +-- container
        |     +-- microVM
        |     +-- fullVM
        |
        v
Patch + Tests + Review Summary
```

## 5. 阶段计划

### 阶段 0：项目边界确认

目标是锁定第一版范围，避免后续改造过程中反复摇摆。

工作内容：

- 明确第一版只做本地 CLI/TUI。
- 明确鉴权方式为 `DEEPSEEK_API_KEY` 和配置文件。
- 明确默认模型组合：V4-Pro 作为主 agent，V4-Flash 作为快速任务和子任务模型，FIM 用于补全。
- 明确 OpenAI 和 ChatGPT 专属能力不进入 MVP。

阶段产物：

- 本文件。
- 后续每个阶段的详细实施计划。

### 阶段 1：建立可编译 fork 基线

目标是让 `deepseek-vibing` 成为一个可以独立构建的项目。

工作内容：

- 从 `codex` 复制必要源码到 `deepseek-vibing`。
- 修改项目名、包名、二进制名和命令名。
- 将基础文案从 Codex/OpenAI 改为 DeepSeek Vibing。
- 保持主要 workspace 结构能编译。
- 运行最小验证，例如 `cargo check`。

阶段产物：

- `deepseek-vibing` 可以独立构建。
- 项目仍可保留部分 Codex/OpenAI 遗留代码，但不能影响本地构建。

### 阶段 2：OpenAI 专属功能软禁用

目标是在不大规模删除源码的前提下，先关闭 OpenAI 和 ChatGPT 专属路径。

优先禁用：

- ChatGPT OAuth 和 device login。
- OpenAI subscription/account plan 检查。
- Codex backend 和 cloud tasks。
- Responses WebSocket 专属路径。
- OpenAI hosted web search、image generation 和 realtime。
- ChatGPT cookies 和 Cloudflare 相关逻辑。
- OpenAI model catalog 和默认 GPT 模型路由。
- OpenAI 包发布脚本和品牌文案。

继续保留：

- CLI/TUI。
- agent loop。
- 本地文件读写。
- `apply_patch`。
- shell command 和 approval。
- sandbox。
- MCP/local tools。
- conversation history。
- config loader。
- 测试执行能力。

阶段产物：

- 用户不会被要求登录 OpenAI 或使用 OpenAI 订阅。
- 项目进入 DeepSeek-only 本地模式。

实施状态（2026-06-03）：

- 已开始：默认 built-in provider 切到 `deepseek`，默认鉴权改为 `DEEPSEEK_API_KEY`。
- 已开始：ChatGPT/OpenAI 专属 CLI 入口采用 soft-disable 策略，隐藏并返回明确 unsupported 信息。
- 已开始：OpenAI hosted web search、image generation、realtime 通过 DeepSeek provider capability 从默认工具面隐藏。
- 已开始：DeepSeek 默认 provider 下禁用启动期 OpenAI/ChatGPT 远程插件同步，避免本地 `exec` 默认访问 `chatgpt.com` 或 `github.com/openai/plugins.git`。
- 已保留：OpenAI/ChatGPT/Amazon/OSS 相关源码和 crate 暂不硬删，继续作为显式配置或后续清理对象。

### 阶段 3：DeepSeek Provider Adapter

目标是打通 DeepSeek Chat/Tool API，而不是继续依赖 OpenAI Responses API。

工作内容：

- 新增 wire API 类型，例如 `deepseek_chat`。
- 将 Codex 内部 `ResponseItem` 转换为 DeepSeek chat messages。
- 将 Codex tool spec 转换为 DeepSeek strict tool schema。
- 支持 DeepSeek streaming。
- 支持 `reasoning_effort`。
- 支持 `reasoning_content` 的接收、保存和回传。
- 适配 DeepSeek 错误格式。
- 记录 token 和 cache telemetry。
- strict tool mode 只通过 `deepseek-beta` provider alias 打开，因为 DeepSeek strict tool calling 需要 `/beta` base URL；默认 `deepseek` provider 使用稳定非 strict tool calling。

阶段产物：

- 能通过 DeepSeek API 完成单轮对话。
- 能通过 DeepSeek API 完成包含 tool call 的 agent turn。

实施状态（2026-06-03）：

- 已开始：新增 `deepseek_chat` wire API、DeepSeek 静态模型目录和 Chat Completions streaming adapter。
- 已开始：`reasoning_content` 会进入现有 reasoning event/history 通道，tool call 前的 reasoning 会在后续请求中回传给 DeepSeek。
- 已开始：DeepSeek usage 映射到现有 `TokenUsage`，包含 prompt cache hit 和 reasoning output token 字段。

### 阶段 4：Reasoning Ledger

目标是利用 DeepSeek V4 在 tool-calling 场景下保留 reasoning content 的能力。

工作内容：

- 保存 assistant message 中的 `reasoning_content`。
- 后续请求原样传回 reasoning content、tool calls 和 tool results。
- UI/TUI 永不展示 reasoning content。
- rollout/trace 默认不明文持久化 reasoning content。
- 增加多轮 tool call 测试，确保 ledger 不丢失。

阶段产物：

- DeepSeek V4 的 interleaved thinking/tool calling 能力被主 agent loop 使用。

实施状态（2026-06-04）：

- 已开始：DeepSeek streaming mapper 覆盖 reasoning delta、assistant text、tool calls、usage 和 tool-call finish。
- 已开始：当前 session history 保留 `reasoning_content`，并在下一轮 DeepSeek Chat 请求中随 assistant tool call 回传。
- 已完成：rollout 默认不持久化 raw reasoning events，`ReasoningText` 序列化时不落明文，跨进程 resume 暂不恢复 raw reasoning。
- 已完成：补充 ledger/privacy 单元测试，验证多轮 tool call 回传和 rollout secret 不泄露。

### 阶段 5：Strict Tool 与本地执行收敛

目标是提高工具调用稳定性，并减少模型自由拼接参数造成的错误。

工作内容：

- 工具 schema 默认 `additionalProperties: false`。
- 将常用工具逐步收敛为少量强类型工具。
- 后续优先保留 `read_file`、`search_code`、`apply_patch`、`run_command`、`run_tests` 和 `git_diff`。
- 工具错误返回结构化 JSON。
- shell command 默认限时、限 cwd，并遵守 sandbox。
- 写文件优先收敛到 `apply_patch`。

阶段产物：

- 工具调用错误率下降。
- agent 行为更容易审计和回放。

实施状态（2026-06-04）：

- 已开始：DeepSeek stable tool schema 递归补 `additionalProperties: false`，但保留原有 `required`。
- 已开始：`deepseek-beta` strict tool mode 只在 schema 可严格化时加 `strict: true` 和 required-all；不兼容 schema 自动回退 stable 并写 warning。
- 已开始：工具失败返回模型可读结构化 JSON，包含 `ok:false`、`kind`、`message`、`tool`；命令类错误额外包含 exit code、cwd、stdout、stderr、duration 和 truncated。
- 已保留：本阶段不新增 `read_file/search_code/run_tests/git_diff` 等外部工具名，继续复用现有 Codex 工具主路径。

### 阶段 6：Context Plane 与 Stable Repo Prefix

目标是利用 DeepSeek V4 的长上下文和 context caching。

工作内容：

- 构建稳定仓库前缀。
- 前缀中放置 repo manifest、README、依赖文件、关键源码、符号索引和测试命令。
- 使用固定排序，避免每轮 prompt 抖动。
- 将当前任务、最新 diff、失败测试和近期工具结果放入 suffix。
- 将当前编辑文件或关键片段重复放在 suffix 末端。
- 记录 `prompt_cache_hit_tokens` 和 `prompt_cache_miss_tokens`。
- 为文件块增加 path、hash、exports、summary 和 content anchor。

阶段产物：

- 多轮任务获得更高 cache hit。
- agent 对大仓库的全局一致性更强。

实施状态（2026-06-04）：

- 已开始：新增 DeepSeek stable repo prefix 的确定性 MVP，不做持久 repo index、不做模型生成摘要、不做跨进程 prefix 缓存。
- 已开始：repo prefix 仅在 `deepseek` / `deepseek-beta` provider 且 `[deepseek.context_prefix] enabled = true` 时注入 full initial context；默认上限为 `max_bytes = 160000`、`max_file_bytes = 20000`、`max_files = 80`。
- 已开始：prefix 文件块包含 path、sha256、size、exports、summary 和 content anchor；默认排除 `.git`、`target*`、`node_modules`、`.pnpm-store`、`dist`、`build`、`out`、`.cache` 等生成目录，并支持 include/exclude glob 覆盖。
- 已保留：最新任务、tool output、apply_patch diff 和失败测试仍走动态 suffix/history；本阶段不重构 suffix packing。
- 已记录：DeepSeek `prompt_cache_hit_tokens` 继续映射到 `cached_input_tokens`，cache miss 使用现有 `TokenUsage::non_cached_input()` 派生，不新增公共协议字段。

### 阶段 7：Model/Effort Router

目标是按任务风险、成本和上下文需求自动选择模型和 reasoning effort。

默认策略：

- 简单解释和日志总结：V4-Flash non-thinking。
- 单文件小修：V4-Flash 或 V4-Pro high。
- 多文件 bugfix：V4-Pro high。
- 高风险重构、并发、安全、Rust unsafe、CUDA 和 C++ ABI：V4-Pro max。
- 行内补全和函数内补全：FIM non-thinking。
- 最终 review：V4-Flash 或 V4-Pro 独立 reviewer。

阶段产物：

- 自动模型路由。
- 自动 effort 路由。
- 每轮记录成本、token 和 cache 命中情况。

实施状态（2026-06-04）：

- 已开始：新增 DeepSeek 专用 deterministic router，默认对 `deepseek` / `deepseek-beta` provider 开启。
- 已开始：显式 `model` 或 `model_reasoning_effort` 优先于 router；router 只覆盖未显式指定的字段。
- 已开始：简单解释/日志总结路由到 `deepseek-v4-flash` + low，默认小任务路由到 `deepseek-v4-flash` + medium，多文件、测试失败、Rust 类型/生命周期和高风险关键词路由到 `deepseek-v4-pro` + high。
- 已开始：router 决策写入 debug/trace，不新增用户可见 CLI/TUI 命令。
- 已保留：FIM 只在内部决策类型和计划中预留，本阶段不实现 inline/function completion 请求路径。

### 阶段 8：Vision Bridge

目标是在 DeepSeek 暂不支持多模态的前提下，让 agent 可以处理图片、截图和设计稿。

设计原则：

- DeepSeek 仍然负责规划、代码推理、工具调用和补丁生成。
- 视觉模型只负责观察图片、OCR、UI 描述和错误截图解析。
- DeepSeek 上下文只接收结构化文本描述，不直接接收图片。
- 原图保存在本地 artifact store，并通过 hash 或 image id 引用。

工作内容：

- 新增 `vision_provider` 配置。
- 支持本地或云端视觉模型适配器。
- 增加 `describe_image(image_path, task_hint)` 工具。
- 支持 OCR、UI 截图描述、设计稿分析和报错截图分析。
- 对图片按 hash 缓存，避免重复调用视觉模型。
- 将视觉结果写入 `image_observation`。

示例配置：

```toml
[vision]
enabled = true
provider = "custom"
model = "vision-model"
cache = true
send_original_images_to_main_model = false
```

示例输出：

```json
{
  "image_id": "sha256:...",
  "source": "user_upload",
  "summary": "页面右上角出现登录失败提示。",
  "visible_text": ["Login failed", "Invalid token"],
  "ui_elements": [
    {
      "type": "error",
      "label": "Login failed",
      "location": "top-right",
      "confidence": 0.91
    }
  ],
  "important_details": ["错误提示和 token 校验有关"],
  "uncertain_details": []
}
```

阶段产物：

- agent 可以处理截图类任务。
- 后续 DeepSeek 支持多模态时，可以将 Vision Bridge 替换为 direct multimodal provider。

实施状态（2026-06-04）：

- 已开始：新增 `[vision]` 配置，默认使用 OpenAI-compatible 视觉接口，字段包含 `enabled`、`provider`、`model`、`base_url`、`api_key_env`、`cache`、`timeout_ms` 和 `send_original_images_to_main_model`。
- 已完成：DeepSeek / `deepseek-beta` 下强制不把原图发给主模型；若配置 `send_original_images_to_main_model = true`，启动时给 warning 并忽略。
- 已完成：新增 Vision Bridge 核心模块，将本地图片或 data URL 转成安全的 `ImageObservation` JSON；成功结果按图片 hash、task hint、detail、provider 和 model 缓存。
- 已完成：用户输入图片时自动预处理，DeepSeek 当前 turn 收到 `<image_observation>` 结构化文本；缺少视觉 key 或 provider 出错时注入 `ok:false` 观察而不是中断 turn。
- 已完成：新增 `describe_image(path, task_hint?, detail?, environment_id?)` 工具，用于 DeepSeek 后续主动观察本地截图/图片文件；旧 `view_image` 仍仅服务真正支持 image input 的模型。
- 已记录：当前阶段只支持单个 configured vision model，不做本地 OCR 引擎、视觉模型路由或 live smoke 强依赖。

### 阶段 9：FIM 补全子系统

目标是提供独立的代码中间补全能力，不把 FIM 混入主 agent loop。

工作内容：

- 输入当前 buffer 的 prefix 和 suffix。
- 使用 DeepSeek FIM non-thinking。
- 限制 max tokens。
- 只返回局部补全。
- 为后续 IDE 集成预留 API。

阶段产物：

- `deepseek-vibing fim` 或内部 FIM API。

实施状态（2026-06-04）：

- 已开始：新增 `[deepseek.fim]` 配置，默认使用 `deepseek-v4-pro`、`https://api.deepseek.com/beta`、`DEEPSEEK_API_KEY`、`max_tokens = 512`、`temperature = 0.2`。
- 已完成：新增独立 DeepSeek FIM core API，请求 `POST /completions`，只发送 `prompt`、`suffix`、`model`、`max_tokens`、`temperature`、`stream:false` 和 `echo:false`，不携带 chat messages、tools、reasoning、图片或 agent history。
- 已完成：新增 `deepseek-vibing fim --prefix-file <FILE|-> [--suffix-file <FILE|->]`，默认 stdout 只输出补全文本，`--json` 输出结构化结果和 usage。
- 已完成：`max_tokens` 按 `1..=4096` 校验；prefix/suffix 不能同时从 stdin 读取；缺少 API key 或 provider 错误返回清晰错误且不泄露 key。
- 已记录：阶段 9 不实现 streaming FIM、IDE/LSP/TUI inline completion，也不让 router 或主 agent loop 自动调用 FIM。

### 阶段 10：Trajectory、Sandbox 与 Replay

目标是让 agent 行为可审计、可复盘、可评测。

工作内容：

- 新增 `[trajectory]` 配置：
  - `enabled = "auto"`：默认只对 `deepseek` / `deepseek-beta` 生效，`always` 对所有 provider 生效，`never` 关闭。
  - `detail = "summary"`：默认只写摘要型 trajectory；`raw` 才同步开启 raw rollout trace。
  - `root_dir = null`：默认写入 `$DEEPSEEK_VIBING_HOME/trajectories`。
  - `output_max_bytes = 20000`，`diff_max_bytes = 200000`。
- 每个 turn 记录 provider、model、reasoning_effort、token usage/cache hit/miss、sandbox tier、网络状态、工具调用摘要和最终状态。
- 对 `exec_command` 记录 command、cwd、duration、exit_code、stdout/stderr/aggregated output 摘要和 truncated 标记。
- 对 `apply_patch` 记录 success/status、stdout/stderr 摘要、变更文件列表和 diff 摘要。
- summary trajectory 不保存 raw reasoning、完整 prompt/response 或 raw payload。
- `detail = "raw"` 时把现有 rollout trace bundle root 自动指向 trajectory root 下的 `raw/` 子目录。
- 新增 `trajectory list/show/replay` CLI；第一版 replay 只生成离线还原报告，不重新执行命令、不恢复文件状态、不自动 rerun tests。
- 设计并记录 sandbox tier：`function`、`container`、`micro_vm`、`full_vm`、`external`、`unknown`；阶段 10 只做映射和记录，不实现新的隔离 runtime。

阶段产物：

- agent 轨迹可审计：`trajectory-<timestamp>-<thread_id>.jsonl`。
- 失败任务可离线复盘：`deepseek-vibing trajectory replay <TRAJECTORY_ID|PATH>`。
- 后续可将轨迹用于 eval 和调优。

实施状态（2026-06-05）：

- 已开始：新增 `[trajectory]` 配置和运行时解析，默认 `auto + summary`，DeepSeek provider 默认启用。
- 已完成：新增 summary trajectory writer，挂入 `Session::send_event`，从现有事件总线提取 turn、token usage、exec、apply_patch 和 diff 摘要。
- 已完成：summary JSONL 禁止记录 raw reasoning；raw reasoning 事件不会进入 trajectory writer。
- 已完成：新增 `TrajectorySandboxTier` / network 状态映射；当前本地 sandbox 归为 `function`，外部 sandbox 归为 `external`，其他 tier 先保留占位。
- 已完成：`detail = "raw"` 自动把 raw rollout trace root 放到 trajectory root 的 `raw/` 下；环境变量 raw trace 仍作为显式 debug 路径保留。
- 已完成：新增 `deepseek-vibing trajectory list`、`show <id|path> [--json]`、`replay <id|path> [--output <FILE>]`。
- 已记录：阶段 10 replay 不重新执行命令，不恢复文件系统，不新增 container/microVM/fullVM runtime。

### 阶段 11：评测与质量门槛

目标是用数据判断 agent 是否变好。

评测指标：

- pass rate。
- 平均轮数。
- 平均 token 和成本。
- cache hit ratio。
- patch 接受率。
- 测试失败恢复率。
- 工具调用错误率。
- over-thinking 次数。
- 用户中断和回滚次数。

工作内容：

- 建立本地 coding task 集合。
- 实现 eval runner。
- 输出简单报告。
- 将失败案例纳入下一轮改造计划。

阶段产物：

- 本地 eval 数据集。
- 质量报告。
- 可比较的版本指标。

实施状态（2026-06-05）：

- 已开始：新增 `[eval]` 配置，默认 dataset 为 `./evals/smoke`，reports 写入 `$DEEPSEEK_VIBING_HOME/eval-reports`，work dir 使用系统临时目录下的 `deepseek-vibing-eval`。
- 已开始：新增 `[eval.gates]`，默认 gate 为 `min_pass_rate = 0.80`、`max_avg_turns = 8`、`max_tool_error_rate = 0.20`，`max_overthinking_tasks` 默认不启用。
- 已完成：新增 core eval 类型和报告聚合逻辑，覆盖 task TOML 解析、workspace materialize、trajectory summary 聚合、pass/cache/tool/patch/over-thinking 指标和 gate 判定。
- 已完成：新增 `deepseek-vibing eval list`、`eval run`、`eval summarize-trajectories`；默认只生成报告，只有 `--enforce` 或提供 `--baseline` 时 gate 失败才返回非零退出码。
- 已完成：新增 6 个内置 Rust smoke task，均为独立微型 Cargo crate，使用 `cargo test --quiet` 验证，不支持任意 setup shell。
- 已记录：`hybrid` 模式下仅当 `DEEPSEEK_API_KEY` 存在才跑 live coding smoke；无 key 时跳过 live task 并仍可汇总离线 trajectory 指标。
- 已记录：第一版 `cost_estimate = null`，不硬编码 DeepSeek 价格；后续可通过用户配置价格表补充成本估算。

### 阶段 12：代码库精简

目标是在 DeepSeek 主路径稳定后，再大规模删除无关源码和依赖。

删除方向：

- OpenAI auth/login crates。
- Codex backend/cloud task crates。
- OpenAI hosted tool integrations。
- Responses WebSocket proxy。
- Realtime/WebRTC。
- OpenAI image-generation 和 web-search extension。
- Bedrock/OpenAI model special cases。
- OpenAI 发布、CI 和 docs 专属内容。

阶段产物：

- 代码库更小。
- 依赖更少。
- 项目身份从 Codex fork 转向 DeepSeek Vibing。

实施状态（2026-06-06）：

- 已开始：采用保守分批瘦身策略，优先断开本地 CLI/TUI 默认构建链中的 OpenAI/ChatGPT 云端入口和 hosted extension 依赖，不在本阶段做内部 crate 全量改名。
- 已完成：从顶层 CLI 删除 `login`、`logout`、`cloud` 和 `responses-api-proxy` 子命令；保留本地 `exec`、TUI、`review`、`fim`、`trajectory`、`eval`、`mcp`、`sandbox`、`apply`、`resume/archive/fork` 与 `debug prompt-input` 路径。
- 已完成：将 `apply` 从 `deepseek-vibing-chatgpt` 云端 apply helper 迁到 CLI 内部本地 patch 应用逻辑，`deepseek-vibing-cli` 不再直接依赖 `deepseek-vibing-chatgpt`。
- 已完成：app-server 默认 extension 安装不再加载 OpenAI hosted `web-search` 和 `image-generation` extension；相关 extension crate 从 workspace、依赖图和源码目录中移除。
- 已完成：移除 `cloud-tasks*`、`responses-api-proxy`、`deepseek-vibing-realtime-webrtc` workspace member、依赖和源码目录；TUI `/realtime` 用户入口隐藏，TUI WebRTC 事件路径改为 unsupported/no-op。
- 已保留：`deepseek-vibing-login` 暂作为 API-key/auth compatibility 层保留；app-server/backend/account/realtime 协议类型和更深的 ChatGPT 处理留到后续深层清理。
- 已记录：本阶段目标是构建链变轻且 DeepSeek 本地主路径继续可用，不为“删干净”牺牲可编译基线。

## 6. 推荐里程碑

推荐里程碑与阶段计划保持一一对应，避免后续执行时出现“阶段编号”和“里程碑编号”错位。

| 里程碑 | 对应阶段 | 目标 |
| --- | --- | --- |
| M0 | 阶段 0 | 项目边界、MVP 范围和默认模型组合落文档 |
| M1 | 阶段 1 | `deepseek-vibing` fork 可独立编译和运行基础 CLI |
| M2 | 阶段 2 | OpenAI / ChatGPT 专属入口软禁用，默认路径切到 DeepSeek |
| M3 | 阶段 3 | DeepSeek Chat provider adapter 跑通单轮对话、streaming、tool call 和 usage 映射 |
| M4 | 阶段 4 | reasoning ledger 跑通多轮工具调用，且不明文持久化 reasoning 原文 |
| M5 | 阶段 5 | DeepSeek tool schema、strict beta 路径和结构化工具错误收敛 |
| M6 | 阶段 6 | stable repo prefix 注入和 cache telemetry 可用 |
| M7 | 阶段 7 | DeepSeek model / effort router 可用，并保持显式配置优先 |
| M8 | 阶段 8 | Vision Bridge 自动图片观察和 `describe_image` 工具可用 |
| M9 | 阶段 9 | FIM core API 和 `deepseek-vibing fim` 可用 |
| M10 | 阶段 10 | trajectory log、sandbox tier 和 replay 基线可用 |
| M11 | 阶段 11 | 本地 eval runner、质量报告和质量门槛可用 |
| M12 | 阶段 12 | 大规模源码删减、依赖收敛和 DeepSeek Vibing 品牌整理完成 |

## 7. 近期工作顺序

截至 2026-06-06，阶段 0-11 的主实现路径已经进入可验收状态，阶段 12 正在做保守瘦身和 DeepSeek Vibing 身份整理。

优先级如下：

1. 完成阶段 12 验收：跑 `cargo check`、DeepSeek/Vision/FIM/trajectory/eval 回归测试、CLI help surface 和 dependency tree 检查。
2. 继续清理 README/help 文案中仍主推 OpenAI、ChatGPT、Codex Cloud、Realtime、hosted web/image 的内容。
3. 下一批深层清理再处理 app-server/backend/account/auth/realtime 协议遗留和内部 `deepseek-vibing-*` crate rename。

## 8. 风险与决策点

### 8.1 DeepSeek API 形态差异

Codex 当前主路径绑定 OpenAI Responses API，而 DeepSeek 更适合通过 Chat/Tool API 适配。provider adapter 是项目最大工程风险。

### 8.2 Reasoning content 持久化

reasoning content 应传回模型，但不能展示给用户。是否持久化、如何加密、是否允许关闭，需要单独决策。

### 8.3 代码库裁剪时机

不建议一开始就硬删大量 crates。应先跑通 DeepSeek 主路径，再删除无关代码。

### 8.4 视觉模型选择

DeepSeek 暂不支持多模态，因此需要 Vision Bridge。视觉 provider 应保持可配置，避免把项目重新绑定到某个单一厂商。

### 8.5 长上下文成本

1M context 不是免费资源。Stable Repo Prefix、cache telemetry 和模型路由必须一起设计，否则容易变成高成本低收益。
