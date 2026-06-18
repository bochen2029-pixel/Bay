# REVIEW_QUEUE — resumed run, session ending 2026-06-17

> Active-review accept/reject queue for the autonomous work done after the
> prior substrate (GLM-5.2) exhausted its quota mid-I-19. Ordered
> cheapest-to-verify-first. Each item: what changed, how to verify, and
> the exact revert. This session did NOT fully close the run (.run-lock
> stays; no `v0.2.0` tag — I-21/I-22/Phase 6 remain). It's a session
> checkpoint, not a release.
>
> All gates green at session end: `cargo test` 152/152, `cargo build`
> warning-clean, `pnpm build` clean, `pnpm test` 93/93,
> `node scripts/test-store-logic.mjs` green.

## Commits this session (newest first)

| sha | what | revert |
|---|---|---|
| (docs) | consolidation: doctrine v1.8/v1.7/v1.5, README, FUTURE_WORK, memory | `git revert <sha>` |
| cbad5b2 | feat(I-20): LLM re-org proposals — human-accepted atomic diff | `git revert cbad5b2` |
| fa77ebb | fix(P2e): two BLOCKING bugs from cold-context verification | `git revert fa77ebb` |
| ac5d6e7 | feat(I-19): batch operations | `git revert ac5d6e7` |

(Prior commits `c2d4278`..`6307b7c` = I-15..I-18, landed by the prior
substrate before the handoff; `0522cec`..`4078c8b` = P0–P3.)

## Review items — cheapest to verify first

### 1. Docs / doctrine (verify: read) — cheapest
- CLAUDE.md v1.8 "Current state" rewritten to reflect I-15..I-20 + P2e.
- SPEC.md → v1.7, PROMPTS.md → v1.5 (drafted by a subagent: resulting_
  event_ids now populated; LLM scope "observe → observe+propose"; batch
  ops in §3; I-15..I-20 increment prompts). Old versions in `archive/`.
- README v0.2.0 features; `FUTURE_WORK.md` (remaining scope + designs);
  memory `project_bay_state.md` updated.
- **Decision to review:** the system prompt evolved from "observe only"
  to "observe + optionally propose a re-org." I read this as fulfilling
  the doctrine's preserved v2 surface (CLAUDE.md §2), NOT a firewall
  change. If you disagree, revert the prompt hunk in cbad5b2.
- Revert: `git revert <docs-sha>` (docs only; no code impact).

### 2. dedup_preserving_order at batch boundaries (verify: read; trivial)
- Defensive de-dup of batch ids (the frontend already sends a Set).
- In ac5d6e7, `commands/items.rs`. Cosmetic; harmless to keep or drop.

### 3. affected_ids accuracy (verify: read the diff)
- batch_set_state/batch_delete now derive affected_ids from the returned
  events (NO_OP items excluded) rather than echoing the input. ac5d6e7.

### 4. Frontend batch UI (verify: `pnpm test` + run the app)
- store multi-select (selectedIds/lastSelectedId + toggle/range/clear),
  Strip checkbox + shift-click, BatchActionBar. 5 vitest specs. ac5d6e7.

### 5. Batch cap-enforcement fix (verify: the regression test)
- The inherited uncommitted batch backend cap-checked against the
  pre-batch count for every item (write_events builds all drafts before
  applying any) → a batch could overflow A/B. Fixed with incremental
  projected counters. Test: `cargo test batch_set_state_to_active_rolls_back_on_cap_overflow`.
  In ac5d6e7. **This was a real bug in the inherited code.**

### 6. P2e BLOCKING-2 — restore cap gate (verify: test + caps.json #12)
- `restore_item_inner` now refuses restoring an ACTIVE item into a full
  A/B (was unchecked — a JOINT_WRONG vs `contracts/golden/caps.json`
  #12). Tests: `restore_active_item_into_full_a_is_cap_exceeded`,
  `restore_blocked_item_into_full_a_succeeds`. In fa77ebb. **Pre-existing
  bug since v0.1.x.** If you'd rather allow archive-restore to exceed cap,
  this is the hunk to revert (and amend caps.json #12).

### 7. P2e BLOCKING-1 — unblock-reason preservation (verify: tests)
- set_item_state/batch_set_state now record the OUTGOING blocked_reason
  in the ITEM_STATE_CHANGED payload when leaving blocked, so undo can
  restore a blocked row without tripping the migration-002 CHECK. Tests:
  `leaving_blocked_records_outgoing_reason_in_event_payload`,
  `undo_after_unblock_does_not_violate_check_constraint`. In fa77ebb.
  **Pre-existing bug since I-17.** Payload shape unchanged (SPEC §4.3
  already allows blocked_reason: string|null).

### 8. Undo (ts,type) action grouping (verify: QUESTIONS Q01 + undo tests)
- undo generalized from "single event or swap-pair" to "most-recent
  events sharing (ts,type)" → delivers batch-undo, subsumes swap-undo.
  ac5d6e7. **Documented limitation in QUESTIONS Q01** (over-groups two
  distinct same-type commands in the same ms — production-unreachable in
  a single-user GUI). The precise fix is a txn_id column (deferred —
  see item 10). Tests: undo_batch_delete/undo_batch_set_state/undo_swap.

### 9. I-20 accept-reorg (verify: the 5 accept-reorg tests)
- accept_suggestion(ops) applies a human-accepted re-org as ONE atomic,
  cap-enforced write_events tx; populates resulting_event_ids (predicted
  via MAX(id)+k, safe because events is append-only). Net-final cap check
  allows a demote+promote swap. cbad5b2. Tests:
  `cargo test accept_reorg`. **Most complex change — review closely.**

### 10. Deferral decision (verify: read FUTURE_WORK.md + QUESTIONS Q01) — a NON-action to ratify
- I did NOT build I-21 (recurring) or the txn_id schema change, because
  recurring-task completion is a mixed-type atomic action whose correct
  undo needs txn_id — a schema change to the append-only `events` table
  that the charter flags for operator review. I judged "stop clean at the
  I-20 boundary" higher-quality than "force an events schema change
  autonomously at the tail of a long run." **If you want recurring
  shipped, the next session should do txn_id first (Q01) — it also
  resolves the Q01 limitation in item 8.**

## Complacency canary
The cold-context P2e verifiers found 2 BLOCKING bugs that 143 passing
tests had missed (items 6 + 7). Lesson re-confirmed: green tests are not
proof of correctness; the operator golden cases are an unenforced
externality (`scripts/check-golden.py` checks existence, not execution —
a generic golden runner is the highest-value untaken P2c follow-up).
