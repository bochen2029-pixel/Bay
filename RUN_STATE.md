# RUN_STATE — Bay v0.2.0 revamp run

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Updated every save
> point. Last updated: 2026-06-17T18:30:00Z (I-19 close, run resumed).

## Run resumed (substrate handoff)

The prior substrate (GLM-5.2) hit its quota mid-I-19. Run resumed by
Claude (Opus). Phases P0–P3 + I-15..I-18 were committed by the prior
run; I-19 was uncommitted/incomplete. I-19 is now complete (see below).
Charter is honored as operator intent (not edited).

## Current task

**Phase 4 complete (I-15..I-19).** I-19 batch operations just landed:
backend cap bug fixed (+ regression test), undo generalized to
batch-undo via (ts,type) grouping, frontend multi-select + BatchActionBar.
cargo 139/139, vitest 90/90, builds clean.

## Next concrete action

**P2e — cold-context two-pass verification.** The prior run dispatched
2 verifier subagents but they never returned (quota). The non-LLM oracle
gate (property + golden) is GREEN, which is the authority per charter §9.
Re-dispatch cold-context `verifier` subagents over the 6 critical
modules + the new batch/undo code; record findings in VERIFIED.md; fix
any BLOCKING drift. Then Phase 5 (I-20 LLM re-org accept-path populating
resulting_event_ids; I-21 recurring tasks; I-22 LLM streaming).

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

I-19 close: `feat(I-19): batch operations` (about to commit). All gates
green: cargo 139/139, vitest 90/90, builds clean, store-logic smoke OK.

## Runway snapshot

Resumed run. Speculations: 0/25. Blockers: 0. One real bug found+fixed
on resume (batch cap enforcement). rework on prior-run code: corrected
the uncommitted batch backend before landing it (not counted as own
rework). Phase 4 done. Next: P2e verification, then P5.

## Pointer back

TASKLIST.md and PROGRESS.md are canonical. AUTONOMY_CHARTER governs.
run-metrics.jsonl is the ledger.
