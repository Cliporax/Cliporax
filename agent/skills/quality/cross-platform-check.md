# Cross-Platform Check Skill

Use this before commits and whenever code touches Tauri windowing, clipboard, files, global shortcuts, platform-specific Rust, CSS/WebView behavior, or browser APIs.

## Required Command

```bash
scripts/agent/cross-platform-check.sh
```

Treat failures as blockers. Warnings require judgment; mention unresolved warnings in the final response.

## Manual Review Focus

- No `confirm()`, `alert()`, or `prompt()` in frontend code.
- No `navigator.clipboard`, Service Worker, or WebGPU usage in app code.
- Linux clipboard/window ordering: write clipboard first, delay if needed, then hide window.
- Linux `set_focus()` must have error handling or a platform-specific fallback.
- Rust paths use `PathBuf` / `.join()` instead of string separator assembly.
- WebKit-prefixed CSS has standard properties where applicable.
- New system-facing behavior degrades gracefully on macOS, Linux, and Windows.

## When Script Output Is Noisy

- Existing `unwrap()/expect()` calls are warning-level unless introduced by the current diff.
- If Rust path scanning flags URL or provider-internal POSIX remote paths, prefer a clear helper name such as `join_remote_path`; otherwise add `agent-ok: path-separator` only on a reviewed false positive.
- Prefer fixing newly introduced warnings immediately.
- Do not perform broad cleanup unrelated to the user request unless explicitly asked.
