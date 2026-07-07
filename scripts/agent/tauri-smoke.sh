#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

TAURI_DRIVER="${TAURI_DRIVER:-$(command -v tauri-driver || true)}"
WEBKIT_DRIVER="${WEBKIT_DRIVER:-$(command -v WebKitWebDriver || true)}"
APP_BIN="${CLIPORAX_APP_BIN:-$ROOT_DIR/src-tauri/target/debug/cliporax}"
RUNTIME_DIR=""

cleanup() {
  if [[ -n "$RUNTIME_DIR" && -d "$RUNTIME_DIR" ]]; then
    rm -rf "$RUNTIME_DIR"
  fi
}
trap cleanup EXIT

if [[ -z "$TAURI_DRIVER" ]]; then
  echo "Skipping native Tauri smoke: tauri-driver not found."
  echo "Install with: cargo install tauri-driver --locked"
  exit 0
fi

case "$(uname -s)" in
  Linux)
    if [[ -z "$WEBKIT_DRIVER" ]]; then
      echo "Skipping native Tauri smoke: WebKitWebDriver not found."
      echo "Install the WebKit WebDriver package for this Linux distribution, for example webkit2gtk-driver on Debian-based systems."
      exit 0
    fi
    ;;
  Darwin)
    echo "Skipping native Tauri smoke: Tauri desktop WebDriver does not support macOS WKWebView."
    exit 0
    ;;
esac

if [[ ! -x "$APP_BIN" ]]; then
  if [[ "${CLIPORAX_NATIVE_SMOKE_BUILD:-0}" = "1" ]]; then
    npm run tauri -- build --debug --no-bundle
  else
    echo "Skipping native Tauri smoke: app binary not found at $APP_BIN."
    echo "Build it first with: CLIPORAX_NATIVE_SMOKE_BUILD=1 scripts/agent/tauri-smoke.sh"
    exit 0
  fi
fi

RUNTIME_DIR="$(mktemp -d "${TMPDIR:-/tmp}/cliporax-native-smoke.XXXXXX")"
RUNTIME_BIN="$RUNTIME_DIR/cliporax"
cp "$APP_BIN" "$RUNTIME_BIN"
touch "$RUNTIME_DIR/portable"
mkdir -p "$RUNTIME_DIR/data"

node tests/native-smoke/smoke.mjs \
  "$RUNTIME_BIN" \
  "$TAURI_DRIVER" \
  "$RUNTIME_DIR/data/cliporax.db"
