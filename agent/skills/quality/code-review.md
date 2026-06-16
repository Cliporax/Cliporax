# Code Review Skill

Use this when the user asks for review, before committing risky changes, or when `agent/skills/workflow/post-change-review.md` finds non-trivial risk.

## Review Stance

Find bugs, regressions, missing tests, security risks, deadlocks, and cross-platform failures. Avoid style-only comments unless they affect maintainability.

## Checklist

1. Diff scope:
   - `git diff --stat`
   - `git diff --cached --stat` if reviewing staged changes
   - identify generated files, artifacts, lockfile churn, and unrelated changes

2. Runtime correctness:
   - IPC commands registered and typed in `src/lib/tauri-api.ts`
   - database changes have migrations/tests or explicit compatibility reasoning
   - sync cursor/conflict/tombstone behavior preserves data
   - async tasks do not hold locks across long operations

3. Security/privacy:
   - validate command inputs
   - do not log secrets, clipboard content, credentials, or tokens
   - sensitive data is flagged or encrypted as required
   - plugin permissions are least-privilege

4. Cross-platform:
   - run `scripts/agent/cross-platform-check.sh`
   - check window/focus/clipboard ordering on Linux
   - avoid platform APIs without `#[cfg(...)]` guards or fallback

5. Tests:
   - run `scripts/agent/targeted-test.sh`, or `--cached` for staged-only review
   - add focused tests for changed behavior when practical

For routine "I just changed code" validation, use `agent/skills/workflow/post-change-review.md` first. Escalate to this skill when findings need a code-review style report or the change is broad/risky.

## Output Format

Lead with findings, ordered by severity:

```markdown
Findings:
- High: [file:line] ...
- Medium: [file:line] ...

Verification:
- ...

Residual risk:
- ...
```

If there are no findings, say so clearly and mention any test gaps.
