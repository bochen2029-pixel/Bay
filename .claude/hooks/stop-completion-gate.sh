#!/usr/bin/env bash
# stop-completion-gate.sh — Stop hook.
# Substantive completion checks. Blocks termination if work remains
# (unfinished TASKLIST items, uncommitted changes, stale PROGRESS).
# Honors $RUN_TERMINATE or STOP_ACK.md for emergency escape.
set -uo pipefail

PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$PROJECT_ROOT" || exit 0

# Emergency escape
if [ -n "${RUN_TERMINATE:-}" ] || [ -f "$PROJECT_ROOT/STOP_ACK.md" ]; then
  echo "[stop-gate] Emergency escape honored. Allowing termination."
  exit 0
fi

# Check TASKLIST for unfinished items
if [ -f "$PROJECT_ROOT/TASKLIST.md" ]; then
  incomplete="$(grep -cE 'status: (NOT_STARTED|IN_PROGRESS|SUSPECT|BLOCKED)' "$PROJECT_ROOT/TASKLIST.md" 2>/dev/null || echo 0)"
  if [ "$incomplete" -gt 0 ]; then
    # Allow stop only if there's a current-phase boundary marker
    echo "{\"decision\":\"block\",\"reason\":\"$incomplete TASKLIST items still NOT_STARTED/IN_PROGRESS/SUSPECT/BLOCKED. Either complete them, file BLOCKERS with what-would-unblock, or write STOP_ACK.md to force-terminate.\"}"
    exit 0
  fi
fi

# Check uncommitted changes
if command -v git >/dev/null 2>&1; then
  if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    echo "{\"decision\":\"block\",\"reason\":\"Uncommitted changes present. Commit (atomic save point) before stopping, or write STOP_ACK.md to force-terminate.\"}"
    exit 0
  fi
fi

# Check PROGRESS recency (warn, don't block — PROGRESS is advisory)
if [ -f "$PROJECT_ROOT/PROGRESS.md" ]; then
  last_progress_epoch="$(stat -c %Y "$PROJECT_ROOT/PROGRESS.md" 2>/dev/null || stat -f %m "$PROJECT_ROOT/PROGRESS.md" 2>/dev/null || echo 0)"
  now_epoch="$(date +%s 2>/dev/null || echo 0)"
  age_min=$(( (now_epoch - last_progress_epoch) / 60 ))
  if [ "$age_min" -gt 30 ]; then
    echo "[stop-gate] WARNING: PROGRESS.md last updated ${age_min}min ago. Consider a final entry before stopping."
  fi
fi

echo "[stop-gate] All gates passed."
exit 0
