---
name: stuck
description: Self-stuck detection. When any of three mechanical signals fires (same error after 3 fix attempts; same file edited 5+ times without forward progress; test failure unchanged across 3 fix attempts), stop attempting, file a BLOCKER with context, mark the task SUSPECT, and apply the fan-out rule.
---

# /stuck

Three mechanical stuck signals. When any fires: stop attempting, file
BLOCKER with context, mark task SUSPECT, apply the fan-out rule.

## The three signals

1. **Same error after 3 different fix attempts.** Pattern repetition
   under variation → root cause is not where you think.
2. **Same file edited 5+ times in same session without forward
   progress** (test status unchanged OR RUN_STATE next-action
   unchanged). Flailing.
3. **Test failure unchanged across 3 fix attempts.** The fix isn't
   changing the symptom; your model of the failure is wrong.

## When stuck

1. **Stop attempting the current approach immediately.**
2. **Write a detailed BLOCKERS.md entry** with:
   - Filed timestamp
   - Blocker (precise, technical)
   - Alternatives tried (so operator doesn't suggest something already
     attempted)
   - What would unblock (specific action needed)
   - Work moved to (what you're doing instead)
3. **Mark the task SUSPECT** in TASKLIST.md.
4. **Apply the fan-out rule.**

## Fan-out rule (v7)

Compute fan-out = transitive count in the blocker's `blocks` graph.

- **Low (< `FOUNDATIONAL_BLOCKER_FANOUT`, default 3):** advance to
  other independent work. Banked work likely survives.
- **High (≥ 3) — foundational:** do NOT build speculative dependents.
  In order:
  (a) one careful diagnostic pass;
  (b) if a *reversible* default exists, take it as a heavily-tagged
      speculation (`// SPECULATION:` + QUESTIONS.md entry);
  (c) else **stop the run early, bank clean, write ESCALATION, end.**

A 2-hour run stopping at the right decision beats an 8-hour run
building six modules on a wrong foundation.

Log `negative_work_avoided` to run-metrics.jsonl when (c) fires.

## The discipline

Do not burn runway on a single sticky problem. A stuck task gets one
careful diagnostic pass, then is deferred. The operator resolves it on
return; the run advances to independent work.
