---
name: calibrate
description: Substrate change or monthly. Read the last N runs from run-metrics.jsonl; report trends — rework_rate over time, charter-health trajectory, compaction TTP distribution, attention-probe landing, subagent-rejection rate, review-complacency flag. The instrument that makes "numbers calibrate" a procedure.
---

# /calibrate

Run at every substrate change and monthly otherwise.

## Reads

The last N runs from `run-metrics.jsonl` (N = 5 minimum, all if fewer).

## Reports

- **rework_rate over time** — is the correctness layer working? Rising
  = confident-wrong output not being caught. Investigate which gates
  failed.
- **charter_health_ratio trajectory** — drift toward stage-3 =
  under-authorized charter. Refine §3 pre-authorizations.
- **post_compaction_ttp_seconds distribution** — validates the
  survivability claim. Should stay <60s. Rising = RUN_STATE subtleties
  insufficient.
- **attention_probe landing** — where does this substrate's reasoning
  degrade? Sets the reasoning budget for CI thresholds.
- **subagent_rejection_rate** — zero is suspicious (validation not
  really running); rising means dispatch contracts too loose.
- **joint_wrong_findings** — count over time. Nonzero proves the
  Externality Principle earned its keep. Trend should stay low; spikes
  mean a contract is ambiguous.
- **REVIEW_COMPLACENCY flag** — if canary runs were accepted without
  comment across several runs, raise the flag.

## Output

A `CALIBRATION_REPORT.md` at repo root with the trends + recommended
charter/threshold adjustments. The operator acts on it; the agent
applies parameter changes (not code changes) on approval.
