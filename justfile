set working-directory := "deepseek-vibing-rs"
set positional-arguments
export JUST_SHELL := justfile_directory() / "scripts/just-shell.py"
set shell := ["python3", "-c", 'import os, runpy; runpy.run_path(os.environ["JUST_SHELL"], run_name="__main__")']
set windows-shell := ["python", "-c", 'import os, runpy; runpy.run_path(os.environ["JUST_SHELL"], run_name="__main__")']

rust_min_stack := "8388608" # 8 MiB
python := if os_family() == "windows" { "python" } else { "python3" }

# Display help
help:
    just -l

# `deepseek-vibing`
alias dsv := deepseek-vibing
deepseek-vibing *args:
    cargo run --bin deepseek-vibing -- {args}

# `deepseek-vibing exec`
exec *args:
    cargo run --bin deepseek-vibing -- exec {args}

# Start `deepseek-vibing exec-server` and run deepseek-vibing-tui.
[no-cd]
[positional-arguments]
[unix]
tui-with-exec-server *args:
    {{ justfile_directory() }}/scripts/run_tui_with_exec_server.sh "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    cargo run --bin deepseek-vibing-file-search -- {args}

# Build the CLI and run the app-server test client
app-server-test-client *args:
    cargo build -p deepseek-vibing-cli
    cargo run -p deepseek-vibing-app-server-test-client -- --deepseek-vibing-bin ./target/debug/deepseek-vibing {args}

# Format the justfile, Rust, Python SDK code, and Python scripts.
fmt:
    {{ python }} ../scripts/format.py

# Check formatting without modifying files.
fmt-check:
    {{ python }} ../scripts/format.py --check

fix *args:
    cargo clippy --fix --tests --allow-dirty {args}

clippy *args:
    cargo clippy --tests {args}

[unix]
install:
    rustup show active-toolchain
    cargo fetch

[windows]
install:
    #!powershell.exe -File
    $pwsh = Get-Command pwsh.exe -ErrorAction SilentlyContinue
    if (-not $pwsh) {
        winget install --exact --id Microsoft.PowerShell --source winget --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    rustup show active-toolchain
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo fetch
    exit $LASTEXITCODE

# Run nextest with --no-fail-fast so all tests are run.
#
# Run `cargo install --locked cargo-nextest` if you don't have it installed.
# Prefer this for routine local runs. Workspace crate features are banned, so
# there should be no need to add `--all-features`.
[unix]
test *args:
    RUST_MIN_STACK={{ rust_min_stack }} cargo nextest run --no-fail-fast "$@"
    just bench-smoke

[windows]
test *args:
    $env:RUST_MIN_STACK = "{{ rust_min_stack }}"; cargo nextest run --no-fail-fast @($args | Select-Object -Skip 1)
    just bench-smoke

# Run explicit workspace benchmark targets.
bench *args:
    cargo bench --workspace --bench '*' {args}

# Run benchmark targets once to ensure they start successfully.
bench-smoke:
    just bench -- --test

# Build and run DeepSeek Vibing from source using Bazel.
# On Unix, use `[no-cd]` and `--run_under="cd $PWD &&"` to ensure Bazel runs
# the command in the current working directory.
[no-cd]
[unix]
bazel-deepseek-vibing *args:
    bazel run //deepseek-vibing-rs/cli:deepseek-vibing --run_under="cd $PWD &&" -- "$@"

[windows]
bazel-deepseek-vibing *args:
    bazel run //deepseek-vibing-rs/cli:deepseek-vibing --run_under='cd /d "{{ invocation_directory_native() }}" &&' -- @($args | Select-Object -Skip 1)

[no-cd]
bazel-lock-update:
    bazel mod deps --lockfile_mode=update

[no-cd]
[unix]
bazel-lock-check:
    {{ justfile_directory() }}/scripts/check-module-bazel-lock.sh

[windows]
bazel-lock-check:
    bazel mod deps --lockfile_mode=error; if ($LASTEXITCODE -ne 0) { Write-Error "MODULE.bazel.lock is out of date. Run 'just bazel-lock-update' and commit the updated lockfile."; exit 1 }

bazel-test:
    bazel test --test_tag_filters=-argument-comment-lint //... --keep_going

[no-cd]
[unix]
bazel-clippy:
    bazel_targets="$({{ justfile_directory() }}/scripts/list-bazel-clippy-targets.sh)" && bazel build --config=clippy -- ${bazel_targets}

[no-cd]
[unix]
bazel-argument-comment-lint:
    bazel build --config=argument-comment-lint -- $({{ justfile_directory() }}/tools/argument-comment-lint/list-bazel-targets.sh)

bazel-remote-test:
    bazel test --test_tag_filters=-argument-comment-lint //... --config=remote --platforms=//:rbe --keep_going

build-for-release:
    bazel build //deepseek-vibing-rs/cli:release_binaries --config=remote

# Run the MCP server
mcp-server-run *args:
    cargo run -p deepseek-vibing-mcp-server -- {args}

# Regenerate the json schema for config.toml from the current config types.
write-config-schema:
    cargo run -p deepseek-vibing-core --bin deepseek-vibing-write-config-schema

# Regenerate vendored app-server protocol schema artifacts.
write-app-server-schema *args:
    cargo run -p deepseek-vibing-app-server-protocol --bin write_schema_fixtures -- {args}

[no-cd]
write-hooks-schema:
    cargo run --manifest-path {{ justfile_directory() }}/deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-hooks --bin write_hooks_schema_fixtures

# Run the argument-comment Dylint checks across deepseek-vibing-rs.
[no-cd]
[unix]
argument-comment-lint *args:
    if [ "$#" -eq 0 ]; then \
      bazel build --config=argument-comment-lint -- $({{ justfile_directory() }}/tools/argument-comment-lint/list-bazel-targets.sh); \
    else \
      {{ justfile_directory() }}/tools/argument-comment-lint/run-prebuilt-linter.py "$@"; \
    fi

[no-cd]
argument-comment-lint-from-source *args:
    {{ python }} {{ justfile_directory() }}/tools/argument-comment-lint/run.py {args}

# Tail logs from the state SQLite database
[unix]
log *args:
    if [ "${1:-}" = "--" ]; then shift; fi; cargo run -p deepseek-vibing-state --bin logs_client -- "$@"

[windows]
log *args:
    $forwarded_args = @($args | Select-Object -Skip 1); if ($forwarded_args.Count -gt 0 -and $forwarded_args[0] -eq "--") { $forwarded_args = @($forwarded_args | Select-Object -Skip 1) }; cargo run -p deepseek-vibing-state --bin logs_client -- @forwarded_args
