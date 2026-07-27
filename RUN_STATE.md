# RUN_STATE — Bay v0.3 execution run (2026-07-26)

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Updated every save
> point. Last updated: 2026-07-26 (run resume).

## Run identity

Resumed from the clean I-20 pause under **operator directive 2026-07-26**
("proceed at your best recommendation — most ambitious, highest
quality"): see DECISIONS ADR-007 for the full dispositions (REVIEW_QUEUE
#1 ratified; envelope-003 authorized; VISION T1/T2/T3/T6/T7/T9 in scope;
T4/T5/T8 still gated). Substrate: Claude Fable 5. VISION.md (written
this session, non-doctrine) is the design source; FUTURE_WORK.md has the
I-21 spec; plan order = VISION §8.

## Current task

**P5a — golden runner.** Execute contracts/golden/*.json in cargo test.
Execution design already surfaced THREE defective proposed caps cases
(#5 blocked-doesn't-count, #6 done-doesn't-count, #8 done_after
miscount) — internally inconsistent with their own names + doctrine;
being corrected as proposal edits (they were never frozen) and flagged
for operator freeze in REVIEW_QUEUE.

## Next concrete actions

1. P5a golden runner (in progress) → commit.
2. P5b migration 003 envelope + undo-by-txn_id (Q01 closes CONFIRMED).
   sha2 dep needs ADR-008 + SPEC: tag. Then two-pass verify (write_events
   is critical module #1).
3. P5 I-21 recurring per FUTURE_WORK spec (unblocked by txn_id).
4. P5c execution core: first_step → Today/Now + day-roll → sessions +
   focus → day open/close → Mirror v1. Backend-first, each increment
   green + committed.
5. P9-equivalent: doctrine co-pass (CLAUDE v1.9 / SPEC v1.8 / PROMPTS
   v1.6) + SPEC_AMENDMENT third pass + REVIEW_QUEUE rebuild + memory.

## Blocking issues

None.

## Subtleties to preserve across compaction

- All prior subtleties from the 2026-06-17 RUN_STATE still hold
  (write_events only write path; ProjectionEvent firewall; restore
  reuses item_created Tauri event BY DESIGN; rank fixtures regen only
  via rank_fixture_gen + SPEC: tag; pre-arch-edit hook needs
  SPEC_AMENDMENT.md for doctrine edits — it EXISTS, extend don't delete).
- Golden files: rank.json FROZEN (untouchable); projection/swap/caps
  are _status:proposed → agent may edit as proposals, flag for freeze.
- caps.json cases 5/6/8 were DEFECTIVE as authored (see REVIEW_QUEUE
  on return); corrected versions are doctrine-derived (blocked/done
  don't count; failed transition leaves state unchanged).
- Undo semantics after 003: group by txn_id (fallback (ts,type) for
  legacy NULL); undo SKIPS actor='system' txns and non-projection
  events inside a txn; compensating events get origin 'undo:<txn_id>'.
- Envelope population: txn_id = one uuidv7 per write_events call;
  actor default 'human', 'system' only for day-roll; origin threaded
  where trivially known (lan, llm_accept:<id>, undo:<txn>, day_roll);
  device_id from settings (generated once, not user-editable);
  schema_ver = 1 constant for now; prev_hash = SHA-256 chain computed
  inside the write tx (read last event's hash under the same tx).
- I-21: child spawn cap-overflow routes to INBOX (doctrine-consistent);
  ITEM_RECURRED is audit-only (to_projection_event -> None; update the
  doc-comment so None reads "no projection effect", firewall = no LLM
  variants still).
- Today is an OVERLAY (items.today_on date column), NOT a tier. Cap 3.
  DAY_ROLLED per-item events are TODAY_REMOVED{cause:expired} with
  actor system; DAY_OPENED/DAY_CLOSED are audit events with NULL
  item_id (first null-item_id events in the log — SPEC §4.3 note).
- Sessions: at most ONE open session; sessions table is a projection
  (rebuild_projection must rebuild it too — extends critical module #6).

## Last save point

Run resume commit (this one). Prior baseline: 709be5d, cargo 152/152 +
vitest 93/93 verified cold 2026-07-26.

## Pointer back

TASKLIST.md (P5a/P5b/P5c added) and PROGRESS.md are canonical.
AUTONOMY_CHARTER governs. VISION.md is design-source, not doctrine.
