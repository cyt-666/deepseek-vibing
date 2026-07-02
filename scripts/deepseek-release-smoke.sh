#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_LIVE=0
RUN_PACKAGE=0

usage() {
  cat <<'EOF'
Usage: scripts/deepseek-release-smoke.sh [--live] [--package]

Runs the local release-readiness smoke suite.

Options:
  --live     Also run real API smoke scripts when local config is present.
  --package  Also build and validate a temporary package directory offline.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --live)
      RUN_LIVE=1
      shift
      ;;
    --package)
      RUN_PACKAGE=1
      shift
      ;;
    --help | -h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

cd "$REPO_ROOT"

step() {
  printf '\n==> %s\n' "$1"
}

run() {
  printf '+'
  for arg in "$@"; do
    printf ' %q' "$arg"
  done
  printf '\n'
  "$@"
}

config_file() {
  local home_dir="${DEEPSEEK_VIBING_HOME:-"$HOME/.deepseek-vibing"}"
  printf '%s/config.toml\n' "$home_dir"
}

has_deepseek_key() {
  [[ -n "${DEEPSEEK_API_KEY:-}" ]] && return 0
  local config
  config="$(config_file)"
  [[ -f "$config" ]] || return 1
  python3 - "$config" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r"(?ms)^\[auth\].*?^deepseek_api_key\s*=\s*\"([^\"]+)\"", text)
raise SystemExit(0 if match and match.group(1).strip() else 1)
PY
}

has_vision_config() {
  [[ -n "${VISION_API_KEY:-}" ]] && local_has_env_key=1 || local_has_env_key=0
  local config
  config="$(config_file)"
  [[ -f "$config" ]] || return 1
  python3 - "$config" "$local_has_env_key" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
has_env_key = sys.argv[2] == "1"

def section(name: str) -> str:
    match = re.search(rf"(?ms)^\[{re.escape(name)}\]\s*(.*?)(?=^\[|\Z)", text)
    return match.group(1) if match else ""

auth = section("auth")
vision = section("vision")
vision_key = re.search(r'^vision_api_key\s*=\s*"([^"]+)"', auth, re.M)
base_url = re.search(r'^base_url\s*=\s*"([^"]+)"', vision, re.M)
model = re.search(r'^model\s*=\s*"([^"]+)"', vision, re.M)

ok = (
    (has_env_key or (vision_key and vision_key.group(1).strip()))
    and base_url
    and base_url.group(1).strip()
    and model
    and model.group(1).strip()
)
raise SystemExit(0 if ok else 1)
PY
}

run_cli() {
  local bin="$REPO_ROOT/deepseek-vibing-rs/target/debug/deepseek-vibing"
  if [[ -x "$bin" ]]; then
    "$bin" "$@"
  else
    cargo run --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing -- "$@"
  fi
}

step "Shell syntax checks"
run bash -n scripts/install/install.sh
run bash -n scripts/deepseek-p0-live-smoke.sh
run bash -n scripts/deepseek-vision-live-smoke.sh
run bash -n scripts/deepseek-fim-live-smoke.sh
run bash -n scripts/deepseek-fim-server-smoke.sh
run bash -n scripts/deepseek-release-smoke.sh

step "Package builder Python tests"
run python3 -m unittest discover -s scripts/deepseek_vibing_package -p "test_*.py"

step "Release packaging residue scan"
residue_pattern='\b[v]8\b|RUSTY_[V]8|[V]8_FROM_SOURCE|[v]8-poc|bin''deepseek-vibing|stage_release''deepseek-vibing|release_dir''deepseek-vibing'
if rg -n "$residue_pattern" scripts .github; then
  echo "Found release packaging residue." >&2
  exit 1
fi

step "Cargo checks"
run cargo check --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli

step "Targeted tests"
run cargo test --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-tools read_file -- --nocapture
run cargo test --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core eval -- --nocapture
run cargo test --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core vision -- --nocapture
run cargo test --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-core fim -- --nocapture
run cargo test --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli eval -- --nocapture
run cargo test --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli fim -- --nocapture

step "CLI smoke"
run_cli --help >/dev/null
run_cli eval list --dataset evals/smoke >/dev/null
run_cli eval list --dataset evals/regression >/dev/null
run_cli fim --help >/dev/null
run_cli fim-server --help >/dev/null

if [[ "$RUN_PACKAGE" -eq 1 ]]; then
  step "Offline package layout smoke"
  rg_bin="$(command -v rg || true)"
  if [[ -z "$rg_bin" ]]; then
    echo "Skipping package smoke: rg is not on PATH." >&2
  else
    cargo build --manifest-path deepseek-vibing-rs/Cargo.toml -p deepseek-vibing-cli --bin deepseek-vibing
    package_dir="$(mktemp -d "${TMPDIR:-/tmp}/deepseek-vibing-package-smoke.XXXXXX")"
    python3 scripts/build_deepseek_vibing_package.py \
      --package-dir "$package_dir" \
      --entrypoint-bin "$REPO_ROOT/deepseek-vibing-rs/target/debug/deepseek-vibing" \
      --rg-bin "$rg_bin" \
      --skip-zsh \
      --force
    "$package_dir/bin/deepseek-vibing" --version >/dev/null
    rm -rf "$package_dir"
  fi
fi

if [[ "$RUN_LIVE" -eq 1 ]]; then
  step "Optional live smoke"
  if has_deepseek_key; then
    run bash scripts/deepseek-p0-live-smoke.sh
    run bash scripts/deepseek-fim-live-smoke.sh
    run bash scripts/deepseek-fim-server-smoke.sh
  else
    echo "Skipping DeepSeek live smoke: no [auth].deepseek_api_key or DEEPSEEK_API_KEY." >&2
  fi

  if has_deepseek_key && has_vision_config; then
    run bash scripts/deepseek-vision-live-smoke.sh
  else
    echo "Skipping Vision live smoke: missing DeepSeek key or Vision config." >&2
  fi
fi

printf '\nDeepSeek release smoke passed.\n'
