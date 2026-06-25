#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEEPSEEK_HOME="${DEEPSEEK_VIBING_HOME:-"$HOME/.deepseek-vibing"}"
CONFIG_FILE="$DEEPSEEK_HOME/config.toml"
KEEP_WORKDIR="${DEEPSEEK_VIBING_P0_KEEP_WORKDIR:-0}"
SKIP_BETA="${DEEPSEEK_VIBING_P0_SKIP_BETA:-0}"
REQUIRE_BETA="${DEEPSEEK_VIBING_P0_REQUIRE_BETA:-0}"

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Missing DeepSeek Vibing config: $CONFIG_FILE" >&2
  echo "Create it with an [auth].deepseek_api_key entry before running this live smoke." >&2
  exit 2
fi

if ! grep -Eq '^[[:space:]]*deepseek_api_key[[:space:]]*=[[:space:]]*"[^"]+"' "$CONFIG_FILE"; then
  echo "Missing [auth].deepseek_api_key in $CONFIG_FILE" >&2
  exit 2
fi

DEEPSEEK_API_KEY_FROM_CONFIG="$(sed -nE 's/^[[:space:]]*deepseek_api_key[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$CONFIG_FILE" | head -n 1)"

if command -v stat >/dev/null 2>&1; then
  mode="$(stat -f '%Lp' "$CONFIG_FILE" 2>/dev/null || stat -c '%a' "$CONFIG_FILE" 2>/dev/null || true)"
  if [[ -n "$mode" && "$mode" != "600" ]]; then
    echo "Warning: $CONFIG_FILE permissions are $mode; 600 is recommended because it may contain API keys." >&2
  fi
fi

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/deepseek-vibing-p0.XXXXXX")"
TRAJECTORY_ROOT="$WORKDIR/trajectories"
STATUS=0

cleanup() {
  if [[ "$STATUS" -eq 0 && "$KEEP_WORKDIR" != "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "P0 live smoke workspace retained at: $WORKDIR" >&2
  fi
}
trap 'STATUS=$?; cleanup; exit $STATUS' EXIT

if [[ -n "${DEEPSEEK_VIBING_BIN:-}" ]]; then
  RUNNER=("$DEEPSEEK_VIBING_BIN")
else
  RUNNER=(
    cargo run
    --manifest-path "$REPO_ROOT/deepseek-vibing-rs/Cargo.toml"
    -p deepseek-vibing-cli
    --bin deepseek-vibing
    --
  )
fi

create_rust_task() {
  local dir="$1"
  mkdir -p "$dir/src"
  cat >"$dir/Cargo.toml" <<'EOF'
[package]
name = "deepseek-vibing-p0-smoke"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
EOF
  cat >"$dir/src/lib.rs" <<'EOF'
pub fn add(left: i32, right: i32) -> i32 {
    left - right
}

#[cfg(test)]
mod tests {
    use super::add;

    #[test]
    fn adds_two_numbers() {
        assert_eq!(add(2, 3), 5);
    }
}
EOF
}

STABLE_TASK="$WORKDIR/stable-task"
create_rust_task "$STABLE_TASK"
mkdir -p "$TRAJECTORY_ROOT"

echo "Running stable DeepSeek live smoke in $STABLE_TASK"
"${RUNNER[@]}" exec \
  --json \
  --skip-git-repo-check \
  --sandbox workspace-write \
  --cd "$STABLE_TASK" \
  -c 'trajectory.enabled="always"' \
  -c 'trajectory.detail="summary"' \
  -c "trajectory.root_dir=\"$TRAJECTORY_ROOT\"" \
  "Fix this Rust crate so cargo test --quiet passes. Read src/lib.rs, patch the bug, run cargo test --quiet, and keep the change minimal."

cargo test --quiet --manifest-path "$STABLE_TASK/Cargo.toml"

TRAJECTORY_FILE="$(find "$TRAJECTORY_ROOT" -type f -name '*.jsonl' -print -quit)"
if [[ -z "$TRAJECTORY_FILE" ]]; then
  echo "Expected summary trajectory JSONL under $TRAJECTORY_ROOT" >&2
  exit 1
fi

if grep -Eq 'ReasoningRawContentDelta|AgentReasoningRawContent|reasoning_content' "$TRAJECTORY_FILE"; then
  echo "Trajectory appears to contain raw reasoning markers: $TRAJECTORY_FILE" >&2
  exit 1
fi

if [[ -n "$DEEPSEEK_API_KEY_FROM_CONFIG" ]] && grep -Fq -- "$DEEPSEEK_API_KEY_FROM_CONFIG" "$TRAJECTORY_FILE"; then
  echo "Trajectory appears to contain the configured DeepSeek API key: $TRAJECTORY_FILE" >&2
  exit 1
fi

if [[ "$SKIP_BETA" != "1" ]]; then
  BETA_TASK="$WORKDIR/beta-task"
  mkdir -p "$BETA_TASK"
  echo "Running optional deepseek-beta strict-tool smoke in $BETA_TASK"
  set +e
  "${RUNNER[@]}" exec \
    --json \
    --skip-git-repo-check \
    --sandbox workspace-write \
    --cd "$BETA_TASK" \
    -c 'model_provider="deepseek-beta"' \
    -c 'trajectory.enabled="always"' \
    -c 'trajectory.detail="summary"' \
    -c "trajectory.root_dir=\"$TRAJECTORY_ROOT\"" \
    "Create a file named beta_tool_check.txt containing exactly ok, then read it back."
  beta_status=$?
  set -e
  if [[ "$beta_status" -ne 0 ]]; then
    echo "Warning: deepseek-beta smoke failed with status $beta_status; stable smoke already passed." >&2
    if [[ "$REQUIRE_BETA" == "1" ]]; then
      exit "$beta_status"
    fi
  fi
fi

echo "P0 live smoke passed. Trajectory: $TRAJECTORY_FILE"
