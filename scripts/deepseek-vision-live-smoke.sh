#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEEPSEEK_HOME="${DEEPSEEK_VIBING_HOME:-"$HOME/.deepseek-vibing"}"
CONFIG_FILE="$DEEPSEEK_HOME/config.toml"
KEEP_WORKDIR="${DEEPSEEK_VIBING_VISION_KEEP_WORKDIR:-0}"

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Missing DeepSeek Vibing config: $CONFIG_FILE" >&2
  exit 2
fi

python3 - "$CONFIG_FILE" <<'PY'
import os
import sys

config_path = sys.argv[1]

def parse_minimal_toml(path):
    config = {}
    section = []
    with open(path, "r", encoding="utf-8") as handle:
        for raw_line in handle:
            line = raw_line.split("#", 1)[0].strip()
            if not line:
                continue
            if line.startswith("[") and line.endswith("]"):
                section = [part.strip('"') for part in line.strip("[]").split(".")]
                cursor = config
                for part in section:
                    cursor = cursor.setdefault(part, {})
                continue
            if "=" not in line:
                continue
            key, value = line.split("=", 1)
            key = key.strip()
            value = value.strip()
            if value.startswith('"') and value.endswith('"'):
                parsed = value[1:-1]
            elif value.lower() == "true":
                parsed = True
            elif value.lower() == "false":
                parsed = False
            else:
                parsed = value
            cursor = config
            for part in section:
                cursor = cursor.setdefault(part, {})
            cursor[key] = parsed
    return config

config = parse_minimal_toml(config_path)

auth = config.get("auth") or {}
vision = config.get("vision") or {}
api_key_env = vision.get("api_key_env") or "VISION_API_KEY"
configured_key = (auth.get("vision_api_key") or "").strip()
env_key = (os.environ.get(api_key_env) or "").strip()
base_url = (vision.get("base_url") or "https://example.com/v1").strip().rstrip("/")
model = (vision.get("model") or "vision-model").strip()
enabled = vision.get("enabled", True)

errors = []
if not enabled:
    errors.append("[vision].enabled must be true")
if not configured_key and not env_key:
    errors.append(f"set [auth].vision_api_key or {api_key_env}")
if not base_url or base_url == "https://example.com/v1":
    errors.append("set [vision].base_url to a real OpenAI-compatible vision endpoint")
if not model or model == "vision-model":
    errors.append("set [vision].model to a real vision-capable model")

if errors:
    print("Vision live smoke is not configured:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    print(f"Config file: {config_path}", file=sys.stderr)
    sys.exit(2)
PY

if command -v stat >/dev/null 2>&1; then
  mode="$(stat -f '%Lp' "$CONFIG_FILE" 2>/dev/null || stat -c '%a' "$CONFIG_FILE" 2>/dev/null || true)"
  if [[ -n "$mode" && "$mode" != "600" ]]; then
    echo "Warning: $CONFIG_FILE permissions are $mode; 600 is recommended because it may contain API keys." >&2
  fi
fi

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/deepseek-vibing-vision.XXXXXX")"
TRAJECTORY_ROOT="$WORKDIR/trajectories"
IMAGE_FILE="$WORKDIR/red-square.png"
OUTPUT_LOG="$WORKDIR/vision-output.jsonl"
STATUS=0

cleanup() {
  if [[ "$STATUS" -eq 0 && "$KEEP_WORKDIR" != "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "Vision live smoke workspace retained at: $WORKDIR" >&2
  fi
}
trap 'STATUS=$?; cleanup; exit $STATUS' EXIT

python3 - "$IMAGE_FILE" <<'PY'
import struct
import sys
import zlib

path = sys.argv[1]
width = 64
height = 64

def chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )

row = b"\x00" + bytes([230, 0, 0]) * width
raw = row * height
png = (
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(raw, 9))
    + chunk(b"IEND", b"")
)
with open(path, "wb") as handle:
    handle.write(png)
PY

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

mkdir -p "$TRAJECTORY_ROOT"

echo "Running Vision Bridge live smoke in $WORKDIR"
set +e
"${RUNNER[@]}" exec \
  --json \
  --skip-git-repo-check \
  --sandbox workspace-write \
  --cd "$WORKDIR" \
  --image "$IMAGE_FILE" \
  -c 'trajectory.enabled="always"' \
  -c 'trajectory.detail="summary"' \
  -c "trajectory.root_dir=\"$TRAJECTORY_ROOT\"" \
  "Use the attached image observation only. What is the dominant color in the image? Reply in one short sentence containing the color name." \
  2>&1 | tee "$OUTPUT_LOG"
exec_status=${PIPESTATUS[0]}
set -e

if [[ "$exec_status" -ne 0 ]]; then
  echo "Vision live smoke failed with status $exec_status" >&2
  exit "$exec_status"
fi

if grep -Eiq 'Vision observation failed|missing vision API key|unsupported_provider|vision provider returned|request_error' "$OUTPUT_LOG"; then
  echo "Vision Bridge reported a provider/configuration failure." >&2
  exit 1
fi

if ! grep -Eiq '\bred\b' "$OUTPUT_LOG"; then
  echo "Expected final output to identify the red image." >&2
  exit 1
fi

if grep -Eiq 'data:image|iVBOR|provider_image_url|raw image' "$OUTPUT_LOG"; then
  echo "Output appears to contain raw image data or raw image markers." >&2
  exit 1
fi

TRAJECTORY_FILE="$(find "$TRAJECTORY_ROOT" -type f -name '*.jsonl' -print -quit)"
if [[ -z "$TRAJECTORY_FILE" ]]; then
  echo "Expected summary trajectory JSONL under $TRAJECTORY_ROOT" >&2
  exit 1
fi

if grep -Eq 'ReasoningRawContentDelta|AgentReasoningRawContent|reasoning_content|data:image|iVBOR' "$TRAJECTORY_FILE"; then
  echo "Trajectory appears to contain raw reasoning or raw image data: $TRAJECTORY_FILE" >&2
  exit 1
fi

echo "Vision Bridge live smoke passed. Trajectory: $TRAJECTORY_FILE"
