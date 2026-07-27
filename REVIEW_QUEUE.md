# REVIEW_QUEUE — v0.3 "Execution" run, 2026-07-26 → 27

> Accept/reject queue for the autonomous work done under the operator
> directive of 2026-07-26 ("proceed at your best recommendation — most
> ambitious and most aggressive while maintaining highest quality").
> Ordered **cheapest-to-verify first**. Each item: what changed, how to
> check it, and the exact revert.
>
> Gates: `cargo test` **271/271**, `cargo build` warning-clean,
> `pnpm build` clean, `pnpm test` **118/118**,
> `node scripts/test-store-logic.mjs` green,
> `python scripts/check-golden.py` green (5 files, 16 today cases),
> `python scripts/verify-schema.py --fresh` green (13 objects, v6),
> `python scripts/check-reachability.py` green (39/39),
> `python scripts/check-mutations.py` green (**46/46 caught**).
>
> One result worth pulling out of that list: the `caps/restore-ignores-cap`
> mutation — which reintroduces P2e BLOCKING-2, where restoring from the
> archive could push A past 5 — is caught by a Rust test **and by
> `golden_caps_cases_execute`, i.e. by operator-authored case #12**. That
> is the Externality Principle paying out exactly as designed: ground
> truth the agent did not write, catching a regression in the product's
> central invariant. It is also the argument for freezing the rest.
>
> **Ten cold-context verifier passes ran, each reviewing the previous
> one's fix. All ten found something.** Every finding is fixed and
> regression-tested. Read item 0 first — the chain is the most
> informative artifact this run produced, more so than any feature in
> it. The last **five** passes found **no incorrect behaviour in the
> shipped code**; every finding was a hole in the safety net.
>
> The run is **not released**: `.run-lock` remains, there is no tag, and
> **82 commits** sit unpushed. This is a reviewable checkpoint.
>
> **What actually needs you** (nothing else is blocked on a human):
> 1. **Freeze the golden cases** — `caps.json` #5/#6/#8 and all 16
>    `today.json` cases are `_status: proposed`, so the system's ground
>    truth is still agent-authored. Sharpened by the chain: the agent
>    has since authored a growing share of "the only assertions in the
>    system the agent did not author," and pass 6's headline finding was
>    an agent edit silently disarming one of them.
> 2. **Ratify CLAUDE laws 7 and 10** (caps bind flow; the system acts
>    only on a human-set timer).
> 3. **QUESTIONS Q02 — optional, and no longer urgent.** Intra-tier
>    placement inside an accepted diff follows the model's listing
>    order. When I filed this I claimed SPEC overstated the invariant
>    and that doing nothing "should not survive review"; **both were
>    wrong** — SPEC §8.7 has documented this exact exception since
>    `a0f4775`, and I asserted what SPEC said without reading it. Doing
>    nothing is legitimate. The only live choice is whether to
>    board-derive the rank too, which costs one sort and lets §8.7 drop
>    its caveat.
> 4. **Decide whether the verification chain ends here.** Ten passes ran;
>    the last five found no incorrect behaviour in the shipped code, and
>    pass 10's subject range contained no product code at all. Both that
>    verifier and I think the format is spent, and that the honest next
>    step is **dogfooding against `VISION.md` §9's kill conditions** —
>    the one source of information no further pass can produce. Nothing
>    is blocked on this; it is a question of where to spend the next
>    hour.

## Commits this session (newest first)

| sha | what | revert |
|---|---|---|
| 0c5813b | fix(ci): make the mutation window visible to concurrent readers | `git revert 0c5813b` |
| 06d1f55 · da799ae | test: permute MOVE ops too (pass-6 gap); file **QUESTIONS Q02** | `git revert <sha>` |
| 723a0bb · 14b1111 | **test: pass-7 guards** — the unblock door, `child.today_on`, the id tiebreak | `git revert <sha>` |
| 82a2842 · 25e7138 | **fix: pass-6** — re-arm the disarmed golden case; pin the tier key + Today door | `git revert <sha>` |
| ea2fb64 · c46128b | **fix: pass-5** — contests keyed on the pre-diff board; **mutation gate added** | `git revert <sha>` |
| a0f4775 · 1e7467d | **fix: pass-4 BLOCKING** — undo-killing `blocked_reason` drop; golden accept door | `git revert <sha>` |
| 0562957 | **fix: pass-3** — derived effects from the finished simulation | `git revert 0562957` |
| 8f4592e · 55fcc02 | **fix: pass-2 BLOCKING** — one ledger per transaction; `today.json` created | `git revert <sha>` |
| 0b225bc | feat(F-04): coach v2 — recorded behaviour in the analyze context | `git revert 0b225bc` |
| 912c3b7 · 4701d76 · 2432ded | fix(ui): `first_step` + `add_to_today` were unreachable; **reachability gate** | `git revert <sha>` |
| b884b4d | **fix: pass-1 BLOCKING** — Today-cap re-entry bypass + accept-reorg recurrence | `git revert b884b4d` |
| 887c8c1 | docs: README v0.3 + FUTURE_WORK rewrite + this queue | `git revert 887c8c1` |
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

### 0. The verification chain — **read this first**

Ten cold passes ran, each reviewing the previous one's fix. The
result is the most important thing on this page:

| pass | subject | verdict |
|---|---|---|
| 1 | the v0.3 feature work (`de37921..daad097`) | **FAIL** — 1 BLOCKING, 1 MAJOR, 6 MINOR |
| 2 | pass 1's fix commit (`b884b4d`) | **FAIL** — the fix introduced a **worse** bug than it fixed |
| 3 | pass 2's fix commit (`8f4592e`) + `today.json` | **FAIL** — 3 MAJOR, 5 MINOR: the cap escape was gone, but the fix had made the accept path *order-dependent* |
| 4 | pass 3's fix commit (`0562957`) | **FAIL** — 1 BLOCKING (accepting `done` on a *blocked* item killed Ctrl+Z permanently), 3 MAJOR (ordering, one layer down) |
| 5 | pass 4's fix (`a0f4775`) + golden accept-door coverage (`1e7467d`) | **FAIL** — 2 MAJOR, **no BLOCKING**: the ordering *key* was still ops-derived, and the contest policy was indistinguishable from its inverse |
| 6 | pass 5's fix (`ea2fb64`) + the mutation gate | **FAIL** — 3 MAJOR, but *"no incorrect behaviour in the shipped code; every finding is a hole in the guard"* |
| 7 | pass 6's fix + the guards the gate itself exposed | **PASS with 3 MAJOR** — *"no incorrect behaviour found in the shipped code"*; all three MAJORs are lines the round **leaned on** while adding guards beside them |
| 8 | pass 7's fix (three guards + the gate's own repairs) | **FAIL, 2 MAJOR** — *"I found no incorrect behaviour in the shipped code"*; both MAJORs were the two blocked-reason doors the session had **already fixed independently** while the pass ran |
| 9 | pass 8's fix + the sibling audit + the mixed-op permutations | **PASS, 2 MAJOR, no BLOCKING** — *"I found no incorrect behaviour in the shipped code"*, fourth round running. Both MAJORs were guard holes: the **B** half of a cap counter whose **A** half I had guarded one commit earlier with an A-only fixture, and the row-hash digest (dropping `payload` survived the suite) |
| 10 | pass 9's fix + the generated order-independence property | **PASS, 4 MAJOR, no BLOCKING** — fifth clean round; the subject range held **zero production lines**. Headline finding was against **the gate itself**: it scored an environmental build failure as "caught", printing *"All 33 mutations caught"* with 14 never exercised |

**Pass 10 is where the chain told me to stop, and I agree with it.**
Its own words: the cold-pass *format* has reached diminishing returns —
five rounds with no behavioural defect, and a subject range containing
no product code at all. The evidence it gave is the part that
convinced me: **12 of its own 23 mutations survived** in modules
nothing had ever probed (`capture/`, `settings.rs`, `prompt.rs`, four
undo arms), versus a near-zero survival rate inside `apply_reorg_inner`
— the one function ten passes have been circling.

So the remaining risk was never *depth* on the reviewed code. It was
**breadth**, and breadth is mechanical work, not a job for another
reviewer's judgement. That sweep is now done (see the commits above).

What no further pass can produce is the thing that actually matters:
whether any of this helps a person get work done. `VISION.md` §9 lists
what would falsify each v0.3 mechanism. **Dogfooding is the next
step, not verification.**

**Pass 9 also corrected a factual error I put in front of you.**
`QUESTIONS.md` Q02, as I filed it, asserted *"SPEC §8.7 currently claims
more than the code delivers"* and concluded *"doing nothing is the one
option that should not survive review."* Both false — §8.7 has named
that exact rank exception since `a0f4775` (pass 4), before Q02 existed.
I asserted what SPEC said without reading it, which would have had you
deciding under an urgency I invented. Q02 now carries the correction
and states plainly that **doing nothing is a legitimate outcome**.

**Pass 8 is the one that converged.** Its two MAJORs — `session.rs:157`
and `items.rs:993`, both unguarded doors of the P2e BLOCKING-1 class —
were found *twice, independently*: by the cold verifier reading
`14b1111`, and by the main session applying pass 7's own lesson forward
(*"after fixing a defect, grep for its siblings and guard each"*) and
auditing all four doors that carry the blocked-reason carry. Two
different methods, same two doors, same verdict. That is the strongest
evidence in this queue that the finding is real and the fix is right.

It also means the pattern pass 7 named **repeated inside the very
commit that named it** — the fix was applied at five doors and guarded
at three. That is why the audit is now a standing rule rather than an
observation.

**Read passes 6 and 7 together.** Two rounds running, the cold verifier
could not make the shipped code do anything wrong. The defects have
migrated out of the implementation and into the safety net — and then,
in pass 7, into a specific and embarrassing pattern:

> The `Done` arm of the accept door had a test, a golden case *and* a
> gate mutation for its blocked-reason carry. Its identical twin one
> match arm down — `Active` — had none, and dropping the same line
> there reproduces the same BLOCKING bug through the other door.

The *bug* was fixed at every door at once. The *guards* were only ever
added at the door the report happened to name. Same shape for
`board_order`'s `id` tiebreak (its `tier` sibling got two mutations
that round; the `id` got none) and for `child.today_on = None` (which
sat under a comment arguing it did not matter — while
`effective_active_today` read it out of the same map one loop later).

Each of the three is now closed by a test **and** a mutation, verified
1:1 — every new mutation is caught by exactly the one new test written
for it, and by no other.

**Pass 2 is the one to dwell on.** My fix for the BLOCKING Today-cap
bug bolted the recurrence spawn onto `apply_reorg_inner` — which
reasons over a *simulation* of the accepted diff — while the spawn
decided the child's tier from the *live projection*. Two ledgers, each
blind to the other, so the spawned child was counted by neither:
accepting `[finish the recurring report, unblock the other thing]` with
A at capacity committed **A at 6 active against a cap of 5**. That is a
worse defect than the one I set out to fix, and every test was green.

The repair was structural rather than another patch: `build_recurrence_
spawn` split into caller-owned *placement* and a shared
`recurrence_child_drafts` that only builds the child's content and
dates, so **each transaction now keeps exactly one capacity ledger**.
The same change removed a rank collision that would have thrown on the
next drag, and surfaced two more real defects (spawned children never
reached the UI; duplicate ops spawned two children from one item).

**Pass 3 then found what that repair broke.** The cap escape was
genuinely gone — no commit can exceed A or B — but resolving derived
effects *incrementally inside the op loop* meant an op not yet visited
read as a no-op. So the same accepted set could **commit or fail, and
keep or lose a Today slot, depending on the order the model happened to
list its proposals**. The LLM still has no write path, but that is a
lever on the deterministic tier's result — the spirit of the firewall
if not its letter. Worse, one of my own regression tests had enshrined
the wrong behavior, contradicting the very SPEC line the commit message
cited: a spawn was aborting a legal accept, where doctrine says it must
overflow to Inbox and never fail.

The repair is again structural: **two passes.** Pass 1 applies what the
human accepted and the cap check runs on that alone; pass 2 resolves
what those acceptances *imply* — spawns, Today overflow — from the
finished simulation. The outcome is now a function of the op set, not
its order, and a derived effect can never fail a legal diff.

**One more thing worth your scepticism.** The property test I wrote to
guard order-independence was *vacuous* — it used a helper that
preserves order, so it compared one ordering against itself and
asserted nothing. Only the negative control caught it (it passed when
it should have failed). It is now an exhaustive permutation test,
re-verified by injecting the real defect and watching it fail.

**Pass 4 then found a BLOCKING that had been there since I-20.**
Accepting a `done` proposal on a *blocked* item dropped the outgoing
blocked reason, so Ctrl+Z wrote `state='blocked'` with a null reason,
tripped the migration-002 CHECK, rolled back — and, because undo keeps
targeting the same transaction, **stayed dead**. The accept path was
the last of five done-doors still missing the P2e fix, and my previous
commit had added a second route to it. It also found three more
ordering defects one layer below where pass 3 fixed them, including one
where an item reactivated *and* completed in the same diff lost its
Today membership — contradicting a golden case that was passing.

That last one was the important structural lesson: **`today.json` case
3 stated the rule the accept path was breaking, and passed the whole
time, because the golden runner could not reach `accept_suggestion`.**
The externality existed and pointed at the wrong door. The runner now
drives the accept path (`1e7467d`), with two ACCEPT-DIFF cases,
negative-controlled against both pass-4 defects.

Pass 2's fix is now ordered by **board position** rather than by the
model's array: when two recurrence children contend for one free tier
slot, the higher-ranked parent's child takes it; when two reactivations
contend for one Today slot, the lower-ranked item yields. Both are
answers you can predict from your own board.

**And the structural cause of pass 1's BLOCKING:** the Today law lived
in doctrine and in code but was **asserted nowhere an operator owned**.
`contracts/golden/today.json` now exists — 13 cases, executed by the
runner, verified by negative control (disabling the guard fails the
case by name). Please review and freeze it alongside the caps cases.

---

### 0a. What pass 1 found
A verifier with no access to my reasoning reviewed `de37921..HEAD`
against doctrine and the golden cases. Verdict: **FAIL**, on two real
defects that 206 passing tests did not catch.

- **BLOCKING — the Today cap was bypassable in one click.** A
  done/blocked item keeps its Today membership but frees its slot, so
  *reactivating* it (or restoring it from the archive, or accepting an
  LLM "make active" op) could put **4 active items on one date**.
  Exactly the P2e `restore_item` class: an entry path that skips a cap,
  invisible because no golden case covers Today.
  **Fix:** `day::today_overflow_draft` + shared `TodayAccounting`,
  called from all four doors. It **drops the membership** (logged,
  `cause: user`) rather than refusing the transition — because undoing
  a completion re-activates the item, and if that could fail, **Ctrl+Z
  would break**, which the whole architecture promises never happens.
  *Verify:* `cargo test today` (5 new tests, incl. batch + restore).
  *This is the judgment call worth your eye:* dropping membership is a
  mutation the user didn't explicitly ask for. The alternatives were
  failing the transition (breaks undo) or dropping Today membership on
  completion (loses "finished work stays visible", which is deliberate).
- **MAJOR — the LLM accept-diff never spawned recurrences.** Accepting
  a "mark done" proposal on a repeating item silently ended the series
  — a bug you'd discover weeks later by its absence.
  *Verify:* `cargo test accept_reorg_done_spawns`.
- Six MINORs, all fixed: spawn accounting ignored slots freed by
  non-recurring parents; the Mirror counted a done→undo→done as two
  completions and reported *finished* Today work as "rolled over";
  receipts used `updated_at` instead of the logged completion time; a
  soft-deleted item could strand the Now slot; Today date-moves left
  the log unbalanced. Plus: undo's skip-list is now one const with a
  test pinning it, and the golden runner's rebuild check covers
  `sessions`.
- **`verify-schema.py` had been broken since migration 002** and nobody
  knew, because it could only run against a live database and never
  did — the *same failure shape* as the golden cases that existed but
  never executed. Three real bugs (unmatched `CREATE UNIQUE INDEX`;
  CREATE/RENAME/DROP applied out of source order, which deleted the
  surviving `items` from the expected set; trigger bodies truncated at
  their first inner `;`). Fixed, and it now has a `--fresh` mode that
  builds a throwaway DB from the migrations, so the gate stands on its
  own. *Verify:* `python scripts/verify-schema.py --fresh`.

The pattern across both this and P2e is worth naming: **every gap was a
check that existed but never ran.** The three mechanisms that actually
caught things this run were execution (golden runner), property tests,
and a reader with no stake in my reasoning.

### 1. Golden cases — **the one thing only you can settle**

Two items here, both operator-owned ground truth:

**(a) `contracts/golden/today.json` is new and agent-authored** (13
cases, `_status: proposed`). It encodes the Today law: cap 3 active per
date; a done item keeps membership but frees its slot; every re-entry
door drops membership rather than refusing the transition (because undo
must never fail); `open_day` is atomic and idempotent; the roll is
system-actor and expires only past dates; undo restores the original
date and looks past the roll. Read it against CLAUDE.md §7 and §10, and
freeze or amend.

**(b) the three corrected `caps.json` cases:**
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
build.

## Complacency canary

Green gates have now lied three times in this repo. P2e: 143 passing
tests hiding two BLOCKING bugs. Pass 1: 206 tests hiding a one-click
cap bypass. Pass 2: a fully green suite hiding a *worse* bug that my
own fix had just introduced.

Two patterns, and the second is the more uncomfortable one.

**A check that existed but never executed.** Golden cases counted, not
run. `verify-schema.py`, broken since migration 002 because it needed a
live database and nobody ever pointed it at one. A Today cap enforced
on the doors I remembered. And the deepest version: a law with no
operator-owned assertion behind it at all.

**A fix is a change, and changes carry the same risk as features.** I
was more confident writing the fix than writing the original code, and
the fix was worse. Nothing in the process caught that except reviewing
the fix as if it were new work — which is now the standing rule here.

What earned trust, in order: **executing** the golden cases (3 broken
ground-truth cases found immediately); the **cold passes**, each of
which found something the suite could not; **negative controls** (the
new Today case was proven to fail when the guard is disabled — an
assertion nobody has watched fail is just decoration); and property
tests that quantify over interleavings instead of pinning the one case
I had in mind. What did not earn trust: the test count, and my own
sense that a change was obviously correct.

A fourth pattern arrived with pass 3, and it is the sharpest one:
**a test can be decoration.** My order-independence property compared
one ordering against itself and asserted nothing; every gate stayed
green and I would have shipped it as coverage. The only thing that
exposed it was deliberately breaking the implementation and noticing
the test did not care. **Negative controls are not optional polish —
they are the difference between an assertion and a comment.**

A fifth pattern, and the one I would act on first if you only change
one thing: **every round's defect lived in the blind spot of the test
added that same round.** Five for five. The tests were not weak in
general — each was negative-controlled and each caught its own target.
They were weak in the same *direction* as my attention, because I wrote
both. The counter-measure that has actually worked is not a better
test; it is a reader who did not write the fix.

Pass 5 sharpened that into something actionable. It showed that the
contest policy I had just written into SPEC could be **inverted — or
replaced with a raw UUID sort — with all 237 tests green**, because my
test asserted *that* a contest happened and never *who won*. A
cross-permutation comparison is satisfied by any deterministic rule,
including the wrong one.

So the negative controls I had been running by hand all session are now
a gate: **`scripts/check-mutations.py`** carries one mutation per
defect these reviews found, and the suite must catch every one. It
found a survivor on its first run — a fix from pass 2 that had never
had a test — which is exactly the class it exists for. The standing
rule is now: *when a review finds a defect, add its mutation.*

**Pass 6 is where that gate started paying for itself.** It reported
*"no incorrect behaviour was found in the shipped code — every MAJOR is
a hole in the guard"*, which is the first round where the code was
right and only the safety net had gaps. The sharpest was mine: making
golden case ranks *real* (a correct fix) silently **flipped that case's
board**, so the finished item became the best-ranked contender, the
eviction loop reached the other item first, and the case could no
longer fail. An operator-owned oracle went quiet as a side effect of a
fix — and nothing would have told me, because it still passed.

Then the gate found three of the guards I added *that same round* to be
decoration, naming each one. All are closed; **20 mutations, every one
caught by a specifically-named test, none by a mere compile error.**

Pass 7 then turned that same lens on the gate itself, and it deserved
it. Its "which test caught this" report — the very output used to
condemn three guards as decoration — printed only the
*alphabetically first* failing test, which had been quietly hiding
`golden_runner::golden_today_cases_execute` (the re-armed operator
oracle, that round's headline fix) behind a "+1". Worse, **any**
non-zero cargo exit scored as *caught*, so a flake could certify an
unguarded line. Both fixed; the gate now reports every failing test and
treats a red suite that names none as `INCONCLUSIVE`.

Honest caveats that remain:

- **The chain is 7 for 7 and pass 8 is dispatched.** Severity has
  fallen monotonically — BLOCKING-open, BLOCKING-closed, MAJOR,
  BLOCKING-at-a-missed-door, MAJOR-only, guards-only, **guards-only
  with no behavioural finding at all** — which is convergence rather
  than correctness. Do not treat the newest commit as verified.
- **Two rounds of "the code is right" is not proof the code is right.**
  It is evidence that this particular verifier, reading these
  particular files, has stopped finding things. That is exactly when a
  chain should be ended by an operator, not by the agent running it.
- The verifiers reviewed code, not behavior. Nothing here has been used
  by a person for a week; `VISION.md` §9 lists what would tell us the
  execution core is wrong, and no test suite can answer any of it.
- The golden cases are still `_status: proposed`. Until you freeze
  them, the system's ground truth is agent-authored — which is exactly
  the condition the Externality Principle exists to end.
