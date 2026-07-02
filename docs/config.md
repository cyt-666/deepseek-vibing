# Configuration

DeepSeek Vibing uses one standard local config file:

```text
~/.deepseek-vibing/config.toml
```

Set `DEEPSEEK_VIBING_HOME=/absolute/path` to move the whole DeepSeek Vibing
home. This changes where config, trajectories, eval reports, package installs,
and caches live.

## Recommended Config

```toml
model_provider = "deepseek"
model = "deepseek-v4-flash"

[auth]
deepseek_api_key = "sk-..."
vision_api_key = ""

[vision]
enabled = true
provider = "openai_compatible"
model = "<your-vision-model>"
base_url = "https://your-vision-provider.example/v1"
cache = true
cache_max_entries = 500
cache_max_bytes = 104857600
# cache_dir = "/absolute/path/to/vision-cache"
# cache_ttl_seconds = 604800

[deepseek.fim]
enabled = true
model = "deepseek-v4-pro"
base_url = "https://api.deepseek.com/beta"
max_tokens = 512
timeout_ms = 60000
temperature = 0.2
max_prefix_bytes = 120000
max_suffix_bytes = 60000
debounce_ms = 150

[trajectory]
enabled = "auto"
detail = "summary"

[eval]
default_mode = "hybrid"

[eval.gates]
min_pass_rate = 0.80
max_avg_turns = 8
max_tool_error_rate = 0.20

[eval.pricing]
currency = "USD"

[eval.pricing.models.deepseek-v4-flash]
cache_hit_input_per_million_tokens = 0.0
cache_miss_input_per_million_tokens = 0.0
output_per_million_tokens = 0.0
reasoning_output_per_million_tokens = 0.0

[eval.pricing.models.deepseek-v4-pro]
cache_hit_input_per_million_tokens = 0.0
cache_miss_input_per_million_tokens = 0.0
output_per_million_tokens = 0.0
reasoning_output_per_million_tokens = 0.0
```

## Auth Priority

For DeepSeek requests, `[auth].deepseek_api_key` wins. `DEEPSEEK_API_KEY` is
only a lower-priority CI fallback.

For Vision Bridge, `[auth].vision_api_key` wins. `VISION_API_KEY` is only a
fallback.

The config file may contain secrets. On Unix-like systems:

```shell
chmod 700 ~/.deepseek-vibing
chmod 600 ~/.deepseek-vibing/config.toml
```

## Vision Bridge

DeepSeek models are treated as text/code models. Images are first sent to the
configured OpenAI-compatible vision provider, converted into structured
observations, and then injected into the DeepSeek turn. DeepSeek does not receive
raw images, base64 image data, or local image URLs.

The default cache lives under the DeepSeek Vibing home as `vision-cache`.
`cache_max_entries`, `cache_max_bytes`, and `cache_ttl_seconds` bound the cache.

## Trajectory

`[trajectory] enabled = "auto"` writes summary trajectories for DeepSeek
sessions. `detail = "summary"` records tool calls, usage/cache stats, sandbox
state, command summaries, and diffs without raw reasoning text.

## Eval

`evals/smoke` is the default small dataset. `evals/regression` is optional and
broader. Baseline-backed checks use:

```shell
deepseek-vibing eval run --dataset evals/smoke \
  --baseline evals/baselines/smoke-baseline.json \
  --mode live \
  --enforce
```

Cost estimates are calculated only when `[eval.pricing]` is configured. DeepSeek
Vibing does not hard-code provider prices or inspect billing data.

## Lifecycle Hooks

Admins can set top-level `allow_managed_hooks_only = true` in
`requirements.toml` to ignore user, project, and session hook configs while
still allowing managed hooks from requirements and managed config layers. This
setting is only supported in `requirements.toml`; putting it in `config.toml`
does not enable managed-hooks-only mode.
