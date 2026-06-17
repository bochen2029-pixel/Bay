---
name: close-run
description: Run end. Produces not a narrative but an ordered accept/reject queue — each autonomous decision and speculation as a discrete item, cheapest-to-verify first, each with the exact `git revert <sha>` ready. Plus final commit, run-metrics emit, tag, and lock release. Rotate ordering/shape each run so review can't become pattern-matched.
---

# /close-run

Trust decay is the *human* failure mode. After three clean runs, the
operator skims. This skill defends mechanically.

## Steps

1. **Final save point.** Any in-flight work committed or stashed.
2. **Active-review accept/reject queue.** Produce a file
   `REVIEW_QUEUE.md` at repo root. Each autonomous decision and
   speculation as a discrete item, **cheapest-to-verify first**. Each
   item has:
   - The decision (one sentence)
   - The commit SHA(s)
   - The exact `git revert <sha>` command ready to paste
   - A one-line "what to check" hint
   - For speculations: the QUESTIONS.md entry ref + suggested lifecycle
     (CONFIRM | UNWIND | ABANDON)
3. **Rotate the queue shape.** Don't present items in the same
   order/shape every run, so review can't become pattern-matched.
   Alternate: by-phase / by-module / by-commit-chronological /
   cheapest-to-verify-first.
4. **Complacency canary (low frequency, ~1 in 5 runs).** If this is a
   canary run, plant one deliberately-flagged decision obviously worth
   a second look into the queue, tagged `CANARY:`. If the operator
   accepts it without comment across several runs, `/calibrate` raises
   `REVIEW_COMPLACENCY`.
5. **Final run-metrics.jsonl record.** The full schema:
   ```json
   {
     "run_id": "...", "substrate": {...},
     "operator_hours": N, "wall_clock_hours": N,
     "modules_completed": N, "modules_reworked": N,
     "loc_shipped": N, "loc_reverted": N, "rework_rate": N,
     "decisions": {"stage1": N, "stage2": N, "stage3_questions": N, "stage3_blockers": N},
     "charter_health_ratio": "N/N/N",
     "subagent_dispatches": N, "subagent_rejections": N, "subagent_rejection_rate": N,
     "compaction_events": N, "post_compaction_ttp_seconds": [...],
     "cold_restart_test": {"ran": bool, "passed": bool},
     "golden_case_failures": N, "joint_wrong_findings": N,
     "negative_work_avoided": N,
     "attention_probe": {"ran": bool, "degradation_token_estimate": N},
     "blockers_open_at_close": N, "speculations_open_at_close": N
   }
   ```
   The five fields that matter most:
   - `rework_rate` (true confident-wrong detector; rising = gates failing)
   - `charter_health_ratio` (healthy ~80/15-20/0-5)
   - `post_compaction_ttp_seconds` (validates survivability; <60s)
   - `subagent_rejection_rate` (zero is suspicious; rising = loose contracts)
   - `joint_wrong_findings` (nonzero proves the Externality Principle earned its keep)
6. **`git tag`** the run end: `autonomous-run-<ts>-end`.
7. **Remove `.run-lock`.**
8. **Update marrow.lock** at the milestone.
9. **Final PROGRESS entry:** "Run closed. N modules completed, N
   reworked, rework_rate N. Review queue at REVIEW_QUEUE.md. N
   speculations open, N blockers open."

## Operator review on return

The operator acts on REVIEW_QUEUE.md items, not prose. For each:
accept (no action) or `git revert <sha>` (paste the ready command).
Resolve QUESTIONS (CONFIRM/UNWIND/ABANDON). Resolve BLOCKERS. Refine
the charter based on stuck patterns. Dispatch the next run.
