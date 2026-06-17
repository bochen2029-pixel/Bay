#!/usr/bin/env bash
# session-start-bootstrap.sh — SessionStart hook (startup, resume, compact).
# Five-step bootstrap + run-lock handling. Fail-safe: errors warn,
# never block session start (the agent can recover from a missing
# bootstrap; it can't recover from being unable to start).
set -uo pipefail

PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$PROJECT_ROOT" || exit 0

echo "[bootstrap] Bay v0.2.0 revamp run — five-step bootstrap"

# Step 0: run-lock handling
LOCK="$PROJECT_ROOT/.run-lock"
if [ -f "$LOCK" ]; then
  # A lock exists. Check staleness (heartbeat older than 2x save interval
  # = 60 min = stale → prior run crashed).
  heartbeat="$(grep -o '"heartbeat": *"[^"]*"' "$LOCK" 2>/dev/null | head -1 | sed 's/.*"heartbeat": *"//;s/"$//')"
  if [ -n "$heartbeat" ]; then
    hb_epoch="$(date -d "$heartbeat" +%s 2>/dev/null || echo 0)"
    now_epoch="$(date +%s 2>/dev/null || echo 0)"
    age_min=$(( (now_epoch - hb_epoch) / 60 ))
    if [ "$age_min" -gt 60 ]; then
      echo "[bootstrap] Stale lock (heartbeat ${age_min}min old) — prior run crashed. Absorbing into crash-recovery, taking lock."
    else
      echo "[bootstrap] WARNING: live lock detected (heartbeat ${age_min}min old). Another run may be active. Proceed with caution — check git status before any write."
    fi
  fi
fi

# Steps 1-4: orient from files + git (advisory; agent does the real read)
echo "[bootstrap] Step 1: read RUN_STATE.md"
[ -f "$PROJECT_ROOT/RUN_STATE.md" ] && head -20 "$PROJECT_ROOT/RUN_STATE.md" || echo "  (missing — fresh run)"
echo "[bootstrap] Step 2: read TASKLIST.md"
[ -f "$PROJECT_ROOT/TASKLIST.md" ] && head -30 "$PROJECT_ROOT/TASKLIST.md" || echo "  (missing — fresh run)"
echo "[bootstrap] Step 3: read QUESTIONS.md + BLOCKERS.md (open items)"
[ -f "$PROJECT_ROOT/QUESTIONS.md" ] && grep -c "^## Q" "$PROJECT_ROOT/QUESTIONS.md" 2>/dev/null | xargs -I{} echo "  Questions: {} open" || echo "  Questions: 0"
[ -f "$PROJECT_ROOT/BLOCKERS.md" ] && grep -c "^## BLOCKER" "$PROJECT_ROOT/BLOCKERS.md" 2>/dev/null | xargs -I{} echo "  Blockers: {} open" || echo "  Blockers: 0"
echo "[bootstrap] Step 4: git status + recent log"
if command -v git >/dev/null 2>&1; then
  git status --short 2>/dev/null | head -10
  git log --oneline -5 2>/dev/null
fi

# Step 5: reality check — run scoped tests for current in-progress module
# (advisory; the agent runs the actual tests. Here we just remind.)
echo "[bootstrap] Step 5: REALITY CHECK — run scoped tests for the in-progress module before proceeding. RUN_STATE may be ahead of filesystem."

echo "[bootstrap] Bootstrap complete. Re-anchor to AUTONOMY_CHARTER.md before any write."
exit 0
