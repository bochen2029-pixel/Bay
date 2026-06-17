---
name: verify
description: Continuous verification cadence. Every 3 modules completed (or every 4 hours wall-clock, whichever first), dispatch a cold-context verifier subagent against the recent diffs + golden cases. Also the skill to invoke after any critical-module change for two-pass verification. Catches drift early; the non-LLM oracle (property + golden) is the gate.
---

# /verify

Two modes:

## A. Continuous cadence (every 3 modules / 4 wall-clock hours)

Dispatch the `verifier` subagent with cold context. It loads:
- ARCHITECTURE / CLAUDE.md / SPEC.md (canon)
- Recent diffs (last 3 modules)
- TASKLIST current state
- Recent DECISIONS.md entries
- Golden cases for touched modules

Mission: "Is anything drifting? Are contracts still coherent? Any
module claiming DONE that shouldn't be? Any speculation that should
be promoted to decision or unwound?"

Returns: structured finding list (empty is the desired result).
Findings logged to VERIFIED.md. Main agent acts on them (or files
BLOCKER if architectural).

## B. Two-pass for critical modules (after every change to one of the 6)

The 6 critical modules (AUTONOMY_CHARTER §9):
1. `db::write_events`
2. `db::items::apply_event_to_projection`
3. `commands::items::swap_move_inner`
4. `domain::rank::rank_between`
5. `commands::events::get_items_at_inner`
6. `commands::events::rebuild_projection_inner`

After ANY change to one of these:

1. **Pass 1 (main):** implement against contract + golden cases.
2. **Pass 2 (verifier subagent, cold context):** receives contract +
   golden cases + impl diff (NO pass-1 reasoning). Verifies vs
   contracts/invariants; flags drift.
3. **Oracle gate (non-LLM, required):** property test + golden cases
   must pass. The LLM verifier is the second pass; the non-LLM oracle
   is the gate. Both required for critical modules.

## Drift patterns the verifier looks for

- Contract changes not propagated to consumers
- Speculation tags older than 5 modules without resolution → assign a
  target lifecycle state (CONFIRMED | UNWOUND | ABANDONED)
- DECISIONS.md entries marked Active that contradict newer entries
- Module-level NFRs not being met
- Cross-module imports unexpected from contract
- LLM firewall bypassed in any new code path
- `UPDATE`/`DELETE` on `events` (append-only violation)
- `items` writes outside `db::write_events`

## Joint-wrong detector

If implementation passes contract tests but fails a golden case, log a
`JOINT_WRONG:` finding — the most dangerous class. The implementer's
tests and code agreed with each other and both disagreed with the
operator's intent. Everything looked green. The golden case caught it.

This is the Externality Principle earning its keep.
