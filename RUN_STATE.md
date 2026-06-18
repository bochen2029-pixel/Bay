# RUN_STATE — Bay v0.2.0 revamp run

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Updated every save
> point. Last updated: 2026-06-17T20:30:00Z (session-end consolidation).

## Run resumed (substrate handoff)

The prior substrate (GLM-5.2) hit its quota mid-I-19. Run resumed by
Claude (Opus). Phases P0–P3 + I-15..I-18 were committed by the prior
run; I-19 was uncommitted/incomplete. I-19 is now complete (see below).
Charter is honored as operator intent (not edited).

## Current task

**Session-end consolidation (clean checkpoint).** Phase 4 (I-15..I-19),
P2e (2 BLOCKING bugs fixed), and Phase 5 I-20 (LLM re-org accept-path)
are all DONE and committed. Doctrine reconciled to v1.8/v1.7/v1.5;
README, FUTURE_WORK.md, REVIEW_QUEUE.md, and memory updated. Gates:
cargo 152/152, vitest 93/93, warning-clean.

## Next concrete action (next session)

**Do txn_id first, then I-21.** Per QUESTIONS Q01 + FUTURE_WORK.md:
recurring-task completion is a mixed-type atomic action whose correct
undo requires a transaction-id column on `events` (migration 003) — a
schema change to the append-only core that was deferred to operator
review rather than self-authorized at the tail of this run. With txn_id
in place, the (ts,type) undo limitation (Q01) is also resolved. Then
build I-21 (recurrence design fully specced in FUTURE_WORK.md), then
I-22 streaming, then Phase 6 (operator sign-off recommended — several
items re-litigate the "Cut from v1" list). Highest-value standalone
follow-up: a generic golden-case runner (check-golden.py only checks
existence, not execution — why the P2e JOINT_WRONG slipped past green
tests).

## Blocking issues

None on current task.

## Active subagents / worktrees

None yet. Phase 2e (two-pass verification) will dispatch the
`verifier` subagent against the 6 critical modules.

## Subtleties to preserve across compaction

- The four load-bearing invariants (events append-only; projection
  pure; swap atomic; caps active-only) are currently convention +
  unit tests. Phase 2 makes them mechanical (property tests + DB
  triggers + type-level firewall + golden cases).
- `db::write_events` is the ONLY write path. `apply_event_to_projection`
  is exhaustive match on `EventType`. Don't break either.
- `swap_move_inner` emits two `ITEM_MOVED` events in one tx with one
  ts. Tests at `commands/items.rs:937+` pin this.
- The LLM firewall is absolute. `apply_event_to_projection`'s LLM-event
  arms are `Ok(())`; Phase 2d promotes this to a type-level
  `ProjectionEvent` boundary.
- SPEC §6 module layout is STALE (lists 9 files that don't exist;
  tree is flatter). Phase 3 reconciles. Don't trust §6 until then.
- `package.json` does NOT list `@tauri-apps/plugin-global-shortcut`
  (SPEC §6.2 is stale). Hotkey is Rust-side. Don't add the JS dep.
- `restore_item` reusing `item_created` Tauri event is DOCUMENTED
  (items.rs:462-466), not a bug. Don't "fix" it.
- `set_item_state` done-to-active at cap correctly CAP_EXCEEDEDs
  (items.rs:329-343). Not a bug.
- `bootstrap` returns `{items, settings}` not `{items, settings, lanCapture}`
  — SPEC drift, Phase 3 reconciles (likely amend SPEC, not code).
- Rank fixtures at `scripts/rank-fixtures.json` are operator-owned
  golden cases. Don't edit directly; regen via
  `cargo run --bin rank_fixture_gen` with `SPEC:` tag.
- **Baseline (Phase 0 close):** cargo build warning-clean, cargo test
  91/91, pnpm build clean, pnpm test 85/85, store-logic 55/55. After
  Phase 1: cargo test 91/91 still (prompt.rs fix doesn't add tests).
- **Hooks are in place** (.claude/hooks/). `pre-arch-edit.sh` BLOCKS
  edits to doctrine docs / golden cases / archive / 001_initial.sql
  unless SPEC_AMENDMENT.md exists at repo root. To edit those, write
  SPEC_AMENDMENT.md first.
- **Phase 2b will add migration 002_invariants.sql** including an
  append-only trigger on `events`. After that, any code path trying
  `UPDATE/DELETE FROM events` will ABORT at runtime. The only write
  path is `db::write_events` -> `events::append_event` (INSERT only).

## Last save point

I-20 close: `feat(I-20): LLM re-org proposals` (cbad5b2). Then P2e fixes
(fa77ebb) and the consolidation docs commit. All gates green: cargo
152/152, vitest 93/93, builds clean, store-logic smoke OK.

## Runway snapshot

Resumed run, session ending. Speculations: 1 (Q01, default applied).
Blockers: 0. Bugs found+fixed: 3 (batch cap enforcement on resume; +2
BLOCKING from P2e cold-context verification — undo-unblock CHECK,
restore cap gap). Shipped: Phase 4 (I-15..I-19) completion of I-19, P2e,
I-20. Deferred (clean foundational-blocker stop): I-21 (needs txn_id),
I-22, Phase 6. See REVIEW_QUEUE.md for the operator accept/reject queue.

## Pointer back

TASKLIST.md and PROGRESS.md are canonical. AUTONOMY_CHARTER governs.
run-metrics.jsonl is the ledger.
