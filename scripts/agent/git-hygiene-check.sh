#!/usr/bin/env bash
set -euo pipefail

failures=0
warns=0

fail() { failures=$((failures + 1)); printf '❌ %s\n' "$1"; }
warn() { warns=$((warns + 1)); printf '⚠️  %s\n' "$1"; }
pass() { printf '✅ %s\n' "$1"; }

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  fail 'Git metadata unavailable; cannot run pre-commit hygiene checks'
  printf '\nSummary: %s warnings, %s failures\n' "$warns" "$failures"
  exit 1
fi

status=$(git status --short)
printf '%s\n' "$status"

if printf '%s\n' "$status" | rg '^(A |\?\?) .*(\.zip|\.dmg|\.msi|\.exe|\.AppImage|\.deb|\.rpm)$' >/dev/null; then
  fail 'Build/package artifact is staged or untracked'
else
  pass 'No obvious package artifact in status'
fi

if printf '%s\n' "$status" | rg '^A  .*bridge|^A  .*bridge-api|^A  .*\.env\.bridge' >/dev/null; then
  warn 'Bridge-related files are staged; confirm user asked for bridge work'
fi

if printf '%s\n' "$status" | rg '^\?\? ' >/dev/null; then
  warn 'Untracked files exist; stage explicit paths only'
fi

printf '\nSummary: %s warnings, %s failures\n' "$warns" "$failures"
if [ "$failures" -gt 0 ]; then
  exit 1
fi
