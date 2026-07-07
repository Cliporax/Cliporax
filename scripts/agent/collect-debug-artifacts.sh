#!/usr/bin/env bash
set -u -o pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

FAILED_COMMAND="${1:-unknown}"
FAILED_STATUS="${2:-unknown}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="artifacts/auto-debug/$STAMP"

mkdir -p "$OUT_DIR"

git status --short > "$OUT_DIR/git-status.txt" 2>&1
git diff --stat > "$OUT_DIR/git-diff-stat.txt" 2>&1

if [[ -d test-results ]]; then
  cp -R test-results "$OUT_DIR/test-results"
fi

if [[ -d playwright-report ]]; then
  cp -R playwright-report "$OUT_DIR/playwright-report"
fi

latest_screenshot="$(find test-results -name '*.png' -print 2>/dev/null | sort | tail -1)"
screenshot_summary="Not captured."
if [[ -n "$latest_screenshot" ]]; then
  cp "$latest_screenshot" "$OUT_DIR/screenshot.png"
  screenshot_summary="$OUT_DIR/screenshot.png"
fi

latest_trace="$(find test-results -name 'trace.zip' -print 2>/dev/null | sort | tail -1)"
trace_summary="Not captured."
if [[ -n "$latest_trace" ]]; then
  cp "$latest_trace" "$OUT_DIR/trace.zip"
  trace_summary="$OUT_DIR/trace.zip"
fi

latest_dom="$(find test-results -name 'dom.html' -print 2>/dev/null | sort | tail -1)"
dom_summary="Not captured."
if [[ -n "$latest_dom" ]]; then
  cp "$latest_dom" "$OUT_DIR/dom.html"
  dom_summary="$OUT_DIR/dom.html"
else
  latest_error="$(find test-results -name 'error-context.md' -print 2>/dev/null | sort | tail -1)"
  if [[ -n "$latest_error" ]]; then
    cp "$latest_error" "$OUT_DIR/dom.html"
    dom_summary="$OUT_DIR/dom.html (Playwright error context)"
  fi
fi

DEV_LOG="${XDG_DATA_HOME:-$HOME/.local/share}/com.cliporax.app/dev.log"
if [[ -f "$DEV_LOG" ]]; then
  tail -200 "$DEV_LOG" > "$OUT_DIR/dev-log-tail.log"
else
  printf 'No Tauri dev log found at %s\n' "$DEV_LOG" > "$OUT_DIR/dev-log-tail.log"
fi

latest_console="$(find test-results -type f -name 'console.log' -print 2>/dev/null | sort | tail -1)"
if [[ -n "$latest_console" ]]; then
  cp "$latest_console" "$OUT_DIR/console.log"
else
  latest_stdout="$(find test-results -type f \( -name 'stdout.txt' -o -name 'stderr.txt' \) -print 2>/dev/null | sort | tail -1)"
  if [[ -n "$latest_stdout" ]]; then
    cp "$latest_stdout" "$OUT_DIR/console.log"
  else
    printf 'No Playwright console/stdout/stderr artifact found.\n' > "$OUT_DIR/console.log"
  fi
fi

cat > "$OUT_DIR/summary.md" <<SUMMARY
# Auto Debug Summary

## Command

$FAILED_COMMAND

## Failure

Exit status: $FAILED_STATUS

## Recent Console Errors

$OUT_DIR/console.log

## DOM Snapshot

$dom_summary

## Screenshot

$screenshot_summary

## Playwright Trace

$trace_summary

## Tauri Dev Log Tail

$OUT_DIR/dev-log-tail.log

## Changed Files

\`\`\`
$(cat "$OUT_DIR/git-status.txt")
\`\`\`

## Diff Stat

\`\`\`
$(cat "$OUT_DIR/git-diff-stat.txt")
\`\`\`
SUMMARY

printf 'Collected auto-debug artifacts in %s\n' "$OUT_DIR"
