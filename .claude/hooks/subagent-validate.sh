#!/usr/bin/env bash
# subagent-validate.sh — SubagentStop hook.
# Runs structural return validation on any subagent return before the
# main session integrates it. Catches hallucinated success (claimed
# complete but diff is broken/empty/out-of-scope).
set -uo pipefail

PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-$(pwd)}"
cd "$PROJECT_ROOT" || exit 0

# The hook fires after a subagent completes. We can't inspect the
# subagent's diff directly here, but we can remind the main agent to
# run return validation, and log the dispatch to run-metrics.
echo "[subagent-validate] Subagent stopped. Before integrating its return, run return validation:"
echo "  1. Diff non-empty (LOC > minimum for claimed work)?"
echo "  2. No deleted tests (git diff --stat tests/)?"
echo "  3. Characterization/property tests added (count ≥ claimed)?"
echo "  4. Contract tests + golden cases pass (RE-RUN IN MAIN, not subagent's claim)?"
echo "  5. No out-of-scope files (all edits within declared module)?"
echo "  6. No GOLDEN block edits (operator-owned)?"
echo "  7. No new top-level deps (package manifest unchanged unless authorized)?"
echo "  8. No suspiciously-small diff for claimed work?"
echo "Failed validation → do NOT integrate; log to BLOCKERS/QUESTIONS; re-dispatch sharper or take into main."

# Log dispatch count to run-metrics (append-only; the main agent
# updates the full record at /close-run).
METRICS="$PROJECT_ROOT/run-metrics.jsonl"
if [ -f "$METRICS" ]; then
  # We can't easily increment a JSONL field from bash; just note it.
  :
fi

exit 0
