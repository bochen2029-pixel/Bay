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
