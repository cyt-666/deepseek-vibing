#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEEP_WORKDIR="${DEEPSEEK_VIBING_FIM_SERVER_KEEP_WORKDIR:-0}"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/deepseek-vibing-fim-server.XXXXXX")"
SERVER_STDOUT="$WORKDIR/server-stdout.log"
SERVER_STDERR="$WORKDIR/server-stderr.log"
RESPONSE_JSON="$WORKDIR/response.json"
STATUS=0
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [[ "$STATUS" -eq 0 && "$KEEP_WORKDIR" != "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "FIM server smoke workspace retained at: $WORKDIR" >&2
  fi
}
trap 'STATUS=$?; cleanup; exit $STATUS' EXIT

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

echo "Starting DeepSeek FIM server smoke in $WORKDIR"
"${RUNNER[@]}" fim-server >"$SERVER_STDOUT" 2>"$SERVER_STDERR" &
SERVER_PID=$!

for _ in $(seq 1 100); do
  if [[ -s "$SERVER_STDOUT" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "FIM server exited before startup output" >&2
    sed -E 's/sk-[A-Za-z0-9._-]+/[REDACTED]/g' "$SERVER_STDERR" >&2 || true
    exit 1
  fi
  sleep 0.1
done

if [[ ! -s "$SERVER_STDOUT" ]]; then
  echo "FIM server did not print startup JSON" >&2
  sed -E 's/sk-[A-Za-z0-9._-]+/[REDACTED]/g' "$SERVER_STDERR" >&2 || true
  exit 1
fi

STARTUP_JSON="$(head -n 1 "$SERVER_STDOUT")"
read -r HOST PORT < <(python3 - "$STARTUP_JSON" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["host"], payload["port"])
PY
)

python3 - "$HOST" "$PORT" "$RESPONSE_JSON" <<'PY'
import json
import re
import sys
import urllib.error
import urllib.request

host, port, response_path = sys.argv[1:4]
body = json.dumps({
    "request_id": "fim-server-smoke",
    "prefix": "pub fn add(left: i32, right: i32) -> i32 {\n    left",
    "suffix": " right\n}\n",
    "max_tokens": 32,
    "temperature": 0,
}).encode("utf-8")
request = urllib.request.Request(
    f"http://{host}:{port}/complete",
    data=body,
    headers={"Content-Type": "application/json"},
    method="POST",
)

try:
    with urllib.request.urlopen(request, timeout=120) as response:
        raw = response.read().decode("utf-8")
except urllib.error.HTTPError as exc:
    raw = exc.read().decode("utf-8")

with open(response_path, "w", encoding="utf-8") as handle:
    handle.write(raw)

if re.search(r"sk-[A-Za-z0-9._-]+", raw):
    raise SystemExit("FIM server response appears to contain an API key")

payload = json.loads(raw)
if payload.get("ok") is True:
    text = payload.get("text")
    if not isinstance(text, str) or not text.strip():
        raise SystemExit(f"FIM server success response has empty text: {payload!r}")
    print(f"FIM server JSON ok: text={text!r} cached={payload.get('cached')} coalesced={payload.get('coalesced')}")
elif payload.get("ok") is False:
    error = payload.get("error")
    if not isinstance(error, dict) or not error.get("kind") or not error.get("message"):
        raise SystemExit(f"FIM server error response is not structured: {payload!r}")
    print(f"FIM server structured error ok: kind={error['kind']} message={error['message']!r}")
else:
    raise SystemExit(f"FIM server response missing ok boolean: {payload!r}")
PY

COMBINED="$(cat "$SERVER_STDOUT" "$SERVER_STDERR" "$RESPONSE_JSON")"
if grep -Eq 'sk-[A-Za-z0-9._-]+' <<<"$COMBINED"; then
  echo "FIM server smoke output appears to contain an API key" >&2
  exit 1
fi

echo "DeepSeek FIM server smoke passed."
