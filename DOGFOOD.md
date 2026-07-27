# DOGFOOD.md — evaluating v0.3 against its own kill conditions

> Written 2026-07-27, at the end of the v0.3 run and ten cold
> verification passes. Not doctrine. `VISION.md` §9 states what would
> falsify each v0.3 mechanism; this file turns each of those into a
> query you can actually run, so the verdict comes from the log rather
> than from memory.
>
> **Why this exists.** Everything v0.3 shipped is a hypothesis about
> how a person works. Ten passes established that the code does what
> the spec says; not one of them could establish that the spec is a
> good idea. The execution core (`first_step`, Today, sessions, the
> rituals) was built on the claim that activation energy — not capacity
> — is where intention dies. That claim is testable, and until it is
> tested the honest description of v0.3 is *unvalidated*.

## How to use this

Run Bay normally for **four weeks** (VISION §9's stated window; two is
enough for a smell test). Then run the queries below against the live
database and compare with the thresholds. Each has a stated
consequence — the point of writing them down in advance is that the
verdict cannot be rationalised afterwards.

Database path (Windows):

```bash
sqlite3 "$APPDATA/com.bay.desktop/bay.db"
```

Everything below is read-only. Nothing here writes to the log.

Every query here was executed against a database built fresh from
`migrations/` and seeded with rows in each table it touches — a
document full of untested SQL is a document full of untested claims,
and this run has spent ten passes learning what those are worth.

---

## 1. Sessions unused → the execution loop doesn't fit

**VISION §9:** *< 1 session/workday median → strip the HUD/focus
chrome, keep the event types (cheap), rethink the affordance.*

```sql
-- Sessions per day, most recent first.
SELECT date(started_at / 1000, 'unixepoch') AS day,
       COUNT(*)                             AS sessions,
       ROUND(SUM(COALESCE(ended_at, started_at) - started_at) / 60000.0) AS minutes
FROM sessions
GROUP BY day
ORDER BY day DESC;
```

**Verdict.** Take the median of `sessions` over days you actually
worked. Below 1 → the focus affordance is not being reached for. The
prescribed response is *not* to add reminders (law 8: Bay never
pushes); it is to make starting cheaper or to remove the chrome.

## 2. Today ignored, or expiry resented

**VISION §9:** *→ downgrade the day-roll to advisory-on-open; if Today
itself is dead weight, remove it — law 4 falls, laws 1–3 stand.*

```sql
-- Chosen vs. expired, per day. `expired` is the day-roll reclaiming a
-- membership you never finished; `user` is you removing it yourself.
SELECT json_extract(payload, '$.date') AS day,
       SUM(type = 'TODAY_ADDED')                                        AS chosen,
       SUM(type = 'TODAY_REMOVED'
           AND json_extract(payload, '$.cause') = 'expired')            AS expired,
       SUM(type = 'TODAY_REMOVED'
           AND json_extract(payload, '$.cause') = 'user')               AS removed_by_you
FROM events
WHERE type IN ('TODAY_ADDED', 'TODAY_REMOVED')
GROUP BY day
ORDER BY day DESC;
```

**Verdict.** `chosen` near zero → Today is dead weight. `expired`
persistently at or near `chosen` → you are planning days you do not
work, which is the failure the cap was supposed to prevent, not one it
can fix. Both are falsifications, with different remedies.

## 3. Mirror unopened → reflection that requires a pilgrimage

**VISION §9:** *→ fold its top three numbers into day-open.*

**This one cannot be answered from the log, and that is a real gap.**
Opening a view emits no event — Bay records *actions*, not navigation,
and there is no telemetry by design (CLAUDE.md architecture). So the
kill condition as written is unmeasurable.

Three honest options, in order of preference:

1. **Self-report.** After four weeks, answer from memory: did you open
   the Mirror more than twice unprompted? Crude, but it is the question
   that matters and you are the only user.
2. **Fold it in anyway.** The prescribed remedy — surfacing the top
   three numbers at day-open — is cheap and is a strict improvement
   whether or not the Mirror is being visited. Doing it removes the
   need to measure.
3. **Do not add a `VIEW_OPENED` event to make this measurable.** It
   would be the first event in the log that records attention rather
   than action, and that is a door worth leaving shut.

## 4. "Tomorrow's first move" skipped nightly → friction theatre

**VISION §9:** *→ cut it before it breeds resentment.*

```sql
SELECT date(ts / 1000, 'unixepoch')                              AS closed_on,
       json_extract(payload, '$.tomorrow_first') IS NOT NULL     AS named_a_first_move
FROM events
WHERE type = 'DAY_CLOSED'
ORDER BY ts DESC;
```

**Verdict.** If most closes name nothing, the evening decision is not
being made and the ritual is costing a prompt for no return. Note the
denominator matters: a day you never closed at all is a *different*
signal — the ceremony itself is not happening — and shows up as a
missing row, not a null.

## 5. `first_step` left empty on >80% of A items → not earning its pixels

**VISION §9:** *→ demote to focus-mode-only prompt.*

```sql
SELECT tier,
       COUNT(*)                                          AS items,
       SUM(first_step IS NULL OR TRIM(first_step) = '')  AS without_first_step
FROM items
WHERE deleted = 0 AND state = 'active' AND tier IN ('A', 'B')
GROUP BY tier;
```

**Verdict.** `without_first_step / items > 0.8` for tier A → the field
is decoration. Worth pairing with query 1: a high empty rate *and*
healthy session counts means starting was never the bottleneck for
you, which would falsify the premise the whole execution core rests on
— the most informative result available here.

---

## The one that matters most, and is not in §9

Whether **the caps still bite**. They are the product; everything else
in v0.3 is scaffolding around them.

```sql
-- Are you living at the cap, under it, or gaming it?
SELECT tier, COUNT(*) AS active
FROM items WHERE deleted = 0 AND state = 'active' AND tier IN ('A','B')
GROUP BY tier;

-- A-leak: items demoted out of A within 48h of entering it. If this is
-- high, A is being used as an inbox and the cap is being routed around
-- rather than respected. (The Mirror computes this for you.)
SELECT COUNT(*) AS blocked_items,
       SUM(blocked_reason IS NULL) AS without_a_reason
FROM items WHERE deleted = 0 AND state = 'blocked';
```

**The failure mode to watch for is `blocked` as an escape hatch.**
Blocked items do not count against the cap — by design, law 4 — so a
growing blocked pile with vague reasons is how a capacity system
quietly stops being one. The Mirror's block-cost table is the place
this shows up first.

---

## After the four weeks

Record the verdicts in `PROGRESS.md`, then act on them *before*
building anything new. VISION §9's closing line is the standard:

> Nothing here is sacred; the log will say what worked; the caps and
> the firewall are the only hills to die on — they are the product.
