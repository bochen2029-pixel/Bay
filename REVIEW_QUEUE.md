# REVIEW_QUEUE — v0.3 "Execution" run, 2026-07-26

> Accept/reject queue for the autonomous work done under the operator
> directive of 2026-07-26 ("proceed at your best recommendation — most
> ambitious and most aggressive while maintaining highest quality").
> Ordered **cheapest-to-verify first**. Each item: what changed, how to
> check it, and the exact revert.
>
> Gates at session end: `cargo test` **206/206**, `cargo build`
> warning-clean, `pnpm build` clean, `pnpm test` **106/106**,
> `node scripts/test-store-logic.mjs` green,
> `python scripts/check-golden.py` green.
>
> The run is **not released**: `.run-lock` remains, there is no tag, and
> ~9 commits sit unpushed. This is a reviewable checkpoint.

## Commits this session (newest first)

| sha | what | revert |
|---|---|---|
| (docs) | README v0.3 + FUTURE_WORK rewrite + this queue | `git revert <sha>` |
| 3b0c0ea | docs(v1.9): doctrine co-pass (CLAUDE v1.9 / SPEC v1.8 / PROMPTS v1.6) | `git revert 3b0c0ea` |
| daad097 | feat(P5c): Mirror v1 + Today lane + day ceremonies | `git revert daad097` |
| fd2ac23 | feat(P5c): sessions + FocusBar | `git revert fd2ac23` |
| 5812d39 | feat(P5c): execution core 1 — first_step, Today overlay, day rituals | `git revert 5812d39` |
| 82f7372 | feat(I-21): recurring tasks | `git revert 82f7372` |
| 686af06 | feat(P5b): undo groups by txn_id (Q01 closed) | `git revert 686af06` |
| aba6082 | feat(P5b): migration 003 — event envelope v2 | `git revert aba6082` |
| de37921 | feat(P5a): golden RUNNER | `git revert de37921` |
| 1e9ae4f | chore(run): resume + ADR-007 + VISION.md | `git revert 1e9ae4f` |

Reverting is layered: I-21 and the execution core assume the envelope
(003). Revert from the top down, not out of the middle.

---

## Review items — cheapest to verify first

### 1. Corrected golden cases — **the one thing only you can settle**
`contracts/golden/caps.json` cases #5, #6, #8 were *proposed*, never
frozen — and executing them (item 2) showed all three contradicted both
their own case names and frozen doctrine:

- **#5** "blocked item doesn't count: 5 active + 1 blocked in A" — the
  original ops blocked one of five A items and then expected the very
  next create to `CAP_EXCEEDED` at 4 active. Doctrine says blocked
  doesn't count, so 4 active < 5 and that create **must succeed**.
- **#6** — same defect for done items.
- **#8** — expected `a_done_after: 0` after a transition that
  `CAP_EXCEEDED`s; a failed transition mutates nothing, so the item
  stays done.

Each carries a `_corrected` note recording what changed and why.
**Verify:** read the three cases against CLAUDE.md §1 ("blocked and
done items do not count against caps"). **Then freeze them** (set
`_status: "frozen"`) — or correct them differently if my doctrine
reading is wrong, which is exactly the JOINT_WRONG this mechanism
exists to catch. Revert: `git checkout de37921~1 -- contracts/golden/caps.json`.

### 2. Golden RUNNER (verify: `cargo test golden_`)
`src-tauri/src/golden_runner.rs` executes projection (7), swap (6), and
caps (12) cases against the real `*_inner` functions. Panics on any
unrecognized op or expectation key — no silent skips. This closes the
gap that let the P2e JOINT_WRONG through: `check-golden.py` only ever
checked that cases *existed*. In de37921.

### 3. Two new doctrine laws to ratify (verify: read CLAUDE.md §7, §10)
- **§7 caps bind flow** — Today ≤3 active, one open session. I argue
  this is not a second prioritization scheme (no new axis; a day-scoped
  WIP limit over the existing one), but it is the closest this run
  comes to the cut list, so it deserves your explicit yes or no.
- **§10 system-actor timer** — the day-roll is the first machine write
  in Bay's history. It touches Today membership only, is always logged,
  and undo ignores it. The alternative (expire lazily on next open, no
  system actor at all) is a ~20-line change if you'd rather have zero
  machine writes.

### 4. Frontend surfaces (verify: `pnpm test`, then run the app)
FocusBar, Today lane + picker + day-close, MirrorView, Strip ▶/🔁, the
Repeat menu. 11 new vitest specs. In fd2ac23 / daad097.

### 5. Mirror definitions (verify: read SPEC §12, then the code)
Every figure is defined in SPEC §12. The judgment calls worth checking:
the A-leak window is **48h**; avoidance means **zero sessions ever**
(not "none this window"); still-open block intervals count to now; the
"A is a second inbox" sentence fires at **≥40%**. All are one-line
changes. In daad097.

### 6. I-21 cap semantics (verify: `cargo test recurr`, `cargo test spawn`)
An **active** parent frees its own slot so the child fits its tier; a
**blocked/done** parent's child would add to the tier, so at cap it
routes to **Inbox** rather than failing the completion. The alternative
(refuse the completion) would make "mark done" fail, which I judged
worse than an Inbox landing. In 82f7372.

### 7. Undo semantics (verify: `cargo test undo`)
Three classes are no longer undo targets: LLM advisory rows (unchanged),
`actor: system` transactions, and **session events**. The last is a
philosophical call worth your eye: undo reverts what a session *did* to
the board, never the record that you spent the time. In 686af06 /
fd2ac23.

### 8. Envelope + hash chain — **the largest single change**
(verify: `cargo test envelope`, `cargo test chain`, then launch the app
and check the console line "event chain verified: N rows")
Migration 003 adds six nullable columns + a `meta` table via `ALTER ADD
COLUMN` (never a rebuild of `events`). `sha2` is a new **runtime**
dependency (ADR-008). Legacy rows stay valid and are tolerated at the
chain head. If you want the smaller change, the txn_id column alone
suffices for Q01 — the other five columns buy provenance, sync-
readiness, payload evolution, and tamper-evidence. In aba6082.

### 9. QUESTIONS Q01 → CONFIRMED (verify: read QUESTIONS.md)
The heuristic is gone; grouping is exact. This is the decision you
explicitly deferred at the last boundary, now taken under your
directive and recorded in ADR-007.

### 10. Deferrals to ratify (a NON-action)
Not built, still gated: **T4** LLM Today-draft (adjacent to the
capture-time-suggestion ban), **T5** calendar ICS read (touches "no
network dependency"), **T8** sync. Each needs a doctrine line before
build. Also not done: a **cold-context verifier pass** over these diffs
— dispatched twice, the first died on a substrate quota limit. That is
the top item in FUTURE_WORK (F-02) and the main reason I'd call this
"reviewable" rather than "verified".

## Complacency canary

Every gate is green, and green gates have lied before in this repo
(P2e: 143 passing tests, two BLOCKING bugs). What actually earned trust
this run: **executing** the golden cases immediately found three broken
ground-truth cases, and the new property tests (Today cap under any
interleaving; chain verification over any write sequence) are the kind
of assertion that fails when I'm wrong rather than when I'm sloppy. The
missing leg is the cold verifier — until F-02 runs, treat items 5–8 as
plausible rather than confirmed.
