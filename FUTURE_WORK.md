# FUTURE_WORK — Bay remaining scope

> Rewritten 2026-07-26 at the v0.3 "Execution" boundary. The prior
> edition (2026-06-17) scoped the txn_id decision and I-21; **both
> shipped this run**, so this file now scopes what is actually left.
> `VISION.md` §8 has the longer-horizon sequencing; `VISION.md` §7 lists
> what remains operator-gated. Doctrine: CLAUDE.md v1.9 / SPEC.md v1.8 /
> PROMPTS.md v1.6.

## Status snapshot

| Phase | Increments | State |
|---|---|---|
| P0–P2 | spine, bug-fix, correctness layer | ✅ done |
| P2e | cold-context two-pass verification | ✅ done (2 BLOCKING bugs fixed) |
| P3 | doctrine reconciliation | ✅ done |
| P4 | I-15..I-19 (palette, C-collapse, undo, audit search, batch) | ✅ done |
| P5 | I-20 LLM re-org accept-path | ✅ done |
| P5a | golden RUNNER (cases execute, not just exist) | ✅ done (v0.3) |
| P5b | migration 003 envelope + undo-by-txn_id (Q01 closed) | ✅ done (v0.3) |
| P5 | I-21 recurring tasks | ✅ done (v0.3) |
| P5c | execution core: first_step, Today, rituals, sessions, Mirror | ✅ done (v0.3) |
| P5 | I-22 LLM streaming | ⏳ remaining |
| P6 | I-23..I-27 (sync, multi-profile, theming, plugins, mobile) | ⏳ remaining |
| P7 | release (tag, runbooks) | ⏳ after the above |

Gates at this boundary: cargo **206/206**, vitest **106/106**, both
builds clean, warning-clean, `check-golden.py` + store-logic green,
`PRAGMA user_version` 6.

---

## Immediate (small, unblocked)

### F-01 — Freeze the corrected golden cases *(operator action)*
Executing the golden cases (P5a) surfaced three defective **proposed**
`caps.json` cases (#5, #6, #8) whose expectations contradicted their own
names and frozen doctrine. They are corrected in place with
`_corrected` annotations and remain `_status: proposed`. **The operator
should review and freeze them** — they are the system's ground truth,
and the agent authored the correction. See REVIEW_QUEUE.md item 1.

### F-02 — Cold-context verifier pass over the v0.3 diffs
A verifier was dispatched twice this run; the first died on a substrate
quota limit. Re-run against `de37921..HEAD` with the areas listed in
REVIEW_QUEUE.md: hash-chain correctness, envelope bypass, undo grouping
edge cases, cap joints (recurrence spawn accounting × batch × Today),
projection purity across both projection tables, and the Mirror's
aggregate definitions.

### F-06 — Dogfood v0.3 against its own kill conditions *(the next step)*
See **`DOGFOOD.md`** — every `VISION.md` §9 falsification criterion
turned into a query you can run, with the threshold and the prescribed
consequence stated in advance so the verdict cannot be rationalised
afterwards. Writing it surfaced one problem: **§9's "Mirror unopened"
criterion is unmeasurable**, because opening a view emits no event and
Bay has no telemetry by design. The recommendation there is to fold the
Mirror's top three numbers into day-open regardless, rather than add a
`VIEW_OPENED` event — that would be the first event in the log
recording *attention* rather than *action*, and it is a door worth
leaving shut.

This is the highest-value remaining work, and no amount of further
verification substitutes for it: ten cold passes established that the
code does what SPEC says, and not one of them could establish that SPEC
is a good idea.

### F-05 — Extract the blocked-reason carry into one helper
**Deliberately deferred, not forgotten.** The rule "carry the outgoing
`blocked_reason` whenever `blocked` is on either side of the
transition" is written out inline at **four** doors:
`items::set_item_state_inner`, `items::batch_set_state_inner`,
`session::end_session_inner` (the Done co-write), and *two* arms of
`llm::apply_reorg_inner`. Every one of them is currently correct.

The contrast worth noticing is with the Today re-entry cap, which is
the *same shape of rule* and has never drifted — because it lives in
one shared helper (`day::today_overflow_draft`) that three doors call.
One mutation on that helper breaks all three call sites at once.
Duplicated logic needs a guard per door; shared logic needs one. The
blocked-reason rule is the duplicated kind, which is exactly why v0.3
pass 4 found it broken at one door and pass 7 found it *unguarded* at
another.

Extracting `outgoing_blocked_reason(current, target) -> Option<String>`
would collapse four copies into one and make the drift structurally
impossible. Not done during the verification chain on purpose: this run
has spent seven passes learning that changes made late, while
confidence is high, are where defects hide — and a refactor of a core
write path is precisely that. Do it at the start of a run, not the end,
and keep the four per-door tests afterwards (they catch a door that
stops calling the helper, which a mutation on the helper cannot).

### F-03 — I-22 LLM streaming
`OpenAiCompatClient.chat` does a single request. Add a streaming
variant (`stream: true`, SSE) and an `analyze_progress` `streaming`
stage so observations render as they arrive. Keep the parse path
strict: buffer, then parse the final JSON. No doctrine impact.

### ~~F-04 — Coach v2 (session-aware analysis)~~ — **done 2026-07-27**
`compress` now carries sessions, outcomes, the interruption taxonomy,
Today honesty, and `never_started` (A/B actives with zero sessions
ever, un-windowed); the prompt renders them and instructs the model to
ground observations in behavior, treat zero sessions as avoidance with
a cause, and report rather than exhort. Token budget test included.
Firewall unchanged — more context, not more authority.

---

## VISION v0.4 "Time & ritual" (designed, not built)

- **`blocked_until` wake dates.** A date carried in the
  `ITEM_STATE_CHANGED` payload so "blocked" can mean "snoozed until
  Tuesday" instead of "hidden forever". Wakes *surface* at day-open for
  one-click batch unblock — advisory, human-confirmed (law 6 forbids
  auto-unblocking).
- **Weekly review mode.** A guided sweep: inbox toward zero, stale A/B
  confronted item-by-item, wake-date audit. Emits `REVIEW_COMPLETED`.
- **C-bankruptcy.** "82 C items untouched >90d — archive them?" One
  accept → batch soft-delete with reason `bankruptcy`. The log keeps
  everything, which is exactly what makes the reset honest rather than
  amnesiac.
- **One-at-a-time triage + the 2-minute lane.** Keyboard flow over the
  Inbox (A/B/C/delete/do-now per item); `do-now` opens an immediate
  micro-session.
- **Aging patina.** Continuous age shading on strips, replacing the
  binary ⚠ threshold.

## VISION v0.5 "Continuity"

- **Auto-backup**: rolling JSONL export + DB snapshot on close to a
  user-chosen folder; the hash chain makes the backup *checkable*.
- **Documented export format** (`docs/FORMAT.md`) as a public contract:
  SQLite + JSONL + the schemas = readable in 30 years without Bay.
- **Capture idempotency** (`capture_uuid`, deduped at the command
  layer) — the prerequisite for any retrying surface, and it also
  closes today's double-tap-on-flaky-wifi duplicate.
- **PWA companion** (I-27 grown up): the LAN page becomes installable
  with an **offline outbox**, plus read-only Today/board and mark-done.
  Still LAN-trust, still no cloud.
- **Projection snapshots** every N events so time-travel and rebuild
  become O(delta). Snapshots are cache, never truth.

## Phase 6 / VISION v0.6 (operator sign-off required)

- **I-23 sync as log replication** (VISION §3.8, tension T8). The
  envelope already carries `device_id`; identity would extend to
  `(device_id, per-device seq)`, merge = set-union with a deterministic
  total order, and the one non-mechanical case (a merge that overflows
  a cap) surfaces as a human-resolved swap queue — never a silent
  auto-demotion. Re-litigates "no cloud sync": the principle was never
  "one device forever", it was "no accounts, no servers we run, no data
  leaving the user's control".
- **I-24 multi-profile** — separate `bay.db` + settings per profile.
  Mostly additive.
- **I-25 theming** — needs an explicit doctrine amendment (the cut list
  names theme customization).
- **I-26 plugin surface** — riskiest; ship an MVP (event subscription +
  palette action) or not at all.
- **T4 LLM Today-draft** / **T5 calendar ICS read** — both adjacent to
  standing prohibitions; each needs its own doctrine line before build.

## P7 — release

Regenerate `marrow.lock`, finalize `run-metrics.jsonl`, write runbooks
(verify projection, verify chain, disable v0.3 surfaces), bump README,
tag. Consider dogfooding through one full week first and running the
`VISION.md` §9 falsification checks — the execution core is a
hypothesis with stated kill conditions, and shipping it as permanent
before testing those would be exactly the scope gravity §9 guards
against.
