#!/usr/bin/env bash
# pre-arch-edit.sh — PreToolUse hook (Edit/Write on doctrine docs + golden cases).
# Requires the SPEC: protocol (operator action) before allowing edits
# to operator-owned ground truth: AUTONOMY_CHARTER, archive/*,
# migrations/001_initial.sql, contracts/golden/*.json,
# scripts/rank-fixtures.json, CLAUDE.md, SPEC.md, PROMPTS.md.
#
# The "SPEC:" tag must appear in the commit message OR a
# SPEC_AMENDMENT.md file must exist at repo root explaining the change.
# Fail-safe: blocks the edit if neither is present.
set -uo pipefail

PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$PROJECT_ROOT" || exit 0

input="$(cat 2>/dev/null || echo '')"
edited="$(echo "$input" | grep -oE '"file_path" *: *"[^"]*"' | head -1 | sed 's/.*"file_path" *: *"//;s/"$//' || echo '')"

if [ -z "$edited" ]; then
  exit 0
fi

# Operator-owned paths. Match by suffix/substring.
is_protected=false
case "$edited" in
  */AUTONOMY_CHARTER.md|*/archive/*|*/migrations/001_initial.sql|*/contracts/golden/*.json|*/scripts/rank-fixtures.json|*/CLAUDE.md|*/SPEC.md|*/PROMPTS.md)
    is_protected=true
    ;;
esac

if [ "$is_protected" = "false" ]; then
  exit 0
fi

# Allow if a SPEC_AMENDMENT.md exists at repo root (operator pre-approved).
if [ -f "$PROJECT_ROOT/SPEC_AMENDMENT.md" ]; then
  echo "[pre-arch-edit] SPEC_AMENDMENT.md present — allowing edit to protected path: $edited"
  exit 0
fi

# Otherwise: block. The agent must either (a) write a SPEC_AMENDMENT.md
# explaining the change and re-attempt, or (b) get explicit operator
# approval (file BLOCKER, mark DEFERRED_HUMAN).
echo "{\"decision\":\"block\",\"reason\":\"Protected path requires SPEC: protocol. Either (a) write SPEC_AMENDMENT.md at repo root explaining the change, or (b) file BLOCKER and mark DEFERRED_HUMAN. Path: $edited\"}"
exit 0
