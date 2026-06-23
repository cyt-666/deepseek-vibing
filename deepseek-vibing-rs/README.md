# DeepSeek Vibing Rust Workspace

This directory contains the Rust workspace inherited from DeepSeekVibing and adapted into the DeepSeek Vibing local agent baseline. The workspace still preserves many internal `deepseek-vibing-*` crate names, but the default provider, CLI surface, and new subsystems are DeepSeek-first.

## Current Baseline

- CLI binary name: `deepseek-vibing`.
- Primary package crate name: `deepseek-vibing-cli`, intentionally preserved for now.
- Internal crates still use `deepseek-vibing-*` names.
- OpenAI/ChatGPT cloud-only CLI entries and hosted web/image/realtime defaults are removed or hidden from the active product surface.
- DeepSeek Chat/tool/reasoning, Vision Bridge, FIM, trajectory, and eval paths are expected to remain buildable.

## Common Commands

Check the CLI package:

```shell
cargo check -p deepseek-vibing-cli
```

Run the CLI:

```shell
cargo run -p deepseek-vibing-cli --bin deepseek-vibing -- --help
```

Run from the repository root with an explicit manifest path:

```shell
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- --help
```

## Workspace Notes

The current implementation intentionally avoids full internal renaming. This keeps the dependency graph stable while Stage 12 removes direct cloud-only CLI dependencies and prepares a later deep cleanup pass.
