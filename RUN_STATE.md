# RUN_STATE — Bay v0.2.0 revamp run

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Updated every save
> point. Last updated: 2026-06-17T00:45:00Z (Phase 1 close).

## Current task

**Phase 2a: Property tests.** Add `proptest` Cargo dev-dep and write
property tests for the 6 critical modules: `rank_between` (strictly-
between, monotone), `apply_event_to_projection` (projection
determinism = rebuild reproduces items), `swap_move_inner` (atomicity
+ cap math), `write_events` (rollback on apply failure), cap
enforcement (invariant under random event ordering), `get_items_at`
(monotone; now==list_active_items). These are the non-LLM oracles for
the Externality Principle.

## Next concrete action

Add `proptest = "1"` to `src-tauri/Cargo.toml` `[dev-dependencies]`.
Then write `src-tauri/src/domain/rank.rs` property tests (start with
the simplest: `rank_between(a,b)` strictly between for all valid
inputs). Then `db/items.rs` projection-determinism property test (the
single most important property in the system). Then swap atomicity,
write_events rollback, get_items_at. Run `cargo test` after each.

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

Phase 1 close: `fix(I-15): prompt.rs use A_CAP/B_CAP constants not
magic numbers` (about to commit). Baseline still green.

## Runway snapshot

Elapsed: ~0.75h. Attention probe: not yet run. `rework_rate` this run:
0.0 (no rework). Speculations: 0/25. Blockers: 0. Phase 1 found fewer
bugs than planned — v0.1.1 was thorough. Time freed for Phase 2.

## Pointer back

TASKLIST.md and PROGRESS.md are canonical. AUTONOMY_CHARTER governs.
run-metrics.jsonl is the ledger.
