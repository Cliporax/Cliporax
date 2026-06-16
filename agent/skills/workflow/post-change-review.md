# Post-Change Review Skill

Use this after implementing any requested code change, before the final response. Keep it lightweight for small changes and deeper for risky changes.

## Goal

Confirm the implementation satisfies the user request, preserves unrelated behavior, and has enough verification for the risk level.

## Workflow

1. Restate the intended change in one sentence.
   - Include the exact behavior requested by the user.
   - Name any assumptions made while implementing.

2. Inspect the diff boundary.
   - Run `git diff --stat` and `git diff --name-only HEAD`.
   - Confirm each changed file is necessary for the request.
   - If unrelated pre-existing work is present, distinguish it from the current change and do not modify it.
   - Check for accidental artifacts, generated files, lockfile churn, debug logs, or broad formatting-only edits.

3. Review logic correctness.
   - Read the changed hunks, not only the final files.
   - Trace the main success path and at least one failure/empty-state path.
   - Verify call sites, state updates, async ordering, error handling, and cleanup still match existing patterns.
   - For Rust async code, confirm locks are not held across long operations or `.await` chains unless the scope is intentionally tiny.
   - For frontend code, confirm state updates cannot stale-read, duplicate requests, or leave loading/error state stuck.

4. Check preservation of existing behavior.
   - Identify adjacent behavior that should remain unchanged.
   - Compare the new branches/defaults with the old behavior in the diff.
   - Look for silent changes to sorting, filtering, persistence, command names, event payloads, settings defaults, permissions, or platform behavior.
   - If a behavior change outside the requested scope is necessary, call it out explicitly.

5. Apply domain-specific guardrails when relevant.
   - IPC changes: use `agent/skills/domain/tauri-ipc-contract.md`.
   - Sync changes: use `agent/skills/domain/sync-engine.md`.
   - Clipboard/window/files/shortcuts/CSS/browser API changes: use `agent/skills/quality/cross-platform-check.md`.
   - Security/privacy-sensitive paths: verify no secrets, tokens, decrypted payloads, or full clipboard content are logged.

6. Verify with the narrowest useful checks.
   - Run `scripts/agent/targeted-test.sh` after code changes when practical.
   - Run `scripts/agent/cross-platform-check.sh` for platform-sensitive changes or before commits.
   - If a check cannot be run, explain the exact reason and the residual risk.

7. Fix issues found during review before responding.
   - Prefer small follow-up edits over broad cleanup.
   - Re-run the relevant targeted check after a review-driven fix when practical.

## Final Response Shape

Keep the final response compact:

```markdown
Implemented ...

Verified:
- ...

Review:
- Scope checked; no unrelated behavior changes found.
```

If the review finds risk, say it plainly:

```markdown
Residual risk:
- ...
```
