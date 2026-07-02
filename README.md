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

Create `~/.deepseek-vibing/config.toml`. See [docs/config.md](./docs/config.md)
for the full recommended configuration.

```toml
model_provider = "deepseek"
model = "deepseek-v4-flash"

[auth]
deepseek_api_key = "sk-..."
vision_api_key = ""
```

On Unix-like systems, the config may contain API keys and should be private:

```shell
chmod 600 ~/.deepseek-vibing/config.toml
```

## Development

Start with [docs/getting-started.md](./docs/getting-started.md), then use the
install and configuration references:

- [docs/install.md](./docs/install.md)
- [docs/config.md](./docs/config.md)
- [docs/authentication.md](./docs/authentication.md)

Build and check the Rust CLI workspace:

```shell
cargo check --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli
```

Run the CLI help after building:

```shell
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- --help
```

Run the local release smoke checklist:

```shell
scripts/deepseek-release-smoke.sh
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

Vision Bridge converts local screenshots, UI error images, repository images,
and data URL images into structured text observations before the DeepSeek model
sees the turn. Cached observations live under the DeepSeek Vibing home by default
and can be bounded with the `[vision]` cache settings above.

Run the DeepSeek FIM live smoke against `/beta/completions`:

```shell
scripts/deepseek-fim-live-smoke.sh
```

Run the local FIM server smoke:

```shell
scripts/deepseek-fim-server-smoke.sh
```

`deepseek-vibing fim-server` starts a local HTTP endpoint for editor experiments.
It prints startup JSON containing `host` and `port`, then accepts
`POST /complete` with `prefix`, optional `suffix`, optional `file_path`,
`max_tokens`, `temperature`, and `request_id`. The service reuses the FIM config
under `[deepseek.fim]`, applies UTF-8-safe prefix/suffix truncation, keeps an
in-memory completion cache, and never applies completions to files.

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
