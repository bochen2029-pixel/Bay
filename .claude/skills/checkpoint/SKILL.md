---
name: checkpoint
description: Optional pre-compaction. Render COMPACTION_BRIEF.md, force a save point, then invoke /compact with focused preserve-instructions. Safety net beyond the PreCompact hook.
---

# /checkpoint

Optional, agent-initiated. Use at a natural module boundary when you
sense context getting heavy (turn-count proxy ≥60%) and want a clean
proactive compaction rather than waiting for auto.

## Steps

1. Complete the current save point (atomic — no half-edits).
2. Write COMPACTION_BRIEF.md (the `pre-compact-brief.sh` hook does
   this automatically, but verify it's fresh).
3. Invoke `/compact` with focused instruction:
   ```
   Compact aggressively. Preserve:
   - AUTONOMY_CHARTER content (the constitution)
   - RUN_STATE (current task, next action, subtleties)
   - TASKLIST (status, dependencies, fan-out)
   - last 5 PROGRESS entries
   - active subagent/worktree state
   - current task subtleties from RUN_STATE
   ```
4. Continue work after bootstrap re-orientation.

## Note

The PreCompact hook is the safety net; this skill is the proactive
path. Compaction-indifference means: if RUN_STATE is truly current,
you survive a compaction at any instant with zero special action.
`/checkpoint` is convenience, not dependence.
