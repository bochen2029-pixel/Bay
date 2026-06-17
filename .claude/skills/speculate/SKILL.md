---
name: speculate
description: Ambiguous reversible decision. Pick the most reversible option, log in QUESTIONS.md with context/alternatives/decision/justification/what-would-change-it, mark the implementation with an inline SPECULATION tag, continue. Budget 25 active; past that, ESCALATE and stop.
---

# /speculate

When a decision is genuinely ambiguous but **reversible**:

1. **Pick the most reversible option.**
2. **Log in QUESTIONS.md** with:
   - Filed timestamp + lifecycle (`OPEN → CONFIRMED | UNWOUND | ABANDONED`)
   - Context (what's the situation)
   - Alternatives considered
   - Decision applied
   - Justification (why this default)
   - **What would change the decision** (the load-bearing field — gives
     operator clear basis for review)
   - Inline tag location
3. **Mark the implementation** with inline tag:
   - `// SPECULATION: see QUESTIONS.md#Q<NN>` (Rust/TS/JS)
   - `# SPECULATION:` (`.sql`/`Cargo.toml`/`yaml`)
   - `<!-- SPECULATION: see QUESTIONS.md#Q<NN> -->` (TSX/HTML/Markdown)
4. **Continue.**

## The inline tag is the unlock

`grep -r "SPECULATION:" .` produces the complete list of every
speculation in the codebase. Operator audits linearly on return.
Without the tag, speculations vanish into commit history.

## Budget

- 10–15 active speculations per run: normal.
- Past 25 (configurable in charter §6): ESCALATE and stop gracefully.
- A high speculation rate = charter under-authorization. Operator's
  response is to refine the charter, not raise the budget.

## Lifecycle (v7)

Every speculation resolves to one of:
- **CONFIRMED** → promote to DECISIONS.md ADR; remove inline tag, add
  DECISION ref.
- **UNWOUND** → `git revert` the choice; note why in QUESTIONS.
- **ABANDONED** → delete the code path; close the QUESTIONS entry.

The continuous verifier (`/verify`) flags speculations older than 5
modules and **assigns a target lifecycle state** (not just flags).
`/close-run` reports open-speculation aging. Monthly
`/consolidate-decisions` prunes superseded ADRs, merges duplicates.

## Speculation vs BLOCKER

- **Speculation** = ambiguous BUT a reasonable default exists AND the
  choice is reversible.
- **BLOCKER** = ambiguous AND no reasonable default OR the choice is
  irreversible (cascading consequences if wrong).

Speculate when you can; BLOCKER when you must.
