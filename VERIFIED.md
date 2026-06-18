# VERIFIED.md — Bay v0.2.0 revamp run

> What's been tested, when, how, by which oracle. Anything not here
> is "trust but reverify on resume." Append-only.

## 2026-06-17T00:00:00Z — Phase 0 baseline (run start)

All four baseline gates GREEN at Phase 0 close. This is the floor
against which all subsequent changes must hold or explicitly regress
(with a characterization test justifying the regression).

- **`cargo build`** (src-tauri/): warning-clean, finished in 57.40s.
  No `warning:` lines in output.
- **`cargo test`** (src-tauri/): **91 passed, 0 failed, 0 ignored**
  in 0.56s. (README cites 75; the v1.1 cleanup pass added mockito-
  backed LLM client tests, bringing the count to 91.) Covers: db
  rollback/migration/schema, rank parity fixture, swap atomicity,
  cap enforcement, LLM client auth/rate-limit/timeout/parse paths,
  parse observations, time-travel, rebuild projection, archive list.
- **`pnpm build`** (tsc --noEmit + vite build): clean. 384 modules
  transformed, built in 1.28s. Output: dist/index.html (0.39kB),
  dist/assets/index-*.css (19.10kB), dist/assets/index-*.js (314.99kB).
- **`pnpm test`** (vitest): **7 test files, 85 tests, all passing**
  in 5.17s. Files: rank.parity.test.ts (43), SwapModal.test.tsx (9),
  MoveReasonModal.test.tsx (9), Strip.test.tsx (8), ArchiveView.test.tsx
  (6), BlockModal.test.tsx (5), LanCaptureToast.test.tsx (5).
- **`node scripts/test-store-logic.mjs`**: all store-logic checks
  passed (55 assertions: rank ordering, insertByRank, needsSwap rules,
  session-done tracking, onItemDeleted identity).
- **`python scripts/verify-schema.py`**: NOT YET RUN (requires a live
  bay.db at %APPDATA%\com.bay.desktop\, which only exists after first
  app launch; defer to Phase 2b where we add migration 002 and update
  the verifier to expect user_version=2).

**Baseline established.** Any subsequent phase that turns any of these
red must either (a) fix the regression before the save point, or
(b) file a BLOCKER with the failure detail and mark the relevant task
SUSPECT.

## 2026-06-17T00:45:00Z — Phase 1 bug-fix audit

Audit-and-fix complete. Of the 8 candidate "bugs" in the plan, **1 was
a real code bug** (fixed); **4 were documented design choices or
correct behavior** (confirmed, not bugs); **1 is SPEC-vs-code drift**
(deferred to Phase 3); **2 were clean** (no action needed).

### Fixed
- **`llm/prompt.rs` hardcoded caps** — A/B board headers in
  `format_user_prompt` used literal `/ 5` and `/ 12` instead of
  `A_CAP`/`B_CAP` constants. A future cap change would require editing
  two files. Fixed: import `domain::{A_CAP, B_CAP}`, interpolate.
  Verified: `cargo build` clean; `cargo test llm` 18/18 pass.

### Confirmed NOT bugs (documented design choices / correct behavior)
- **`restore_item` reuses `item_created` Tauri event** — Intentional
  and documented at `commands/items.rs:462-466`. `ITEM_RESTORED` lives
  on the backend event log; the Tauri event channel reuses
  `item_created` as the "now-alive-again" wire signal. Frontend
  `onItemCreated` is idempotent. Not a mismatch — a deliberate
  reconciliation. No change.
- **`set_item_state` done→active at cap** — Correct. Lines 329-343
  check caps on any transition TO active from non-active (covers
  done→active and blocked→active). Matches doctrine: done items don't
  count toward cap, so un-done at cap must refuse. Test at
  `items.rs:1285` pins it. No change.
- **Store `onItemUpdated` `updated_at` idempotency** — Correct. The
  guard at `store.ts:194` deduplicates invoke-resolve + event
  double-delivery (same item, same `updated_at`). "Same-ms double-
  edit" is structurally impossible: each `write_event` is a separate
  tx with its own `unix_ms_now()` ts. No change.
- **`SettingsView` HotkeyInput listener cleanup** — Correct React.
  Effect deps `[capturing, onChange]`; cleanup
  `removeEventListener` runs on unmount-while-capturing. No change.

### Deferred to Phase 3 (SPEC drift, not code bug)
- **`bootstrap` return shape** — Code returns `{items, settings}`
  (`lib.rs:49-52`); SPEC §5.1 says `{items, settings, lanCapture}`.
  Frontend `BootstrapResult` also omits `lanCapture`. Code is
  internally consistent (frontend matches backend); drift is SPEC-vs-
  code. Phase 3 reconciles (likely: amend SPEC §5.1 to match code,
  since the frontend already calls `get_lan_capture_status` separately
  when needed, OR add `lanCapture` to bootstrap if it's genuinely
  wanted at startup — DECISIONS.md ADR at Phase 3).

### Confirmed clean (sweep results)
- **No `dbg!` / `println!` for debugging** in Rust src. The one
  `println!` is `rank_fixture_gen.rs:135` CLI success output
  (appropriate). `eprintln!` calls in capture/hotkey/keychain/lib/
  settings/parse are operational stderr diagnostics (correct Rust
  idiom for Tauri; not debug leftovers).
- **No `console.log`** in TS. The 19 `console.error` calls are all
  error-path diagnostics (correct; PROMPTS.md §5 forbids `console.log`
  for debugging, not `console.error` for diagnostics).
- **No `TODO`/`FIXME`/`XXX`/`HACK`** in Rust src or TS src (one
  false-positive in `src/test/setup.ts:8` is a comment using "not"
  near "these", not a TODO marker).

**Phase 1 verdict:** v0.1.1's cleanup pass was thorough; only one
real bug found (prompt.rs magic numbers). Baseline still green after
the fix.

## 2026-06-17T01:30:00Z — Phase 2a property tests (non-LLM oracle)

15 property tests added across the 6 critical modules' surfaces. These
are the Externality Principle's mechanical checks — structural laws
that must hold for ALL valid inputs, independent of anyone's
interpretation of expected output. If a future refactor breaks an
invariant, the property tests catch it where unit tests (which assert
specific cases) might miss it.

**Oracle: property tests (proptest 1.11, dev-dep).**

### rank_between (domain/rank.rs) — 8 property tests
- `prop_strictly_between_both_bounded` — rank_between(a,b) strictly
  between a and b for all valid (a,b) with a < b.
- `prop_strictly_below_upper_bound` — rank_between(None, b) < b.
- `prop_strictly_above_lower_bound` — rank_between(a, None) > a.
- `prop_unbounded_returns_nonempty` — rank_between(None, None) is a
  valid non-empty rank with no trailing '0'.
- `prop_never_trailing_zero` — the no-trailing-zero invariant holds
  for ALL valid inputs (the precondition for future rank_between calls).
- `prop_front_inserts_are_monotone_decreasing` — repeated front-insert
  produces a strictly decreasing sequence.
- `prop_end_inserts_are_monotone_increasing` — repeated end-insert
  produces a strictly increasing sequence.
- `prop_midpoint_inserts_stay_between_bounds` — repeated midpoint
  insert between fixed bounds stays strictly between.

### apply_event_to_projection + rebuild_projection (commands/events.rs) — 2 property tests
- `prop_rebuild_reproduces_items_for_any_event_sequence` — **THE load-
  bearing property**: for ANY valid event sequence (random interleaving
  of create/edit/move/state/delete/restore), rebuild_projection
  (wipe + replay all events) reproduces the items table exactly.
- `prop_get_items_at_now_matches_live` — get_items_at(now) matches the
  live non-deleted projection for any event sequence (time-travel-to-
  now == live state).

### swap_move_inner + cap enforcement (commands/items.rs) — 4 property tests
- `prop_cap_a_never_exceeded_under_creates` — A active count never
  exceeds A_CAP under any number of creates (extra creates return
  CAP_EXCEEDED, count stays at cap).
- `prop_cap_b_never_exceeded_under_creates` — same for B / B_CAP.
- `prop_inbox_and_c_unbounded` — Inbox and C never hit CAP_EXCEEDED.
- `prop_swap_move_preserves_active_counts_and_atomicity` — after a
  successful swap into full A: entering-tier count unchanged (one in,
  one out), leaving_dest +1, two ITEM_MOVED events with adjacent ids
  + shared ts (single-tx atomicity).

### write_events (db/mod.rs) — 1 property test
- `prop_write_events_rolls_back_for_any_failing_position` — for ANY
  position of the failing event in a multi-event batch (first, middle,
  last), the entire batch rolls back (zero events, zero projection
  changes). Generalizes the existing single-scenario rollback test.

**Test count: 106/106 passing** (up from 91 at baseline; +15 property
tests). `cargo build` warning-clean. `proptest` dev-dep added per
ADR-003.

## 2026-06-17T02:00:00Z — Phase 2b DB-enforced invariants (migration 002)

The CLAUDE.md "events is append-only" prohibition is now a **runtime
trigger-enforced truth**, not just prose. Migration `002_invariants.sql`
adds:

**Oracle: runtime (SQLite triggers + CHECK constraints).**

### events append-only triggers
- `events_no_update` — `BEFORE UPDATE ON events` raises
  `ABORT 'events is append-only (Bay doctrine): UPDATE refused'`.
- `events_no_delete` — `BEFORE DELETE ON events` raises
  `ABORT 'events is append-only (Bay doctrine): DELETE refused'`.
- INSERT (the only legal write path via `db::write_events` →
  `events::append_event`) is unaffected.

### items CHECK constraints (table rebuild)
- `length(content) BETWEEN 1 AND 4096` (SPEC §4.3; matches Rust
  `MAX_CONTENT_LEN` counted as Unicode scalar values).
- `length(rank) >= 1` (rank never empty).
- `deleted IN (0, 1)` (soft-delete flag is boolean).
- `state != 'blocked' OR blocked_reason IS NOT NULL` (SPEC §3.1 guard:
  blocked requires a reason — previously enforced only in
  `set_item_state_inner`).
- Existing `tier`/`state` CHECKs from migration 001 preserved.

### Tests proving enforcement (4 new, db/mod.rs)
- `events_append_only_trigger_blocks_update` — direct `UPDATE events`
  ABORTs with "append-only"; row unchanged.
- `events_append_only_trigger_blocks_delete` — direct `DELETE FROM
  events` ABORTs with "append-only"; row remains.
- `events_append_only_trigger_allows_insert` — 5 sequential
  `write_event` calls succeed; trigger doesn't over-fire.
- `items_check_constraints_reject_invalid_rows` — direct INSERTs
  violating each CHECK (deleted=2, empty content, empty rank,
  blocked-without-reason) all ABORT; valid row inserts cleanly.

### Infrastructure updates
- `db/mod.rs` `MIGRATIONS` const now includes `(2, include_str!(...002...))`.
- Migration version tests updated to expect `user_version=2`.
- `scripts/verify-schema.py` rewritten to load expected CREATEs from
  ALL migration files (later overrides earlier), expect v2, and
  include triggers in the schema object set.

**Test count: 110/110 passing** (up from 106; +4 trigger-enforcement
tests). `cargo build` warning-clean. `PRAGMA user_version=2`.

The four load-bearing invariants are now enforced at THREE layers:
(1) Rust handlers (convention + validation), (2) property tests
(non-LLM oracle), (3) DB triggers + CHECKs (runtime mechanical). A
future code path that tries `UPDATE events SET ...` or writes an
invalid item row will ABORT at the storage layer regardless of what
the Rust code does.

## 2026-06-17T03:00:00Z — Phase 2e two-pass verification (in flight)

**Oracle gate (non-LLM) — GREEN.** This is the load-bearing gate per
AUTONOMY_CHARTER §9. All 6 critical modules have BOTH:
- Property tests (Phase 2a): 15 property tests covering rank bounds,
  projection determinism, swap atomicity, cap enforcement, write_events
  rollback, get_items_at monotonicity. All pass.
- Operator golden cases (Phase 2c): projection.json (7), swap.json (6),
  caps.json (12), rank.json (42 mirrored from scripts/rank-fixtures.json).
  All _status:proposed pending operator freeze. CI check
  (scripts/check-golden.py) green.

**Cold-context LLM verifier pass (pass 2) — DISPATCHED, IN FLIGHT.**
Two `verifier` subagents dispatched in parallel with cold context (no
pass-1 implementer reasoning):
- Verifier A: event/projection spine (write_events,
  apply_event_to_projection, rebuild_projection, get_items_at,
  ProjectionEvent firewall, append-only trigger).
- Verifier B: mutation/rank spine (swap_move_inner, rank_between,
  cap enforcement in create/move/set_item_state).

Their findings will be appended here on completion. Any BLOCKING
drift finding will be fixed before Phase 3 commits; STRUCTURAL findings
logged for operator review; COSMETIC noted.

**Test count at Phase 2e dispatch: 113/113 passing.** cargo build
warning-clean. The non-LLM oracle gate is the authority; the cold-
context pass is defense-in-depth against rationalization drift.

## 2026-06-17T18:30:00Z — I-19 batch operations (resumed run)

Backend completed and corrected after the substrate handoff. Oracle:
characterization + regression tests (batch ops are command-layer over
the critical write_events primitive; the cap path is critical-adjacent).

### Backend (commands/items.rs, commands/events.rs)
- batch_set_state_inner / batch_delete_inner: atomic via write_events
  (whole batch lands or none). affected_ids derived from returned
  events (NO_OP excluded).
- **Cap bug fixed:** incremental projected counters close the
  build-before-apply gap. Regression test
  `batch_set_state_to_active_rolls_back_on_cap_overflow` (A: 3 active +
  3 blocked → unblock all 3 → CAP_EXCEEDED, full rollback, still 3
  active, all 3 still blocked).
- 9 batch tests in items.rs: mark-done, NO_OP skip, blocked-requires-
  reason, shared-reason, cap-overflow rollback, within-cap success,
  missing-item rollback, batch-delete-all, batch-delete missing-item
  rollback.
- undo_last_action generalized to (ts, type) grouping → batch-undo.
  3 undo tests in events.rs: undo_batch_delete (restores all 3),
  undo_batch_set_state (reverts both), undo_swap (the swap-undo path,
  previously untested).

### Frontend (store.ts, Strip.tsx, BatchActionBar.tsx)
- Multi-select store state + actions; per-strip checkbox + shift-click
  range; BatchActionBar with cap-error surfacing + two-step delete.
- 5 BatchActionBar vitest specs (empty render, count+actions, mark-done
  clears selection, cap error keeps selection, two-step delete).

### Gates (all green)
- cargo test **139/139** (+12 from 127 at I-18); cargo build warning-clean.
- pnpm build clean (387 modules); pnpm test **90/90** (+5).
- node scripts/test-store-logic.mjs green.

Phase 4 (I-15..I-19) complete.

---

## Oracle taxonomy (v7, for reference)

| Oracle | When it catches | Used for |
|---|---|---|
| Operator golden cases | Joint-wrong (tests+code agree, both wrong vs human intent) | All 6 critical modules (Phase 2c) |
| Property/metamorphic tests | Systematic error (shared blind spot) | All 6 critical modules (Phase 2a) |
| Cold-context LLM verifier (two-pass) | Drift (implementer rationalization) | All 6 critical modules (Phase 2e) |
| Observed runtime behavior | "Does the system do the thing?" | Smoke tests, integration |

The non-LLM oracle (golden + property) is the gate for critical
modules. The LLM verifier is the second pass. Both required.
