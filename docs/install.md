# Installing And Building

## System Requirements

| Requirement | Details |
| --- | --- |
| Operating systems | macOS 12+, Ubuntu 20.04+/Debian 10+, Windows 11 |
| Git | 2.23+ recommended |
| RAM | 4 GB minimum, 8 GB recommended |
| Config | `~/.deepseek-vibing/config.toml` |

## Standalone Installer

Released builds are installed under the DeepSeek Vibing home and exposed through
a visible `deepseek-vibing` command.

Unix-like systems:

```shell
curl -fsSL https://raw.githubusercontent.com/deepseek-vibing/deepseek-vibing/main/scripts/install/install.sh | sh
```

Windows PowerShell:

```powershell
iwr https://raw.githubusercontent.com/deepseek-vibing/deepseek-vibing/main/scripts/install/install.ps1 -UseB | iex
```

Useful environment variables:

```shell
DEEPSEEK_VIBING_HOME="$HOME/.deepseek-vibing"
DEEPSEEK_VIBING_INSTALL_DIR="$HOME/.local/bin"
DEEPSEEK_VIBING_RELEASE="latest"
DEEPSEEK_VIBING_NON_INTERACTIVE="true"
```

The installer expects package archives with this layout:

```text
.
├── deepseek-vibing-package.json
├── bin
│   └── deepseek-vibing[.exe]
├── deepseek-vibing-path
│   └── rg[.exe]
└── deepseek-vibing-resources
    ├── bwrap                         # Linux only
    └── deepseek-vibing-*.exe         # Windows helper binaries
```

## Build From Source

```shell
git clone https://github.com/deepseek-vibing/deepseek-vibing.git
cd deepseek-vibing

cargo check --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli
cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- --help
```

For a debug binary:

```shell
cargo build --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing
./deepseek-vibing-rs/target/debug/deepseek-vibing --version
```

For a release binary:

```shell
cargo build --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing --release
```

## Package Builder

The package builder stages the canonical release layout and can write archives:

```shell
python3 scripts/build_deepseek_vibing_package.py \
  --package-dir /tmp/deepseek-vibing-package \
  --entrypoint-bin ./deepseek-vibing-rs/target/debug/deepseek-vibing \
  --rg-bin "$(command -v rg)" \
  --skip-zsh \
  --force
```

Release jobs should use `--cargo-profile release` or pass signed/notarized
prebuilt binaries through `--entrypoint-bin` and the resource-specific flags.

## Release Smoke

Use the local release checklist before publishing:

```shell
scripts/deepseek-release-smoke.sh
```

Optional checks:

```shell
scripts/deepseek-release-smoke.sh --package
scripts/deepseek-release-smoke.sh --live
```

`--live` can make real DeepSeek and Vision provider API calls. Missing live
configuration is reported as skipped, not as a local build failure.

## Logging

DeepSeek Vibing honors `RUST_LOG`. The TUI records diagnostics in bounded local
stores by default. Set `log_dir` explicitly to enable a plaintext TUI log:

```shell
deepseek-vibing -c log_dir=./.deepseek-vibing-log
tail -F ./.deepseek-vibing-log/deepseek-vibing-tui.log
```
