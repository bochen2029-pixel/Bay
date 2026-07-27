# RUN_STATE — Bay v0.3 "Execution" run (2026-07-27)

> Postcard to future-self. Kept current enough to BE the compaction
> brief. If this file is stale, the run is lost. Last updated:
> 2026-07-27, during the verification chain (pass 8 in flight).

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

**A chain of cold verification passes, each reviewing the last one's
fix.** Record: **7 for 7** — every fix commit contained something.
Severity fell monotonically, and the last two passes found **no
incorrect behaviour in the shipped code at all**.

| pass | subject | verdict |
|---|---|---|
| 1 | v0.3 feature work | FAIL — BLOCKING Today-cap bypass (open) |
| 2 | `b884b4d` (pass-1 fix) | FAIL — BLOCKING cap **escape** introduced by the fix |
| 3 | `8f4592e` (pass-2 fix) | FAIL — 3 MAJOR, order-dependence (fails closed) |
| 4 | `0562957` (pass-3 fix) | FAIL — BLOCKING: accepting `done` on a *blocked* item killed Ctrl+Z permanently |
| 5 | `a0f4775` + `1e7467d` | FAIL — 2 MAJOR, no BLOCKING: the ordering *key* was still ops-derived |
| 6 | `ea2fb64` + the mutation gate | FAIL — 3 MAJOR, all *guard* holes; "no incorrect behaviour in the shipped code" |
| 7 | `82a2842` + `25e7138` | PASS w/ 3 MAJOR — again all guard holes, all at *siblings of a fixed door* |
| 8 | `723a0bb` + `14b1111` | dispatched |

Two structural repairs carry the run. Pass 3's: `apply_reorg_inner` now
runs in **two passes** — pass 1 applies the human's ops (cap check on
those alone), pass 2 resolves derived effects (recurrence spawns, Today
overflow) from the FINISHED simulation. The outcome is a function of
the op set, not its order, and a derived effect can never fail a legal
diff. Pass 5's: `board_order(orig, sim, id) -> (tier, rank, id)` keys
every contest on the **pre-diff** board, so the model's listing order
cannot decide who wins.

**The pattern worth carrying (passes 6–7).** The defects have left the
implementation and now live in the safety net: a bug gets fixed at
every door at once, but the guards accrete only at the door the review
named. Pass 7 found the accept path's `Done` arm holding a test, a
golden case AND a mutation while its identical `Active` twin one match
arm below held none. Same for `board_order`'s `id` tiebreak vs its
`tier` sibling. **After fixing anything, grep for its siblings and
guard each.**

## Next concrete actions

1. **Read pass 8's findings** when they arrive; fix and re-gate. Two
   consecutive behaviourally-clean passes mean the chain is close to
   its end condition — but "this verifier stopped finding things" is
   not "the code is correct," and ending the chain is an operator call.
2. **F-01 (operator, now the top blocker)**: freeze `caps.json`
   #5/#6/#8 and all 16 `today.json` cases. Until then the ground truth
   is agent-authored — the condition the Externality Principle exists
   to end. Note the agent has since AUTHORED a growing share of those
   cases, which sharpens the point.
3. Ratify CLAUDE laws 7 and 10.
4. F-03 I-22 streaming; F-04 coach v2 over session aggregates.
5. VISION v0.4 (wake dates, weekly review, C-bankruptcy, triage flow).
6. Do NOT build T4 / T5 / T8 — still operator-gated (VISION §7).
7. Before calling v0.3 permanent: dogfood a week and run the
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

Gates: cargo **247/247** (from 152), vitest **118/118** (from 93), both
builds clean, warning-clean, store-logic + check-golden (5 files, 16
today cases), `verify-schema.py --fresh` (13 objects, v6),
`check-reachability.py` 39/39, and **`scripts/check-mutations.py`
20/20** — one mutation per defect a cold pass found, each caught by a
specifically-named test, none by a mere compile error. Speculations: 0
open (Q01 CONFIRMED and closed). Blockers: 0. ~20 commits ahead of
origin, **unpushed, untagged**.

Defects found + fixed this run: 3 defective golden cases (ground truth,
found by executing them); 3 BLOCKING, 11+ MAJOR and many MINOR across
seven cold passes; 3 in `verify-schema.py` (found by finally running
it); and several in the mutation gate itself (found by auditing the
tool that judges the evidence).

**Two lessons, in order of how expensive they were to learn.**
(1) *A check that exists but never executes is not a check* — golden
cases counted not run, `verify-schema.py` broken since migration 002,
a cap enforced only at remembered doors, two registered commands with
no UI. (2) *A check that executes but cannot fail is not a check
either* — an order-independence property built on
`proptest::sample::subsequence`, which preserves order and so compared
one ordering against itself; a policy test that asserted a contest
happened but never who won; and a correct fix that silently flipped a
golden case's board so the item under test was never reached.
`check-mutations.py` exists to make the second class detectable, and
**the standing rule is: when a review finds a defect, add its
mutation — and guard every sibling of the door it named.**

## Pointer back

TASKLIST.md (P5a/P5b/P5c DONE) and PROGRESS.md are canonical.
AUTONOMY_CHARTER governs. REVIEW_QUEUE.md is the operator's entry
point. FUTURE_WORK.md scopes what's left; VISION.md is design source,
not doctrine — including §9, which states what would falsify each
mechanism this run shipped.
