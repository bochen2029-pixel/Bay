---
name: bootstrap
description: Five-step bootstrap protocol. Run at every session start and every post-compaction continuation. Acquires the run-lock, reads RUN_STATE/TASKLIST/QUESTIONS/BLOCKERS, checks git status, and runs scoped tests for the current in-progress module to verify reality matches claimed state. Time-to-productivity target <60s.
---

# /bootstrap

Five steps. Every (re)start. Encoded as the `session-start-bootstrap.sh`
SessionStart hook (matchers: startup, resume, compact); this skill is
the agent-driven version for manual invocation.

## Steps

0. **Acquire run-lock.** Check `.run-lock`. If a live lock exists
   (heartbeat < 60min old) → abort with ESCALATION (two sessions
   corrupting one spine). If stale → prior run crashed; absorb into
   crash-recovery flow, then take lock.
1. **Read RUN_STATE.md.** Current snapshot: what's the task, what's
   next, what's blocking, what subtleties must survive compaction.
2. **Read TASKLIST.md.** What's done, what's next, dependencies,
   fan-out, critical-module flags.
3. **Read QUESTIONS.md + BLOCKERS.md** (open items only). Pending
   uncertainty + hard stops.
4. **`git status` + `git log --oneline -20`.** VCS ground truth.
5. **Run scoped tests + golden cases for the current in-progress
   module.** Verify reality matches claimed state. If RUN_STATE says
   tests passing but they fail cold, claimed state is wrong —
   reconcile RUN_STATE to filesystem before proceeding.

## Step 5 is load-bearing

RUN_STATE is written by the agent, who may have been wrong about
reality. After a crash or compaction, run-state files may be ahead of
the actual filesystem. Without step 5, a fresh agent proceeds on stale
state and compounds the error.

Step 5 catches it. RUN_STATE says "ALLOC-002 contract tests passing";
bootstrap runs them cold. If they fail, the first action is to
reconcile RUN_STATE to the actual filesystem.

## Time-to-productivity target

<60s for this Standard/Heavy-tier project. If consistently slower,
refine RUN_STATE's "subtleties" section and PROGRESS's last-5 entries.
