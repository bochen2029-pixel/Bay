---
name: save
description: Atomic save point. One indivisible unit — never partial. Run every completed task, every 25–30 min (15 if tired), before any risky op, at any natural module boundary, and before predicted compaction. Each save point leaves the system compaction-indifferent.
---

# /save

Each save point is one indivisible unit. **Never partial.**

## Steps

1. **All edited files saved to disk.** (Write tool does this; verify
   no buffer unsaved.)
2. **Scoped tests + golden cases run.** Passing, OR failure logged to
   VERIFIED.md / BLOCKERS.md before commit. Do not commit a known-broken
   state without a BLOCKER entry explaining why.
3. **`git commit -m "<module>: <action> [TASKLIST: ITEM-ID]"`** with
   prefix per PROMPTS.md §4:
   - `feat(I-NN):` — new increment work
   - `fix(I-NN):` — revision to a prior increment
   - `chore:` — dependencies, build config, non-behavioral
   - `docs:` — CLAUDE/SPEC/PROMPTS/README edits
   - `refactor:` — internal restructuring, no behavior change
   - `DECISION:` — autonomous decision logged in DECISIONS.md
   - `SPEC:` — doctrine/contract change (requires SPEC_AMENDMENT.md)
   - `STUCK:` — partial work preserved before BLOCKERS filing
   - `CHARTER_EXPANSION:` — self-authorized reversible decision
   - `WIP:` — non-save-point interim commit (allowed; squash later)
4. **RUN_STATE.md updated.** Current task, next action, subtleties.
   Kept current enough to BE the compaction brief.
5. **TASKLIST.md updated.** Status transitions, cost_actual.
6. **Append a `run-metrics.jsonl` increment.** Modules completed,
   LOC shipped, decisions this batch, charter health ratio.
7. **Update `.run-lock` heartbeat** to current timestamp.

## When to save

- Every completed TASKLIST item (→ DONE)
- Every 25–30 min wall-clock (15 if tired-mode)
- Before any risky operation (migration, large refactor, contract
  change with downstream impact)
- Before predicted compaction (~60% capacity by turn-count proxy)
- At any natural module boundary

## Worst case

Lost work bounded by inter-save interval (~30 min, ~15 min tired).
Git log is the durable source of truth; filesystem state at any point
is reconstructable from `git checkout <commit>`.

## Compaction-indifference

If RUN_STATE / TASKLIST / PROGRESS are truly current, the agent
survives a compaction at any instant with zero special action —
COMPACTION_BRIEF.md is just the latest RUN_STATE rendered for a cold
reader. Always brief-ready; compact whenever.
