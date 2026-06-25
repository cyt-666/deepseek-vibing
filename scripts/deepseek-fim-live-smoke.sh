#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEEPSEEK_HOME="${DEEPSEEK_VIBING_HOME:-"$HOME/.deepseek-vibing"}"
CONFIG_FILE="$DEEPSEEK_HOME/config.toml"
KEEP_WORKDIR="${DEEPSEEK_VIBING_FIM_KEEP_WORKDIR:-0}"

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Missing DeepSeek Vibing config: $CONFIG_FILE" >&2
  exit 2
fi

if ! grep -Eq '^[[:space:]]*deepseek_api_key[[:space:]]*=[[:space:]]*"[^"]+"' "$CONFIG_FILE" \
  && [[ -z "${DEEPSEEK_API_KEY:-}" ]]; then
  echo "Missing [auth].deepseek_api_key in $CONFIG_FILE or DEEPSEEK_API_KEY" >&2
  exit 2
fi

if command -v stat >/dev/null 2>&1; then
  mode="$(stat -f '%Lp' "$CONFIG_FILE" 2>/dev/null || stat -c '%a' "$CONFIG_FILE" 2>/dev/null || true)"
  if [[ -n "$mode" && "$mode" != "600" ]]; then
    echo "Warning: $CONFIG_FILE permissions are $mode; 600 is recommended because it may contain API keys." >&2
  fi
fi

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/deepseek-vibing-fim.XXXXXX")"
PREFIX_FILE="$WORKDIR/prefix.rs"
SUFFIX_FILE="$WORKDIR/suffix.rs"
OUTPUT_JSON="$WORKDIR/fim-output.json"
STDERR_LOG="$WORKDIR/fim-stderr.log"
STATUS=0

cleanup() {
  if [[ "$STATUS" -eq 0 && "$KEEP_WORKDIR" != "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "FIM live smoke workspace retained at: $WORKDIR" >&2
  fi
}
trap 'STATUS=$?; cleanup; exit $STATUS' EXIT

cat >"$PREFIX_FILE" <<'EOF'
pub fn add(left: i32, right: i32) -> i32 {
    left
EOF

cat >"$SUFFIX_FILE" <<'EOF'
 right
}
EOF

if [[ -n "${DEEPSEEK_VIBING_BIN:-}" ]]; then
  RUNNER=("$DEEPSEEK_VIBING_BIN")
else
  DEFAULT_BIN="$REPO_ROOT/deepseek-vibing-rs/target/debug/deepseek-vibing"
  if [[ -x "$DEFAULT_BIN" ]]; then
    RUNNER=("$DEFAULT_BIN")
  else
    RUNNER=(
      cargo run
      --manifest-path "$REPO_ROOT/deepseek-vibing-rs/Cargo.toml"
      -p deepseek-vibing-cli
      --bin deepseek-vibing
      --
    )
  fi
fi

echo "Running DeepSeek FIM live smoke in $WORKDIR"
set +e
"${RUNNER[@]}" fim \
  --prefix-file "$PREFIX_FILE" \
  --suffix-file "$SUFFIX_FILE" \
  --max-tokens 32 \
  --temperature 0 \
  --json \
  >"$OUTPUT_JSON" \
  2>"$STDERR_LOG"
fim_status=$?
set -e

if [[ "$fim_status" -ne 0 ]]; then
  echo "FIM live smoke failed with status $fim_status" >&2
  sed -E 's/sk-[A-Za-z0-9._-]+/[REDACTED]/g' "$STDERR_LOG" >&2 || true
  exit "$fim_status"
fi

python3 - "$OUTPUT_JSON" "$STDERR_LOG" <<'PY'
import json
import re
import sys

output_path, stderr_path = sys.argv[1:3]
with open(output_path, "r", encoding="utf-8") as handle:
    raw_output = handle.read()
with open(stderr_path, "r", encoding="utf-8") as handle:
    raw_stderr = handle.read()

combined = raw_output + "\n" + raw_stderr
if re.search(r"sk-[A-Za-z0-9._-]+", combined):
    raise SystemExit("FIM output appears to contain an API key")

try:
    payload = json.loads(raw_output)
except json.JSONDecodeError as exc:
    raise SystemExit(f"FIM output is not valid JSON: {exc}\n{raw_output}") from exc

text = payload.get("text")
if not isinstance(text, str) or not text.strip():
    raise SystemExit(f"FIM completion text is empty: {payload!r}")
if "+" not in text:
    raise SystemExit(f"Expected FIM completion to contain '+', got: {text!r}")

model = payload.get("model")
if not isinstance(model, str) or not model:
    raise SystemExit(f"FIM response did not include model: {payload!r}")

usage = payload.get("usage")
if usage is None:
    raise SystemExit(f"FIM response did not include usage: {payload!r}")
for key in (
    "prompt_tokens",
    "completion_tokens",
    "prompt_cache_hit_tokens",
    "prompt_cache_miss_tokens",
    "total_tokens",
):
    value = usage.get(key)
    if not isinstance(value, int) or value < 0:
        raise SystemExit(f"FIM usage.{key} is invalid: {usage!r}")

if usage["completion_tokens"] == 0:
    raise SystemExit(f"FIM completion_tokens should be non-zero: {usage!r}")

print(
    "FIM JSON ok: "
    f"model={model} "
    f"text={text!r} "
    f"prompt_tokens={usage['prompt_tokens']} "
    f"completion_tokens={usage['completion_tokens']} "
    f"cache_hit={usage['prompt_cache_hit_tokens']} "
    f"cache_miss={usage['prompt_cache_miss_tokens']}"
)
PY

echo "DeepSeek FIM live smoke passed."
