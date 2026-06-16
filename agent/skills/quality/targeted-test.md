# Targeted Test Skill

Use this after code changes to choose the fastest verification that still covers the changed behavior.

## Command

```bash
scripts/agent/targeted-test.sh
```

The script infers checks from `git diff --name-only HEAD`.
Use `--cached` or `--staged` before committing when you want checks inferred only from staged paths.

## Heuristics

- `src-tauri/src/sync/**` or `src-tauri/Cargo.toml` -> `cargo test sync::`
- other Rust backend files -> `cargo test`
- `src/**/*.ts(x)` -> `npx tsc --noEmit` and `npm run test:run`
- plugin source/manifest -> plugin build command when available
- config/package changes -> `npm run build` when practical

## Escalation

Run full checks before commits:

```bash
scripts/agent/cross-platform-check.sh
npm run test:run
cd src-tauri && cargo test
```

If full checks are too expensive, state exactly what was skipped and why.
