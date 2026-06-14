# Git Hygiene Skill

Use this before staging, committing, or pushing.

## Required Checks

```bash
scripts/agent/git-hygiene-check.sh
scripts/agent/targeted-test.sh --cached
git status --short
git diff --cached --stat
```

## Rules

- Do not commit build artifacts: zip/dmg/msi/exe/appimage, `dist/`, `target/`, generated plugin bundles.
- Do not commit experimental or unrelated untracked code unless the user asks.
- Do not mix package managers for one package. If a plugin has `yarn.lock`, avoid adding `package-lock.json` unless intentionally migrating.
- Review docs/plans for feature-specific content before staging; plans for abandoned experiments should stay untracked.
- If pushing fails due to auth, report the local commit hash and exact push error.

## Commit Flow

1. Inspect `git status --short`.
2. Stage explicit paths only.
3. Re-check `git diff --cached --stat`.
4. Run targeted tests for staged paths and cross-platform checks.
5. Commit with a concise conventional message.
6. Push and report success or auth failure.
