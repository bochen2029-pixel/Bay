# Hooks — Bay v0.2.0 revamp run

Six hooks enforce the autonomy spine. Portable bash (Git Bash on
Windows; `CLAUDE_PROJECT_DIR` env var set by the harness). Each is
idempotent and fail-safe (a hook error warns, never blocks the run
except `stop-completion-gate` and `pre-arch-edit` by design).

| Hook | Event | Function |
|---|---|---|
| `pre-compact-brief.sh` | PreCompact | Render COMPACTION_BRIEF.md (safety net); force save |
| `session-start-bootstrap.sh` | SessionStart (startup, resume, compact) | Bootstrap + run-lock + inject state |
| `stop-completion-gate.sh` | Stop | Substantive checks; block if incomplete |
| `post-edit-tests.sh` | PostToolUse (Edit, Write) | Scoped tests for touched module; save cadence |
| `pre-arch-edit.sh` | PreToolUse (Edit on doctrine/golden) | Require SPEC: / operator protocol |
| `subagent-validate.sh` | SubagentStop | Run return validation; log results |

## Ordering

If multiple hooks fire on one event, the order above is the order.
Declare conflicts in this README (currently none).

## Escape valve

`stop-completion-gate.sh` honors `$RUN_TERMINATE` env var or a
`STOP_ACK.md` file at repo root for emergency termination without
the gate blocking.
