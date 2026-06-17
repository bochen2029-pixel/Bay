---
name: runway
description: On-demand runway report. Wall-clock elapsed, save cadence, speculation budget remaining, BLOCKER depth, rework_rate, attention-probe landing. The agent's situational-awareness check.
---

# /runway

On demand. Report:

- **Wall-clock elapsed** this run (from run-metrics.jsonl).
- **Save cadence** — last save point timestamp, gap since (warn if
  >30min).
- **Speculation budget** — open speculations / 25. ESCALATE if past.
- **BLOCKER depth** — open blockers + their fan-out. Flag any
  foundational (fan-out ≥3) as "do not build speculative dependents."
- **rework_rate** this run — the confident-wrong detector. Rising =
  gates failing.
- **Attention probe** — last landing (token estimate). If not run this
  substrate, note it.
- **Charter health ratio** — stage1/stage2/stage3 decisions. Healthy
  ~80/15-20/0-5.

Output as a compact block, not prose. The operator (or you) uses this
to decide: continue, compact, escalate, or close-run.
