#!/bin/bash

# Set the DeepSeek Vibing extension CLI executable to this script in VSCode settings
# to always get the latest deepseek-vibing-rs binary when debugging.


set -euo pipefail

DEEPSEEK_VIBING_RS_DIR=$(realpath "$(dirname "$0")/../deepseek-vibing-rs")
(cd "$DEEPSEEK_VIBING_RS_DIR" && cargo run --quiet --bin deepseek-vibing -- "$@")
