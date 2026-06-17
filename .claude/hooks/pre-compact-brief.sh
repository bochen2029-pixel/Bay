#!/usr/bin/env bash
# pre-compact-brief.sh — PreCompact hook.
# Renders COMPACTION_BRIEF.md before any compaction (proactive or
# auto), and forces a save point. Safety net for compaction-indifference:
# if RUN_STATE is truly current, the brief is just RUN_STATE rendered
# for a cold reader. Fail-safe: errors warn, never block compaction.
set -uo pipefail

PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$PROJECT_ROOT" || exit 0

BRIEF="$PROJECT_ROOT/COMPACTION_BRIEF.md"
NOW="$(date -Iseconds 2>/dev/null || date)"

# Force a save point: commit anything in flight (non-blocking).
if command -v git >/dev/null 2>&1; then
  git add -A >/dev/null 2>&1
  if ! git diff --cached --quiet >/dev/null 2>&1; then
    git commit -m "WIP: pre-compaction save ($NOW)" >/dev/null 2>&1 || true
  fi
fi

# Pull fields from RUN_STATE if present (graceful if missing).
current_task="(see RUN_STATE.md)"
next_action="(see RUN_STATE.md)"
if [ -f "$PROJECT_ROOT/RUN_STATE.md" ]; then
  current_task="$(awk '/^## Current task/{f=1;next} /^## /{f=0} f{print}' "$PROJECT_ROOT/RUN_STATE.md" | head -5 | tr '\n' ' ' | sed 's/  */ /g')"
  next_action="$(awk '/^## Next concrete action/{f=1;next} /^## /{f=0} f{print}' "$PROJECT_ROOT/RUN_STATE.md" | head -8 | tr '\n' ' ' | sed 's/  */ /g')"
fi

last_commit="(none)"
if command -v git >/dev/null 2>&1; then
  last_commit="$(git log -1 --format='%H: %s (%ci)' 2>/dev/null || echo '(none)')"
fi

cat > "$BRIEF" <<EOF
# COMPACTION_BRIEF — $NOW

## Current goal
$current_task

## Last productive action
$last_commit

## Next concrete action
$next_action

## Do NOT redo
- Phase 0 (autonomy spine) — DONE once git tag autonomous-run-2026-06-17-start exists
- Any TASKLIST item marked DONE (check TASKLIST.md)
- Golden cases in contracts/golden/*.json once frozen (operator-owned)
- migrations/001_initial.sql (NEVER_MODIFY per charter §1)

## Active subagents / worktrees
(run \`git worktree list\` for current state)

## Open speculations
(see QUESTIONS.md — count open entries)

## Open blockers
(see BLOCKERS.md — count open entries)

## Pointer back
RUN_STATE.md, TASKLIST.md, PROGRESS.md are canonical.
AUTONOMY_CHARTER.md governs all decisions.
run-metrics.jsonl is the machine ledger.
EOF

echo "Brief written to $BRIEF"
exit 0
