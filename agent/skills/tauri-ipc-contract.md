# Tauri IPC Contract Skill

Use this when adding or changing Tauri commands, frontend invoke wrappers, events, or shared TypeScript/Rust data contracts.

## Contract Checklist

- Rust command has `#[tauri::command]`.
- Command is registered in `invoke_handler`.
- Inputs are validated for empty strings, length, ID ranges, and list sizes.
- Errors include useful context but no secrets or clipboard content.
- Frontend wrapper exists in `src/lib/tauri-api.ts`.
- TypeScript return/input types match Rust serde shape.
- Frontend caller handles rejection with `try/catch` or state-level error handling.
- Tests cover success and the most important failure path.

## Useful Searches

```bash
rg -n "#\[tauri::command\]|invoke_handler|invoke<|listen\(" src src-tauri/src
rg -n "type .*Status|interface .*Input|interface .*Result" src/lib/tauri-api.ts
```

## Avoid

- Calling raw `invoke()` across components when a typed wrapper already exists.
- Returning loosely typed JSON when a stable struct/enum is clearer.
- Adding frontend-only types without updating Rust behavior.
