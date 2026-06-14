#!/usr/bin/env bash
set -euo pipefail

PASS=0
WARN=0
FAIL=0

pass() { PASS=$((PASS + 1)); printf '✅ %s\n' "$1"; }
warn() { WARN=$((WARN + 1)); printf '⚠️  %s\n' "$1"; }
fail() { FAIL=$((FAIL + 1)); printf '❌ %s\n' "$1"; }
section() { printf '\n%s\n' "$1"; }

section '1. Frontend forbidden dialogs'
if rg -n '(^|[^A-Za-z0-9_])(confirm|alert|prompt)\s*\(' src --glob '*.{ts,tsx}' --glob '!**/*.test.*' >/tmp/cliporax_dialogs.txt 2>/dev/null; then
  cat /tmp/cliporax_dialogs.txt
  fail 'Found confirm/alert/prompt usage in frontend code'
else
  pass 'No confirm/alert/prompt usage found'
fi

section '2. Unsupported browser APIs'
rg -n 'navigator\.clipboard|ServiceWorker|serviceWorker|WebGPU|gpu\s*in\s*navigator' src --glob '*.{ts,tsx}' \
  | rg -v 'const ServiceWorker = undefined|const serviceWorker = undefined|PluginSandbox' \
  > /tmp/cliporax_browser_api.txt 2>/dev/null || true
if [ -s /tmp/cliporax_browser_api.txt ]; then
  cat /tmp/cliporax_browser_api.txt
  fail 'Found browser APIs that are unsafe in Tauri WebViews'
else
  pass 'No unsafe browser APIs found'
fi

section '3. WebKit CSS prefixes'
if [ -f src/index.css ]; then
  webkit_issues=0
  while IFS= read -r line; do
    case "$line" in
      *webkit-scrollbar*|*webkit-app-region*|*webkit-user-drag*) continue ;;
    esac
    prop=$(printf '%s' "$line" | sed -n 's/.*-webkit-\([a-z-]*\):.*/\1/p')
    if [ -n "$prop" ] && ! rg -n "^[[:space:]]*${prop}:" src/index.css >/dev/null 2>&1; then
      printf '%s\n' "$line"
      webkit_issues=$((webkit_issues + 1))
    fi
  done < <(rg -n -- '-webkit-' src/index.css || true)
  if [ "$webkit_issues" -gt 0 ]; then
    warn "Found $webkit_issues WebKit-prefixed CSS lines without obvious standard property"
  else
    pass 'WebKit CSS prefix check passed'
  fi
else
  warn 'src/index.css not found'
fi

section '4. Linux window/clipboard race scan'
if rg -n -U 'hide\(\)\?(.|\n){0,240}(write_text|write_image|clipboard)' src-tauri/src --glob '*.rs' >/tmp/cliporax_hide_before_clipboard.txt 2>/dev/null; then
  cat /tmp/cliporax_hide_before_clipboard.txt
  fail 'Potential hide-before-clipboard race found'
elif rg -n -U '(write_text|write_image|clipboard)(.|\n){0,240}hide\(\)\?' src-tauri/src --glob '*.rs' >/tmp/cliporax_clipboard_before_hide.txt 2>/dev/null; then
  pass 'Clipboard-before-hide ordering found; manually confirm delay/error handling where needed'
else
  pass 'No close-range window hide / clipboard ordering issue found'
fi

section '5. set_focus handling scan'
set_focus_lines=$(rg -n 'set_focus\(' src-tauri/src --glob '*.rs' || true)
if [ -z "$set_focus_lines" ]; then
  pass 'No set_focus usage found'
elif printf '%s\n' "$set_focus_lines" | rg 'map_err|if let Err|match|warn!|error!' >/dev/null; then
  pass 'set_focus calls appear to include error handling'
else
  printf '%s\n' "$set_focus_lines"
  warn 'set_focus usage needs manual Linux fallback review'
fi

section '6. Rust path separator scan'
rg -n 'format!\([^\n]*\{\}/|push_str\("/"\)|[A-Za-z_]+\s*\+\s*"/"' src-tauri/src --glob '*.rs' \
  > /tmp/cliporax_paths_raw.txt 2>/dev/null || true
rg -v 'agent-ok: path-separator|https?://|[Uu][Rr][Ll]|[Uu][Rr][Ii]|join_remote_path|remote_path|remote_root' \
  /tmp/cliporax_paths_raw.txt > /tmp/cliporax_paths.txt 2>/dev/null || true
if [ -s /tmp/cliporax_paths.txt ]; then
  cat /tmp/cliporax_paths.txt
  fail 'Found possible hard-coded path separator assembly in Rust'
else
  pass 'No obvious hard-coded Rust path assembly found'
fi

section '7. Rust clippy'
if (cd src-tauri && cargo clippy -- -D warnings) >/tmp/cliporax_clippy.txt 2>&1; then
  pass 'cargo clippy passed'
else
  cat /tmp/cliporax_clippy.txt
  fail 'cargo clippy failed'
fi

section '8. TypeScript typecheck'
if npx tsc --noEmit >/tmp/cliporax_tsc.txt 2>&1; then
  pass 'TypeScript typecheck passed'
else
  cat /tmp/cliporax_tsc.txt
  fail 'TypeScript typecheck failed'
fi

section '9. New unwrap/expect in diff'
if git rev-parse --verify HEAD >/dev/null 2>&1; then
  if git diff -U0 HEAD -- '*.rs' 2>/dev/null | rg '^\+.*\.(unwrap\(\)|expect\()' >/tmp/cliporax_new_unwrap.txt 2>/dev/null; then
    cat /tmp/cliporax_new_unwrap.txt
    warn 'New unwrap()/expect() introduced in Rust diff'
  else
    pass 'No new unwrap()/expect() in Rust diff'
  fi
else
  warn 'Git HEAD unavailable; skipped new unwrap()/expect() diff check'
fi

printf '\nSummary: %s passed, %s warnings, %s failures\n' "$PASS" "$WARN" "$FAIL"
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
