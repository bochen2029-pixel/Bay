# VISION.md — Bay at the limit

> v0.1 draft — 2026-07-26. **Status: BRAINSTORM, NOT DOCTRINE.** Written
> on operator request ("think from first principles: what could Bay
> become at its best?"). Nothing in this file changes scope, amends the
> "Cut from v1" list, or authorizes implementation. Every idea here is a
> *proposal*; the promotion path is the normal archive-and-diff doctrine
> pass (CLAUDE.md / SPEC.md amendment with operator sign-off). Written
> against CLAUDE.md v1.8 / SPEC.md v1.7, at the I-20 pause boundary
> (cargo 152/152, vitest 93/93 green, verified cold this session).

---

## 0. The question, and the answer in one page

**Question.** If Bay were structured, architected, and implemented in
the most optimal way possible — at every layer — to help one human do
what they intend to do, against procrastination, activation energy, and
drift: what would it be?

**Answer, compressed.** Bay v0.2 is a *prioritization-audit instrument*.
It bounds what you may claim you'll do (caps), makes changing those
claims leave a trace (asymmetric friction, reasons), and records every
management action forever (the log). This is correct and rare — keep
all of it. But it is half a product, because it closes only half the
loop:

- **Bay sees your promises, never your behavior.** The event log records
  *management* (create, move, block, done) and nothing between "moved to
  A" and "done." The most important questions — *what did I actually
  work on? what am I avoiding?* — are structurally unanswerable.
- **Bay bounds stock, not flow.** A≤5 bounds the commitment inventory,
  but nothing bounds *today*. Every glance at the board re-opens the
  "which one now?" decision — and re-deciding is exactly where
  procrastination lives.
- **Bay has no verb for starting.** The single moment of highest
  activation energy — beginning an aversive task — has zero affordance.
  There is a button for *done* and none for *begin*.
- **Bay's mirror is LLM-gated.** The deterministic facts (throughput,
  lead time, leak rates, block durations) require no model, yet today
  all reflection routes through "Analyze."

**The remake is not a rewrite.** The kernel — append-only log, pure
projection, atomic multi-event transactions, type-level LLM firewall,
caps on active items — is exactly the right foundation and becomes
*more* load-bearing, not less. The remake is:

1. **One schema deepening** (the event envelope: txn_id, actor,
   device_id, schema_ver, hash chain) — the only overhaul, and it's
   already half-designed (QUESTIONS Q01 / FUTURE_WORK).
2. **One new organ** (the execution loop: Today/Now flow caps, focus
   sessions, day boundaries) — the missing half of the product.
3. **A sharper mirror** (deterministic stats first, LLM narration
   optional on top) plus **rituals** that give the log human-scale
   boundaries (days, weeks) without ever pinging the user.

Everything else — capture ubiquity, integrity, sync-as-replication —
is infrastructure serving those three.

---

## 1. First principles, re-derived

Design for the human system as it actually is, not as productivity
culture pretends it is. Seven constraints, each with a design
consequence:

1. **Working memory leaks.** Open loops intrude on attention until
   externalized (the Zeigarnik effect). → Capture must be ubiquitous,
   instant, and lossless. *(Bay v0.2: strong — hotkey + LAN page.)*
2. **Deciding is expensive; re-deciding is procrastination fuel.**
   Choice overload and decision fatigue are the observed failure mode of
   every long todo list. → Decide *rarely, at boundaries* (morning,
   weekly), then execute against the decision. Never make the user
   re-choose per glance. *(Bay v0.2: caps reduce the option set but the
   daily re-decision remains unbounded.)*
3. **Value discounts hyperbolically.** Distant, vague tasks lose to the
   phone every time (present bias; Steel's temporal motivation theory:
   motivation ≈ expectancy × value / (impulsiveness × delay)). → Shrink
   *delay-to-reward* by shrinking the unit of work: the next physical
   action, not the project. Raise *expectancy* with evidence of past
   completion. *(Bay v0.2: nothing addresses this.)*
4. **Starting is the energy peak.** Activation energy, not capacity, is
   where intention dies. Implementation intentions ("when X, I will Y" —
   Gollwitzer) reliably lower it; so does pre-deciding the first move.
   → The app must *subsidize the start*: a one-keystroke "begin," a
   pre-named first step, tomorrow's first move chosen tonight.
   *(Bay v0.2: no affordance at all.)*
5. **Self-report lies; recorded behavior doesn't.** Planning fallacy on
   the way in, self-serving memory on the way out. → The mirror must be
   built from *recorded behavior* (events, sessions), never from
   self-assessment. *(Bay v0.2: the log exists but records only
   management behavior.)*
6. **Shame demotivates; visible progress motivates** (the progress
   principle — small wins, made visible, are the strongest workday
   motivator). → Feedback confronts but never punishes. Completed work
   must remain visible as *evidence*, not vanish. No red badges, no
   guilt streaks. *(Bay v0.2: done items literally disappear next
   launch; the emotional ledger is all cost, no receipt.)*
7. **Attention is the product being protected.** An attention-management
   tool that interrupts is self-refuting. → Bay never pushes, pings,
   nags, or badges. It speaks only when opened. Visibility is the nudge.
   *(Bay v0.2: already doctrine — elevate it from behavior to law.)*

**What a tool can and cannot do.** It cannot supply motivation, meaning,
or discipline. It can: reduce friction at the start, reduce re-decision,
make consequences visible, make progress visible, and protect attention.
The remade Bay does all five and refuses everything else. Every item on
the "Cut from v1" list is something else.

---

## 2. The three loops — the core reframe

A complete self-management instrument closes three loops around one
axle (the event log):

```
        ┌──────────────── LOOP 1: CAPTURE → TRIAGE ────────────────┐
        │   thought → Inbox → deliberate tiering (A/B/C)           │
        │   v0.2: STRONG (hotkey, LAN, caps, swap modals)          │
        └───────────────────────────┬───────────────────────────────┘
                                    ▼
        ┌──────────────── LOOP 2: COMMIT → EXECUTE ────────────────┐
        │   A-tier → Today (≤3) → Now (1) → focus session → done   │
        │   v0.2: ABSENT — the missing organ                       │
        └───────────────────────────┬───────────────────────────────┘
                                    ▼
        ┌──────────────── LOOP 3: OBSERVE → ADJUST ────────────────┐
        │   log → deterministic Mirror → rituals → LLM coach       │
        │   v0.2: HALF (LLM-gated Analyze; no session data to see) │
        └───────────────────────────┬───────────────────────────────┘
                                    │
                    ═══ the event log is the axle ═══
```

Loop 2 is where procrastination is fought or lost, and Bay currently
doesn't have it. Loop 3 can only be as honest as the data Loop 2
generates — an execution-blind log can audit your *promises* but not
your *behavior*. This is why the remake's center of gravity is the
execution loop, and why sessions are an event-log feature, not a UI
feature.

A queueing-theory footnote that makes the caps rigorous rather than
aesthetic: by Little's law, lead time = WIP / throughput. Bay already
caps WIP at the commitment level (A≤5). Capping WIP at the *execution*
level (Today≤3, Now=1) is the same law applied where it actually binds.
The Mirror can then display real flow metrics — arrival rate, WIP, lead
time, cumulative flow — straight from the log, which happens to be a
perfect flow-metrics dataset already.

---

## 3. The remade architecture, layer by layer

### 3.0 Kernel — keep everything, deepen the envelope

The four load-bearing invariants (events append-only; projection pure;
multi-event ops atomic; caps active-only) and their three enforcement
layers (handlers, property tests, DB triggers/CHECKs) are the best part
of the codebase. Untouched. One deepening:

**Event envelope v2** (migration 003 — the *only* schema-level overhaul
in this entire vision):

| new column | type | why |
|---|---|---|
| `txn_id` | TEXT (uuid per `write_events` call) | exact undo grouping — resolves QUESTIONS Q01; unblocks I-21; already fully designed in FUTURE_WORK.md |
| `actor` | TEXT: `human` \| `system` | provenance: which writes came from the person vs. a human-configured timer (see §3.4); LLM is *not* an actor — it has no write path to be an actor of |
| `origin` | TEXT nullable | finer provenance: `hotkey`, `lan`, `palette`, `llm_accept:<suggestion_event_id>`, `undo:<txn_id>`, `ritual:day_open` … makes "how do I actually use this app" queryable |
| `device_id` | TEXT | prepares sync-as-replication (§3.8) and multi-device audit; meaningless-but-harmless single-device |
| `schema_ver` | INTEGER | per-event payload version → payload evolution with upcasters; a forever-log needs this before its second decade, cheapest now |
| `prev_hash` | TEXT | hash chain over (prev_hash, ts, type, item_id, payload) → the log becomes tamper-*evident*, completing "perfect self-auditability" into a cryptographic property |

All nullable/defaulted; legacy rows stay valid; `ALTER TABLE ADD COLUMN`
doesn't fire the migration-002 row triggers. One migration, six columns,
five capabilities (precise undo, provenance, sync-readiness, payload
evolution, tamper evidence).

Kernel-adjacent hardening, none of it schema-breaking:

- **Projection snapshots**: checkpoint the projection every N=10k events
  → time-travel and rebuild become O(delta from nearest snapshot).
  Snapshots are cache, never truth; deletable at will.
- **Verify-on-boot**: rebuild-hash vs. projection-hash + chain spot-check
  at startup (at solo scale this is milliseconds). Surfaced as a quiet
  "log verified ✓ N events" line in Settings — trust made visible.
- **The golden runner** (standing correctness debt, pre-vision): 
  `check-golden.py` checks *existence*, not *execution* — the exact gap
  the P2e JOINT_WRONG slipped through. Execute the cases in CI. This
  outranks every feature in this file.

### 3.1 Domain — two orthogonal small additions, no new tiers

Tier (commitment horizon) and state (workability) stay exactly as they
are. Two additions, both projections of new events, both orthogonal to
tier and state:

- **`first_step`** (TEXT ≤140, nullable; `ITEM_FIRST_STEP_SET` event).
  The single next *physical* action: "open contract.pdf," "dial Marco."
  This is the activation-energy handle — deliberately **one line, not
  subtasks** (subtasks stay cut; a checklist is a place to hide from
  work, a first step is a place to start it). Surfaced big in focus
  mode, small on the strip. The LLM may *propose* one (§3.6); triage
  UI *invites* one for A-items ("What's the first physical move?" —
  skippable, never required).
- **Today membership** (`today_on` DATE nullable in projection;
  `TODAY_ADDED` / `TODAY_REMOVED {cause: done|expired|user}` events).
  Today is an *execution overlay*, not a fifth tier — items keep their
  tier; Today marks "chosen for this day." Cap **3**. Membership expires
  nightly (§3.4). Choosing Today each morning is the one deliberate
  decision that buys a re-decision-free day.

Plus the two already-sanctioned v2 items, unchanged from FUTURE_WORK:
**recurrence** (I-21 exactly as specced — RRULE subset, spawn-on-done,
overflow→Inbox, needs txn_id) and **blocked_until** (a wake date carried
in the `ITEM_STATE_CHANGED` payload: snooze honestly instead of letting
"blocked" mean "hidden forever"; wakes *surface* at day-open for
one-click batch unblock — advisory, human-confirmed, per principle 2).

### 3.2 The execution loop — the new organ

**Sessions.** `SESSION_STARTED {item_id}` / `SESSION_ENDED {item_id,
outcome, note?}` events; a `sessions` projection table rebuildable from
the log under the same purity law as `items`. Outcome taxonomy, one tap:
`done` (→ also emits the state change, same txn) · `progress` ·
`interrupted {reason: meeting|person|self_switch|blocked|energy}` —
that reason field quietly builds the user's personal interruption
taxonomy, which no time-tracker ever gets honestly because they all
require configuration up front. Invariant: **at most one open session**
(that open session *is* the "Now" slot — no new state column needed).

**Starting is the cheapest verb in the app.** Enter on a Today strip,
`⌘K start`, or the HUD — one action, and the item goes fullscreen:
content, first step in large type, elapsed time, and nothing else. An
optional tiny always-on-top **HUD pill** (item + elapsed + first step)
keeps the commitment visually present while working in other apps —
presence without interruption, the only legitimate way for Bay to exist
outside its own window.

**The 2-minute lane.** Inbox triage gains a one-item-at-a-time keyboard
flow (A/B/C/delete/do-now per item — triage as a fast ritual, not board
gardening). `do-now` starts an immediate micro-session for sub-2-minute
items; done items of this kind never pollute a tier.

**What sessions buy the rest of the system.** Real durations (so the
Mirror can say "items like this have taken you 2–4 sessions" — measured,
never estimated; estimate theater stays out of Bay). Real avoidance data
(A-items with zero sessions in 14 days is *the* procrastination metric,
and it is currently invisible). Real focus-break patterns. Loop 3
finally gets behavior to reflect, not just promises.

### 3.3 Time made real

- **Horizon semantics, named.** A = active commitments (order weeks);
  B = staged next; C = someday; Inbox = untriaged; Today = this day.
  Doctrine wording only — but it sharpens staleness defaults, gives the
  coach a vocabulary, and makes tier moves mean something crisp
  ("demote" = "push the horizon out," and the reason string records why).
- **Read-only calendar overlay** (optional, off by default): point Bay
  at an ICS URL/file; day-open shows real free hours next to Today's
  plan. No write-back, no OAuth, no accounts — same dependency class as
  the optional LLM endpoint (a URL the user owns). *Tension with the
  no-network principle honestly ledgered in §7.* The point: the board
  can currently be arithmetically impossible without the app noticing;
  one glance at "Today: 3 items, calendar: 90 free minutes" is the
  cheapest feasibility check that exists.
- **No manual time estimates, ever.** The planning fallacy is not a user
  error to be fixed with more user input. Sessions give empirical
  durations; the Mirror reports them; that's Bay's answer.

### 3.4 Rituals as events — boundaries, not notifications

Days and weeks are the human units of intention; the log currently has
no boundaries at all. Three tiny ceremonies, all opt-in, all surfaced
*only when the user opens the app* (law 8: Bay never pings):

- **Day open** (`DAY_OPENED {today_ids}`): pick Today (≤3) from A (B
  allowed, logged); see yesterday's "tomorrow's first move" answer as
  the suggested first session; see any blocked items whose wake date
  passed (one-click batch unblock, human-confirmed).
- **Day close** (`DAY_CLOSED {tomorrow_first?, note?}`): exactly one
  question — "tomorrow's first move?" — because pre-deciding tonight is
  the cheapest known defeat of tomorrow-morning activation energy
  (implementation intention, event-native). Optional one-line note.
- **Day roll** (`DAY_ROLLED {expired_ids}`, `actor: system`): at the
  user-configured day boundary, Today membership expires. Items return
  to their tier — **no rollover, no guilt banner** — but the expiry is
  *logged*, and the Mirror will show the planned-vs-started-vs-finished
  delta, which is confrontation enough. This is the single sanctioned
  `system`-actor write: the deterministic execution of a timer the human
  configured, touching only Today membership — never tier, state, or
  content. (New doctrine line required; §7.)
- **Weekly review** (`REVIEW_COMPLETED {…}`): a guided sweep — inbox to
  zero-ish, stale A/B confronted item-by-item (promote/demote/delete:
  the honest options), wake-date audit, and quarterly **C-bankruptcy**
  ("82 C items untouched >90d — archive them?" one accept, batch
  soft-delete, reason `bankruptcy`, log preserved). Institutionalized
  fresh starts, because the fresh-start effect is real and a system that
  only accretes eventually gets abandoned in shame — the log makes reset
  *cheap* (nothing is ever lost) so bankruptcy is honest, not amnesiac.

### 3.5 The Mirror — deterministic feedback first, LLM optional

A native stats view computed by SQL over the log — **zero LLM
required** (the current Analyze-only reflection inverts the right
dependency order: facts should be free; interpretation is the add-on):

- Cumulative flow per tier; throughput and lead time (Little's law,
  live, from your own data); age distributions.
- **A-leak rate** (items A→C within 48h of promotion — "A is being used
  as an inbox," now a number with a trend).
- Block map: reasons clustered by frequency and duration ("waiting on
  Marco" = 40% of blocked-days is an organizational fact worth knowing).
- **Avoidance list**: oldest A-items with zero sessions — the
  procrastination report, stated flatly, never in red.
- Session honesty: Today planned vs. started vs. finished, per week;
  interruption taxonomy breakdown.
- **Completion receipts**: on done, the inspector shows the item's whole
  journey (created → tiers → sessions → done, with real dates and
  durations); a weekly "what actually happened" page keeps finished work
  visible as evidence (progress principle) instead of vanishing at next
  launch.
- **Bay audits Bay**: feature-usage queries from the log itself
  (`origin` column). If sessions or rituals go unused, the Mirror says
  so — every mechanism in this vision must earn its place in the
  operator's actual event stream or be removed (§9).

### 3.6 The coach — LLM with more to see, still zero authority

The firewall is unchanged and remains absolute: **the LLM never writes;
`ProjectionEvent` still has no LLM variants; every proposal flows
through GENERATED → human accept/reject → deterministic atomic
cap-enforced apply** (the I-20 path, which is exactly the right shape —
extend it, never bypass it). What changes is what the LLM can see and
propose:

- **Sees**: compressed aggregates now including session/ritual data —
  the coach can finally reference behavior ("zero sessions on X since
  you called it critical 3 weeks ago") instead of only board topology.
- **Proposes** (all accept/reject diffs through the one path): re-orgs
  (shipped, I-20); **first-step decompositions** ("first step: email
  Sarah for the Q3 numbers?"); wake/snooze suggestions; and — flagged
  as the sharpest doctrine edge in this file (§7) — a **Today draft** at
  day-open. Capture-time tier suggestions remain banned (that's where
  auto-tiering sneaks in and kills the user's judgment); a day-open
  Today draft is a different surface (pull not push, planning not
  triage, over items the human already tiered) but it is *adjacent* to
  the ban and ships only with an explicit doctrine line, default off.
- **Weekly-review copilot**: narrates the Mirror and asks the
  confronting questions; writes nothing.
- Streaming (I-22) as specced; local-model-first posture unchanged.

### 3.7 Capture at the limit

- **Idempotency key** on every capture (`capture_uuid` in the payload,
  deduped at the command layer) — the prerequisite for every retrying
  surface below; also closes the double-tap-on-flaky-wifi dupe today.
- **PWA companion** (I-27 grown up): the LAN page becomes an installable
  PWA with an **offline outbox** — capture on the subway, sync when home
  (capture must *never* be lost; unsynced state visibly queued). Plus
  read-only Today/board and mark-done. Still LAN-trust, still no cloud.
- Palette-native capture (`⌘K` from anywhere in-app), OS share-target
  where Tauri permits, clipboard capture command.
- Voice stays the phone's native dictation into the PWA (posture
  unchanged); a local-whisper path is a someday-option, never a cloud
  STT dependency. No email-in (requires a server in the world; cut).

### 3.8 Continuity — the data outlives the app

- **Auto-backup**: rolling JSONL export + DB snapshot on close to a
  user-chosen folder. The export format documented as a public contract
  (`docs/FORMAT.md`): SQLite + JSONL + this repo's schemas = readable in
  30 years without Bay.
- **Hash chain + verify** (§3.0) makes the backup *checkable*, not just
  present.
- **Sync, if it ever ships (Phase 6, operator-gated)**, is
  **replication of the user's own log between the user's own devices**
  — never a service, never an account. Mechanics the envelope already
  prepares: `(device_id, per-device seq)` identity; merge = set-union
  with deterministic total order (ts, device_id, seq); projection
  rebuild resolves everything mechanical; the one non-mechanical case —
  a merge that overflows a cap because two devices promoted
  independently — surfaces as a human-resolved swap queue (the swap
  modal, replayed), never a silent auto-demotion. Transport = user's
  folder / WebDAV / self-hosted relay. This re-litigates "no cloud sync"
  honestly: the *principle* was never "one device forever," it was "no
  accounts, no servers we run, no data leaving the user's control" —
  log replication satisfies all three.

---

## 4. The ten laws (invariants of the remade Bay)

The six principles, sharpened and extended to ten. 1–3, 5, 7 are
today's doctrine restated; 4, 6, 8–10 are new or elevated.

1. **Append-only forever.** Every state is a pure function of the event
   log; nothing updates or deletes history; the log is hash-chained and
   verifiable. (Today: enforced by triggers + property tests. Add:
   chain, verify-on-boot.)
2. **One write path.** Every mutation is one atomic `write_events`
   transaction bearing one `txn_id`; undo is compensating events grouped
   by `txn_id`, never history rewrite.
3. **Caps bind stock.** A ≤ 5 and B ≤ 12 active items, swap-or-reject,
   enforced across every entry path (create, move, restore, batch,
   accept-reorg, recur-spawn). *The caps are the product.*
4. **Caps bind flow.** Today ≤ 3, chosen by the human at day-open;
   at most one open session ("Now"). Decide at boundaries; execute
   between them.
5. **Machines propose; the human disposes; the deterministic tier
   writes.** No LLM write path, structurally (`ProjectionEvent`). No
   auto-tiering, no smart sort, ever. Every suggestion and its
   accept/reject lives in the log.
6. **The system may act alone only to execute a timer the human set** —
   Today expiry, wake surfacing — always as `actor: system`, always
   visible in the log, never touching tier, state-except-expiry,
   or content.
7. **Capture is never lost and never blocked** (instant, offline-safe,
   idempotent, unbounded Inbox); **triage is always deliberate** (a
   human hand moves every item out of Inbox).
8. **Friction is engineered, asymmetrically.** Starting work is the
   cheapest verb in the app. Breaking a commitment (cross-tier demotion,
   abandoning a session) is always possible but always leaves a reason
   in the log. **And Bay never interrupts** — no push, no badges, no
   nags; it speaks only when opened; visibility is the nudge.
9. **The mirror is deterministic first and honest always.** Facts
   (throughput, leaks, avoidance) render from SQL with no model
   configured; the LLM narrates on top, on demand. Feedback confronts;
   it never shames; progress stays visible as evidence.
10. **The data outlives the app.** Local-first forever; no accounts, no
    telemetry; documented export format; sync (if ever) is replication
    of the user's own log between the user's own devices.

---

## 5. Event taxonomy v2 (delta)

Envelope on **all** events: `{id, ts, txn_id, actor, origin, device_id,
schema_ver, type, item_id, payload, prev_hash}`.

| new type | payload sketch | projection effect |
|---|---|---|
| `ITEM_FIRST_STEP_SET` | `{before, after}` | `items.first_step` |
| `TODAY_ADDED` | `{}` | `items.today_on = date` |
| `TODAY_REMOVED` | `{cause: done\|expired\|user}` | `items.today_on = null` |
| `SESSION_STARTED` | `{}` (item_id on envelope) | open row in `sessions` |
| `SESSION_ENDED` | `{outcome, reason?, note?}` | close row; `done` outcome co-writes `ITEM_STATE_CHANGED` in the same txn |
| `DAY_OPENED` | `{today_ids[]}` | none (ritual/audit) |
| `DAY_CLOSED` | `{tomorrow_first?, note?}` | none (ritual/audit) |
| `DAY_ROLLED` | `{expired_ids[]}` — `actor: system` | clears expired `today_on` |
| `REVIEW_COMPLETED` | `{counts…}` | none (ritual/audit) |
| `ITEM_RECURRENCE_SET` / `ITEM_RECURRED` | per FUTURE_WORK I-21 | `items.recurrence` / none |
| `ITEM_STATE_CHANGED` (extended) | `+ blocked_until?` | wake date on blocked items |

Non-projection types (`DAY_*`, `REVIEW_*`, `ITEM_RECURRED`) return
`None` from `to_projection_event()` — the firewall boundary generalizes
cleanly from "LLM events don't project" to "advisory/audit events don't
project," with the LLM variants still structurally absent from
`ProjectionEvent`.

New property tests, same non-LLM-oracle discipline: Today cap under any
op interleaving; single-open-session under any interleaving; day-roll
idempotence; session/projection rebuild determinism (the sessions table
joins the THE-property); chain verification over arbitrary logs.

---

## 6. What stays out — the sharpened refusals

The cut list is a load-bearing feature. Reaffirmed, with additions the
new surfaces make newly tempting:

- **No gamification.** No points, streaks, levels, confetti economies.
  Streaks convert a bad Tuesday into cascading shame; Bay's currency is
  evidence, not dopamine.
- **No notifications, ever, elevated to law** (8). Not even "gentle"
  ones. An attention guardian that pings is self-refuting.
- **No auto-planning.** The machine never fills Today, never orders your
  day unbidden, never schedules. Drafts on request, behind a doctrine
  amendment, default off — decisions never.
- **No estimates.** Measured durations only (§3.3).
- **Still no tags/labels/categories.** Search + four tiers + Today +
  the log's own history answer every retrieval need one human has;
  taxonomizing a todo list is procrastination with extra steps.
- **Still no subtasks/checklists.** `first_step` is one line by design —
  a doorknob, not a corridor. (If an item needs a checklist it's a
  project; the honest move is decomposing it into items.)
- **Still no second prioritization scheme** (Eisenhower, scores,
  urgency×importance): one axis, human-ranked, capacity-bounded.
- **No accounts, no cloud services, no telemetry** — unchanged,
  unchangeable.
- **No engagement mechanics of any kind.** Bay succeeds when it is
  *left* quickly, holding trust in the interim.

---

## 7. Tension ledger — where this vision touches doctrine

Stated plainly so the doctrine pass can dispose of each explicitly:

| # | proposal | tension | disposition needed |
|---|---|---|---|
| T1 | envelope migration 003 | schema change to the append-only core — the exact class the charter gates ("new event-log field → bank clean, ask") | operator sign-off; Q01 already queues it; supersedes/absorbs the txn_id-only migration |
| T2 | `DAY_ROLLED` system-actor write | today, every write is human-initiated; this introduces a machine write (of a human-set timer, Today-membership only) | new doctrine line (law 6); alternative: expiry-on-next-open (no system actor, slightly staler board) if the operator prefers zero machine writes |
| T3 | Today/Now flow caps | new interaction surface; arguably "a second scheme"? — it isn't a *prioritization* scheme (no new axis; a day-scoped WIP limit over the existing axis) but the distinction deserves a written line | doctrine amendment naming Today an execution overlay, cap 3 |
| T4 | LLM Today-draft at day-open | adjacent to the banned "capture-time tier suggestions" | explicit line-drawing amendment; default off; or drop — the coach works without it |
| T5 | ICS read-only overlay | "no network dependency beyond the optional LLM endpoint" | amend to "…beyond user-owned optional endpoints (LLM, calendar ICS)" or defer; nothing else depends on it |
| T6 | sessions/rituals/Mirror | none — pure additions inside existing principles | normal SPEC increments |
| T7 | recurrence (I-21) | already sanctioned v2, blocked on T1 | ship after 003 |
| T8 | sync as log replication | re-litigates "no cloud sync" | Phase 6, operator-gated as already planned (I-23); the envelope merely stops the door from rusting shut |
| T9 | bankruptcy batch-archive | none (soft-delete + reason; log preserved) | normal increment |

---

## 8. Phasing — against the existing increment rhythm

Every increment ships demoable and green, doctrine co-passes per
archive-and-diff, critical-module work gets the two-pass + non-LLM
oracle treatment. Rough shape:

- **v0.3 "Execution"** — the golden runner (first, it's correctness
  debt); migration 003 envelope + undo-by-txn_id (closes Q01); I-21
  recurring (unblocked); `first_step`; sessions + focus mode + HUD;
  Today/Now + day events + day-roll; Mirror v1 (flow metrics, avoidance,
  receipts).
- **v0.4 "Time & ritual"** — blocked_until + wake surfacing; day
  open/close ceremonies; weekly review mode + C-bankruptcy; triage
  one-at-a-time flow + 2-minute lane; aging patina on strips
  (continuous age visibility, not just the binary ⚠); coach v2 over
  session data; I-22 streaming.
- **v0.5 "Continuity"** — hash chain + verify-on-boot; auto-backup +
  documented export format; capture idempotency + PWA companion with
  offline outbox; projection snapshots; ICS overlay (iff T5 amended).
- **v0.6 "Replication"** — sync via log replication (I-23 decomposed,
  operator-gated); multi-profile (I-24).

Priority if only five things ever ship, in order:
**(1)** envelope 003, **(2)** sessions + focus mode, **(3)** Today/Now,
**(4)** Mirror v1, **(5)** day close/open with "tomorrow's first move."
(The golden runner rides above all five as debt, not feature.)

---

## 9. What would falsify this vision

Calibration hooks, so the remake is subject to the same discipline as
the code — each mechanism carries its own Bay-audits-Bay query, reviewed
monthly (/calibrate) after four weeks of dogfooding:

- **Sessions unused** (< 1 session/workday median) → the execution loop
  as built doesn't fit the operator; strip the HUD/focus chrome, keep
  the event types (cheap), rethink the affordance.
- **Today ignored or expiry resented** → downgrade DAY_ROLLED to
  advisory-on-open (T2 alternative); if Today itself is dead weight,
  remove — law 4 falls, laws 1–3 stand.
- **Mirror unopened** → fold its top three numbers into day-open;
  reflection that requires a pilgrimage doesn't happen.
- **"Tomorrow's first move" skipped nightly** → the ritual is friction
  theater for this user; cut it before it breeds resentment.
- **First_step left empty on >80% of A items** → the field isn't
  earning its pixels; demote to focus-mode-only prompt.

The failure mode this section guards against: a vision document turning
into scope gravity. Nothing here is sacred; the log will say what
worked; the caps and the firewall are the only hills to die on — they
are the product.

---

*End of VISION.md v0.1 draft. Companion reading order: CLAUDE.md
(doctrine) → SPEC.md (implementation) → FUTURE_WORK.md (near-term
scope) → this file (horizon). Promotion of any section = normal
doctrine pass with operator sign-off; until then this file has no
authority over scope.*
