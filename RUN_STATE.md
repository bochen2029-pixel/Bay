# RUN_STATE — Bay v0.3 "Execution" run (2026-07-26)

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Last updated:
> 2026-07-26, session-end consolidation.

## Run identity

Resumed from the clean I-20 pause under the **operator directive of
2026-07-26** ("proceed at your best recommendation — most ambitious and
most aggressive while maintaining highest quality"). Dispositions in
DECISIONS **ADR-007**; the `sha2` dep in **ADR-008**. Design source:
`VISION.md` (written this session, non-doctrine). Substrate: Claude
Fable 5, continued on Claude Opus 5 after a mid-run quota limit.

## What shipped this run (all committed, all green)

1. **P5a golden RUNNER** (de37921) — `contracts/golden/*.json` execute
   under `cargo test`; found + corrected 3 defective *proposed* caps
   cases (freeze pending).
2. **P5b migration 003 envelope v2** (aba6082) — txn_id / actor /
   origin / device_id / schema_ver / prev_hash + `meta` + SHA-256 chain
   + boot verification. `sha2` runtime dep.
3. **P5b undo by txn_id** (686af06) — **QUESTIONS Q01 CONFIRMED**.
   System + session txns are not undo targets.
4. **I-21 recurring tasks** (82f7372) — migration 004, RRULE subset,
   spawn-on-done in one txn, Inbox overflow, 🔁 UI.
5. **P5c execution core 1** (5812d39) — migration 005, first_step,
   Today overlay (cap 3), day open/close/roll (the one system write).
6. **P5c sessions + FocusBar** (fd2ac23) — migration 006, `sessions` as
   a second projection table, ≤1 open (index-enforced).
7. **P5c Mirror v1 + Today lane** (daad097) — deterministic stats, no
   LLM; TodayLane, day ceremonies UI, MirrorView.
8. **Doctrine co-pass** (3b0c0ea) — CLAUDE v1.9 (laws 7–10) / SPEC v1.8
   (§4.0 envelope, §4.7 runner, §3.7 Today, §3.8 sessions, §12 Mirror) /
   PROMPTS v1.6 (six shipped increment prompts).

## Current task

**Session close — done.** The cold-context verifier ran (Opus, cold,
`de37921..HEAD`) and returned **FAIL**; every finding is fixed and
regression-tested (commit b884b4d): a BLOCKING Today-cap bypass on the
re-activation/restore/accept doors, a MAJOR missing recurrence spawn in
the LLM accept path, six MINORs, and three long-standing bugs in
`verify-schema.py` (broken since migration 002, never noticed because
it required a live DB — now runnable with `--fresh`).

## Next concrete actions

1. **F-01 (operator)**: freeze the corrected `caps.json` cases.
2. A second cold pass over **b884b4d itself** — the fixes were written
   in response to the verifier and have not been cold-reviewed.
3. F-03 I-22 streaming; F-04 coach v2 over session aggregates.
4. VISION v0.4 (wake dates, weekly review, C-bankruptcy, triage flow).
5. Do NOT build T4 / T5 / T8 — still operator-gated (VISION §7).
6. Before calling v0.3 permanent: dogfood a week and run the
   `VISION.md` §9 falsification checks. No test suite can answer them.

## Blocking issues

None. The verifier gap is a quality gap, not a blocker.

## Subtleties to preserve across compaction

- **Envelope**: `write_events_ctx` stamps one `txn_id` per call;
  payload is serialized ONCE and the same bytes are inserted + hashed;
  the chain tail is read INSIDE the write tx. Legacy (NULL-envelope)
  rows are tolerated only at the chain HEAD.
- **Undo**: groups by `txn_id`; the legacy `(ts,type)` fallback is
  fenced with `txn_id IS NULL`. Skips `actor='system'`, LLM rows, and
  SESSION_* rows. Audit rows (`ITEM_RECURRED`, `DAY_*`) are skipped
  during compensation but do not prevent targeting their txn.
- **Recurrence caps**: active parent frees its own slot → child fits;
  blocked/done parent at cap → child to **Inbox**. `SpawnAccounting`
  is shared across a batch (net-active per tier + rank chaining).
- **Today**: an OVERLAY (`items.today_on`), never a tier. Cap 3
  **active** — a done Today item keeps membership but frees its slot.
  The FRONTEND owns the local date; Rust never computes calendar days.
- **Sessions**: `sessions` is a projection table — `rebuild_projection`
  truncates and replays BOTH tables. ≤1 open via a partial UNIQUE index
  over a constant. Behavior records are never undone.
- **Firewall**: `ProjectionEvent` has 13 variants, zero LLM. The `None`
  arm now means "no projection effect" and covers LLM advisory +
  audit/link events. Do not read that as a firewall weakening.
- `SCHEMA_VERSION` derives from `MIGRATIONS` — new migrations no longer
  require touching version-pin tests.
- Test-pool gotcha: `max_size(1)` in tests means an open `conn` blocks
  the next write. `drop(conn)` before a subsequent command call.
- All pre-existing subtleties from the v0.2.0 run still hold
  (write_events is the only write path; restore reuses `item_created`
  BY DESIGN; rank fixtures regen only via `rank_fixture_gen` + `SPEC:`;
  the `pre-arch-edit` hook needs `SPEC_AMENDMENT.md` — it EXISTS, with
  a third pass appended; extend it, don't delete it).

## Runway snapshot

Gates: cargo **216/216** (from 152), vitest **106/106** (from 93), both
builds clean, warning-clean, store-logic + check-golden green,
`verify-schema.py --fresh` green (13 objects, v6). Speculations: 0 open
(Q01 CONFIRMED and closed). Blockers: 0. ~12 commits ahead of origin,
**unpushed, untagged**.

Defects found + fixed this run: 3 defective golden cases (ground-truth,
found by executing them); 1 BLOCKING + 1 MAJOR + 6 MINOR (found by the
cold verifier); 3 in `verify-schema.py` (found by finally running it).
**Every one of them was a check that existed but had never executed** —
that is the lesson worth carrying into the next run.

## Pointer back

TASKLIST.md (P5a/P5b/P5c DONE) and PROGRESS.md are canonical.
AUTONOMY_CHARTER governs. REVIEW_QUEUE.md is the operator's entry
point. FUTURE_WORK.md scopes what's left; VISION.md is design source,
not doctrine — including §9, which states what would falsify each
mechanism this run shipped.
