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
