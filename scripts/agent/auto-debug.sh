#!/usr/bin/env bash
set -u -o pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

mkdir -p artifacts/auto-debug

COMMANDS=(
  "npx tsc --noEmit"
  "npm run test:run"
  "npm run test:e2e"
)

if [[ "${CLIPORAX_NATIVE_SMOKE:-0}" = "1" ]]; then
  COMMANDS+=("scripts/agent/tauri-smoke.sh")
fi

last_command=""
status=0

for command in "${COMMANDS[@]}"; do
  last_command="$command"
  echo "==> $command"
  bash -lc "$command"
  status=$?
  if [[ $status -ne 0 ]]; then
    break
  fi
done

if [[ $status -ne 0 ]]; then
  scripts/agent/collect-debug-artifacts.sh "$last_command" "$status"
fi

exit "$status"
