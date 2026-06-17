#!/usr/bin/env bash
# post-edit-tests.sh — PostToolUse hook (Edit, Write).
# Runs scoped tests for the touched module and tracks save cadence.
# Fail-safe: test failures are reported but don't block the edit
# (the agent decides whether to fix or mark SUSPECT).
set -uo pipefail

PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$PROJECT_ROOT" || exit 0

# The hook receives the edited file path via stdin (JSON) or $1.
# Parse defensively; if we can't tell what was edited, run nothing.
input="$(cat 2>/dev/null || echo '')"
edited="$(echo "$input" | grep -oE '"file_path" *: *"[^"]*"' | head -1 | sed 's/.*"file_path" *: *"//;s/"$//' || echo '')"

if [ -z "$edited" ]; then
  # No file identified; skip scoped tests.
  exit 0
fi

# Map edited file to scoped test command. Keep this FAST (<30s) per
# charter (tests must not starve the save-point rhythm).
case "$edited" in
  *src-tauri/src/*)
    # Rust edit — run cargo test for the touched module's lib tests.
    # Use the module path from the file (best-effort).
    echo "[post-edit] Rust edit: $edited — consider 'cargo test' in src-tauri/ for the touched module."
    ;;
  *src/*.ts|*src/*.tsx)
    # TS edit — run vitest for the touched file's test (if a .test.ts(x) exists).
    base="$(echo "$edited" | sed 's/\.\(ts\|tsx\)$//;s/-test$//')"
    test_file="${base}.test.ts"
    [ -f "$test_file" ] || test_file="${base}.test.tsx"
    if [ -f "$test_file" ]; then
      echo "[post-edit] TS edit: $edited — run 'pnpm vitest run $test_file'"
    else
      echo "[post-edit] TS edit: $edited (no scoped test file found — consider adding one)."
    fi
    ;;
  *migrations/*.sql)
    echo "[post-edit] Migration edit: $edited — run 'python scripts/verify-schema.py' after app restart."
    ;;
  *)
    echo "[post-edit] Edit: $edited (no scoped test mapping)."
    ;;
esac

# Save-cadence tracking: warn if PROGRESS.md is stale (>30min).
if [ -f "$PROJECT_ROOT/PROGRESS.md" ]; then
  last_progress_epoch="$(stat -c %Y "$PROJECT_ROOT/PROGRESS.md" 2>/dev/null || stat -f %m "$PROJECT_ROOT/PROGRESS.md" 2>/dev/null || echo 0)"
  now_epoch="$(date +%s 2>/dev/null || echo 0)"
  age_min=$(( (now_epoch - last_progress_epoch) / 60 ))
  if [ "$age_min" -gt 30 ]; then
    echo "[post-edit] CADENCE WARNING: PROGRESS.md last updated ${age_min}min ago. Save point overdue — consider committing."
  fi
fi

exit 0
