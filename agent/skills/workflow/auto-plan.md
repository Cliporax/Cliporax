# Auto Plan Skill

Use this when a request is likely to touch 3+ files, crosses frontend/backend boundaries, changes data flow, or the user asks for a plan.

## Goal

Create just enough plan to keep implementation safe and fast. Do not produce long template reports for small changes.

## Workflow

1. Inspect first:
   - `git status --short`
   - relevant files with `rg` / `sed`
   - existing tests and commands

2. Classify scope:
   - **Small**: 1-2 files, direct fix. Skip formal plan, implement and verify.
   - **Medium**: 3-6 files or one subsystem. Share a compact checklist.
   - **Large**: cross-subsystem, schema, IPC, sync, plugin, or security impact. Share a phased plan.

3. For medium/large work, use this compact plan shape:

```markdown
Plan:
- Backend: ...
- Frontend: ...
- Tests: ...
- Risk checks: ...
```

4. Execute in thin slices:
   - keep one behavior change per slice when possible
   - run the narrowest useful verification after each risky slice
   - update the checklist only when it reduces user uncertainty

5. Before final response:
   - run targeted tests via `scripts/agent/targeted-test.sh` when available
   - run `scripts/agent/cross-platform-check.sh` before commits or platform-sensitive changes
   - summarize changed behavior and verification results

## Constraints

- Prefer existing repository patterns over new abstractions.
- Do not stop at a plan unless the user explicitly asks for planning only.
- Avoid verbose reports unless the task is architectural, security-sensitive, or user-facing.
