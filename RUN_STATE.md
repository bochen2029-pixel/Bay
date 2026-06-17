# RUN_STATE — Bay v0.2.0 revamp run

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Updated every save
> point. Last updated: 2026-06-17T00:30:00Z (Phase 0 close).

## Current task

**Phase 1: Bug-fix pass.** Fix real bugs found in the audit, one
concern per save point, each `fix(I-NN):` with characterization test
→ fix → regression test. Bugs to address: `restore_item` event
semantics, `llm/prompt.rs` hardcoded caps, `bootstrap` return shape,
`set_item_state` done→active at cap, HotkeyInput listener cleanup,
store `onItemUpdated` idempotency, sweep for debug leftovers.

## Next concrete action

Start Phase 1 with the highest-leverage fix: `llm/prompt.rs` hardcoded
`/ 5` and `/ 12` → use `A_CAP`/`B_CAP` constants (smallest, clearest,
unblocks the "caps are constants not magic numbers" cleanliness). Then
`restore_item` event semantics (the `item_created` vs `ITEM_RESTORED`
mismatch). Then `bootstrap` return shape (SPEC §5.1 drift). Sweep last.

## Blocking issues

None on current task.

## Active subagents / worktrees

None yet. Phase 2e (two-pass verification) and Phase 4+ may dispatch
`verifier`/`implementer` subagents.

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
- `restore_item` emits `item_created` Tauri event but appends
  `ITEM_RESTORED` — Phase 1 reconciles.
- Rank fixtures at `scripts/rank-fixtures.json` are operator-owned
  golden cases. Don't edit directly; regen via
  `cargo run --bin rank_fixture_gen` with `SPEC:` tag.
- **Baseline (Phase 0 close):** cargo build warning-clean, cargo test
  91/91, pnpm build clean, pnpm test 85/85, store-logic 55/55. Any
  phase that turns these red must fix or file BLOCKER + SUSPECT.
- **Hooks are in place** (.claude/hooks/). `pre-arch-edit.sh` BLOCKS
  edits to doctrine docs / golden cases / archive / 001_initial.sql
  unless SPEC_AMENDMENT.md exists at repo root. To edit those, write
  SPEC_AMENDMENT.md first.

## Last save point

Phase 0 close: `autonomous-run-2026-06-17-start` tag (about to be
placed). Baseline all-green per VERIFIED.md.

## Runway snapshot

Elapsed: ~0.5h. Attention probe: not yet run (deferred to first
substrate-stress point). `rework_rate` this run: 0.0 (no rework yet).
Speculations: 0/25. Blockers: 0.

## Pointer back

TASKLIST.md and PROGRESS.md are canonical. AUTONOMY_CHARTER governs.
run-metrics.jsonl is the ledger.
