# QUESTIONS.md — Bay v0.2.0 revamp run

> Decisions made under uncertainty, with reasonable default applied,
> flagged for operator review on return. Append-only. Each entry has
> a lifecycle: OPEN → (CONFIRMED | UNWOUND | ABANDONED).

## Q01 — undo action grouping: (ts, type) heuristic vs. a transaction id

**Status:** CONFIRMED → resolved 2026-07-26. The operator authorized the
schema change via the 2026-07-26 directive (DECISIONS ADR-007);
migration 003 added `events.txn_id` (one uuid per `write_events`
transaction) and `undo_last_action` now groups by `txn_id` exactly —
the `(ts, type)` heuristic survives only as the fallback for legacy
pre-envelope rows (`txn_id IS NULL`, pinned so legacy and enveloped
rows can never co-group). Undo additionally skips `actor = 'system'`
transactions (VISION law 6). Mixed-type transactions — the case this
question existed for — now undo as one action (regression-tested;
I-21's recurrence trio builds on this).

**Context.** `undo_last_action` must treat a multi-event atomic operation
(swap = 2 ITEM_MOVED; batch = N ITEM_STATE_CHANGED / ITEM_DELETED) as ONE
undoable action, while a single command undoes exactly one event. The
event log records no transaction boundary, so "which events form one
action" must be inferred.

**Decision (default applied).** Group the most-recent non-LLM events that
share `(ts, type)`. Every atomic op writes its events in one
`write_events` tx with a single shared ts and a single type, so this
groups swaps and batches correctly. Keying on type as well as ts avoids
over-grouping a fast create+edit that share a ms (different types).

**Known limitation.** Two *distinct same-type* commands that land in the
SAME millisecond (e.g. two separate mark-done clicks) would be grouped
into one undo. This is **unreachable in production**: Bay is a single-user
GUI; two same-type commands can't be issued within 1 ms by hand, and the
only same-ms same-type producers are the atomic ops themselves (which ARE
one action). It only manifests in microsecond-fast automated test
sequences (handled in the tests by asserting no-crash rather than exact
revert count).

**The exact fix (operator-gated).** Add a `txn_id` column to `events`
(migration 003), stamp it once per `write_events` call, and group undo by
`txn_id`. This is precise (no heuristic) but is a schema change to the
append-only core table — flagged in AUTONOMY_CHARTER's foundational-blocker
list ("new event-log field → bank clean, ask"). Deferred to operator
review rather than self-authorized, since the heuristic is production-correct
and the schema change is the kind the charter asks me not to make casually.

**What would change it:** if a programmatic/scripted write path is ever
added (breaking the single-user-GUI assumption), or batch sizes/automation
make sub-ms same-type actions reachable, switch to `txn_id`.

---

## Question lifecycle (v7)

Every speculation resolves to one of:
- **CONFIRMED** → promote to DECISIONS.md ADR; remove inline
  `SPECULATION:` tag, add DECISION ref.
- **UNWOUND** → `git revert` the choice; note why here.
- **ABANDONED** → delete the code path; close this entry.

The continuous verifier flags speculations older than 5 modules and
assigns a target lifecycle state.

## Q02 — intra-tier placement inside an accepted LLM diff: ops order or board order?

**Status:** OPEN — reasonable default applied, flagged for operator
review. Raised 2026-07-27 while closing the mixed-op permutation gap
pass 6 flagged.

**Context.** SPEC §8.7 requires an accepted diff to be a function of
the op SET, not its order — the whole point being that the LLM must
not gain a lever on the deterministic tier by choosing how it lists
proposals. Seven cold passes hardened the *contests* accordingly: who
takes the last A slot, who keeps the last Today slot. Both are now
decided by `board_order` keyed on the PRE-DIFF board.

One thing is still decided by the array: **intra-tier placement.**
`next_rank` hands out end-of-tier ranks as the ops vector is walked, so
accepting `[move x -> A, move y -> A]` leaves x above y, and the
reversed listing leaves y above x. Same op set, two different boards.

**Decision (default applied).** Left as is, and the new test
`accept_reorg_mixed_move_and_done_commutes_over_all_orderings`
deliberately excludes `rank` from its 24-permutation fingerprint,
asserting only that tier and state commute. The reasoning: the human
accepted "move both of these to A" and expressed no preference between
them, so the model`s listing is the only stated ordering available, and
using it is closer to honouring the diff than inventing an order. It is
also visible and trivially draggable, unlike a lost Today slot.

**Why it is a question anyway.** It is a real order-dependence in a
path whose headline invariant is order-independence. Two defensible
alternatives:
1. **Board-derive it too**: sort the moves by `board_order` on the
   pre-diff board before allocating ranks, so a re-listed diff yields a
   byte-identical board and the invariant needs no caveat.
2. **Keep it, and say so in SPEC §8.7** — narrow the stated invariant
   to "tier, state and scarce-slot contests", so nobody later reads the
   broad claim and builds on it.

**CORRECTION, 2026-07-27 (v0.3 pass 9).** As originally filed, this
entry claimed *"SPEC §8.7 currently claims more than the code
delivers"* and concluded *"doing nothing is the one option that should
not survive review."* **Both were false, and the error was mine.**
SPEC.md §8.7 lines 1566–1571 already narrow the invariant and name this
exact exception —

> The one deliberate exception: the **relative rank of several items
> moved into the same tier in one accept** follows the order they
> appear in the diff … it decides nothing about what commits or what
> is lost.

— and that paragraph landed at `a0f4775` (pass 4), well before this
question was written. Alternative (2) is therefore **already
implemented**. I did not check SPEC before asserting what it said, and
the effect would have been to make the operator decide under an urgency
I invented.

**Operator call, restated honestly.** Only alternative (1) is live, and
it is optional rather than owed: board-deriving the rank would cost one
sort and let §8.7 drop its caveat. **Doing nothing is now a legitimate
outcome** — the documentation and the code already agree.
