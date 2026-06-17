# BLOCKERS.md — Bay v0.2.0 revamp run

> Hard stops requiring operator action to unblock. Each entry: filed
> timestamp, blocker, alternatives tried, what would unblock, work
> moved to. Append-only.

(none yet — run just started)

---

## Blocker fan-out rule (v7)

When a BLOCKER is filed, compute fan-out = transitive count in its
`blocks` graph.

- **Low (< 3):** advance to other independent work. Banked work
  likely survives.
- **High (≥ 3, the `FOUNDATIONAL_BLOCKER_FANOUT` threshold):** the
  blocked item is *foundational*. Do NOT build speculative dependents.
  In order: (a) one careful diagnostic pass; (b) if a reversible
  default exists, take it as a heavily-tagged speculation; (c) else
  **stop the run early, bank clean, write ESCALATION, end.** Log
  `negative_work_avoided` to run-metrics.jsonl.

A 2-hour run stopping at the right decision beats an 8-hour run
building six modules on a wrong foundation.
