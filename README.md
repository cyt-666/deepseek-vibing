# DeepSeek Vibing

DeepSeek Vibing is a local coding agent forked from the open-source Codex codebase and adapted for DeepSeek models.

This repository is now a DeepSeek-first local coding agent baseline. It keeps the mature CLI/TUI, local tool loop, patch application, shell execution, approval flow, sandboxing, MCP foundations, Vision Bridge, FIM, trajectory, and eval paths while pruning OpenAI/ChatGPT cloud-only surfaces from the default product.

## Current Scope

- Local CLI/TUI first.
- Standard single-file configuration at `~/.deepseek-vibing/config.toml`, with `DEEPSEEK_VIBING_HOME` as the home override.
- DeepSeek API key configuration via `[auth].deepseek_api_key`; `DEEPSEEK_API_KEY` remains only as a lower-priority CI fallback.
- No OpenAI or ChatGPT login requirement for the intended MVP; legacy `auth.json`, browser/device login, and ChatGPT account flows are not part of DeepSeek Vibing.
- DeepSeek Chat/tool/reasoning path, Vision Bridge, FIM, trajectory, and eval are part of the active baseline.
- Internal Rust packages and helper binaries use the `deepseek-vibing-*` product prefix.

## Configuration

Create `~/.deepseek-vibing/config.toml`:

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

[trajectory]
enabled = "auto"
detail = "summary"

[eval.pricing]
currency = "USD"

[eval.pricing.models.deepseek-v4-flash]
cache_hit_input_per_million_tokens = 0.0
cache_miss_input_per_million_tokens = 0.0
output_per_million_tokens = 0.0
reasoning_output_per_million_tokens = 0.0
```

On Unix-like systems, the config may contain API keys and should be private:

```shell
chmod 600 ~/.deepseek-vibing/config.toml
```

## Development

Build and check the Rust CLI workspace:

```shell
cargo check --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli
```

Run the CLI help after building:

```shell
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- --help
```

Run the P0 live smoke against your real local config:

```shell
scripts/deepseek-p0-live-smoke.sh
```

Run the Vision Bridge live smoke after configuring `[auth].vision_api_key`,
`[vision].base_url`, and `[vision].model`:

```shell
scripts/deepseek-vision-live-smoke.sh
```

Run the DeepSeek FIM live smoke against `/beta/completions`:

```shell
scripts/deepseek-fim-live-smoke.sh
```

List eval tasks and run a baseline-backed report:

```shell
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- eval list --dataset evals/smoke
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- eval run --dataset evals/smoke --mode offline
```

`evals/regression` contains optional broader coding tasks for local or live regression runs.
Use the committed baseline for live or CI checks where tasks are actually attempted:

```shell
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- eval run --dataset evals/smoke --baseline evals/baselines/smoke-baseline.json --mode live --enforce
```

Cost estimates are only calculated when `[eval.pricing]` supplies per-model token prices;
DeepSeek Vibing does not hard-code provider pricing or inspect billing data.

See [PROJECT_PLAN.md](./PROJECT_PLAN.md) for the staged migration plan.

## License

This project keeps the upstream Apache-2.0 license inherited from Codex. See [LICENSE](./LICENSE).
