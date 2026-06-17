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
