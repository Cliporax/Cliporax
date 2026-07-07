#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/agent/targeted-test.sh [--cached|--staged|--all]

Runs checks inferred from changed paths.

  default   changes against HEAD, including staged and unstaged files
  --cached  staged changes only
  --staged  alias for --cached
  --all     same as default; kept for explicit pre-commit use
EOF
}

mode="head"
case "${1:-}" in
  "")
    ;;
  --cached|--staged)
    mode="cached"
    ;;
  --all)
    mode="head"
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1 || ! git rev-parse --verify HEAD >/dev/null 2>&1; then
  echo 'Git metadata or HEAD is unavailable; cannot infer targeted tests from changed paths.'
  echo 'Run explicit checks instead, for example: npm run test:run && cargo test --manifest-path src-tauri/Cargo.toml'
  exit 0
fi

if [ "$mode" = "cached" ]; then
  files=$(git diff --cached --name-only)
else
  files=$(git diff --name-only HEAD)
fi

if [ -z "$files" ]; then
  if [ "$mode" = "cached" ]; then
    echo 'No staged changes; nothing to infer.'
  else
    echo 'No changes against HEAD; nothing to infer.'
  fi
  exit 0
fi

ran=0
run() {
  echo
  echo "▶ $*"
  "$@"
  ran=1
}

needs_e2e=0
needs_native_smoke=0

if printf '%s\n' "$files" | rg '^src-tauri/src/sync/|^src-tauri/Cargo\.(toml|lock)$' >/dev/null; then
  run cargo test --manifest-path src-tauri/Cargo.toml sync::
elif printf '%s\n' "$files" | rg '^src-tauri/src/.*\.rs$' >/dev/null; then
  run cargo test --manifest-path src-tauri/Cargo.toml
fi

if printf '%s\n' "$files" | rg '^src-tauri/src/(commands/|main\.rs)|^src-tauri/tauri\.conf\.json|^src-tauri/capabilities/' >/dev/null; then
  needs_e2e=1
  needs_native_smoke=1
fi

if printf '%s\n' "$files" | rg '^src/.*\.(ts|tsx)$|^package(-lock)?\.json$|^tsconfig|^vite\.config\.ts$|^playwright\.config\.ts$|^tests/e2e/' >/dev/null; then
  run npx tsc --noEmit
  run npm run test:run
  needs_e2e=1
fi

if printf '%s\n' "$files" | rg '^src/lib/tauri-api\.ts$' >/dev/null; then
  needs_native_smoke=1
fi

if [ "$needs_e2e" -eq 1 ]; then
  run npm run test:e2e
fi

if [ "$needs_native_smoke" -eq 1 ] && [ "${CLIPORAX_NATIVE_SMOKE:-0}" = "1" ]; then
  run scripts/agent/tauri-smoke.sh
fi

if printf '%s\n' "$files" | rg '^plugins/com\.cliporax\.cloud-sync/' >/dev/null; then
  if [ -f plugins/com.cliporax.cloud-sync/package.json ]; then
    run npm --prefix plugins/com.cliporax.cloud-sync run build
  fi
fi

if [ "$ran" -eq 0 ]; then
  echo 'No targeted test mapping matched these files.'
fi
