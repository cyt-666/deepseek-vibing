# Getting Started

DeepSeek Vibing is a DeepSeek-first local coding agent. The first release path
is local CLI/TUI usage with a single config file, local tools, Vision Bridge,
FIM, trajectory, and eval support.

## 1. Install Or Build

For released binaries, use the standalone installer documented in
[install.md](./install.md). For local development, build the Rust CLI:

```shell
cargo check --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- --help
```

## 2. Configure DeepSeek

Create `~/.deepseek-vibing/config.toml` and keep it private:

```shell
mkdir -p ~/.deepseek-vibing
chmod 700 ~/.deepseek-vibing
$EDITOR ~/.deepseek-vibing/config.toml
chmod 600 ~/.deepseek-vibing/config.toml
```

Minimum config:

```toml
model_provider = "deepseek"
model = "deepseek-v4-flash"

[auth]
deepseek_api_key = "sk-..."
vision_api_key = ""
```

Set `DEEPSEEK_VIBING_HOME` only when you want the home directory somewhere other
than `~/.deepseek-vibing`. `DEEPSEEK_API_KEY` remains a lower-priority CI
fallback; normal users should put the key in `[auth].deepseek_api_key`.

## 3. Run

Interactive TUI:

```shell
deepseek-vibing
```

Non-interactive task:

```shell
deepseek-vibing exec "explain this repository"
```

Local FIM completion:

```shell
deepseek-vibing fim --prefix-file prefix.rs --suffix-file suffix.rs
deepseek-vibing fim-server
```

Eval task listing:

```shell
deepseek-vibing eval list --dataset evals/smoke
deepseek-vibing eval list --dataset evals/regression
```

## 4. Verify A Local Checkout

For development and release preparation:

```shell
scripts/deepseek-release-smoke.sh
```

Live API checks are opt-in:

```shell
scripts/deepseek-release-smoke.sh --live
```

See [config.md](./config.md) for Vision, trajectory, eval pricing, and FIM
server settings.
