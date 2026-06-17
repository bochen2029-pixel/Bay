---
name: cold-start-check
description: Every 5–10 modules. Load the full canon to a fresh subagent that has not seen the implementation; ask "what is this system? what are the invariants? what's the next action?" Compare its interpretation to reality. Gaps reveal canon insufficiency.
---

# /cold-start-check

Periodic canon self-sufficiency check. Every 5–10 modules built, or
before any planned operator review.

## When to use

- After 5–10 modules completed this run
- Before an operator review (ensure canon is self-sufficient)
- After a substrate change (does the new model interpret canon
  correctly?)
- If compaction-survival seems degraded (cold restarts confused)

## Load pattern

The entire canon (CLAUDE.md, SPEC.md, PROMPTS.md, README.md,
ARCHITECTURE if present, glossary, all module headers, golden cases,
NFRs) into a fresh subagent that has **not** seen the implementation.

Budget: surveillance, ~400–800K depending on canon size.

## Mission

Ask the fresh subagent:
1. "What is this system? What does it do?"
2. "What are the load-bearing invariants?"
3. "What's the current state and next action?" (give it RUN_STATE +
   TASKLIST)
4. "What would you do next?"

Compare to reality. Gaps reveal canon insufficiency — refine
CLAUDE.md / SPEC.md / RUN_STATE subtleties.

## For Bay

The first cold-start-check should land after Phase 2 (correctness
layer complete) — verify a fresh agent reading the reconciled canon
understands the four mechanically-enforced invariants and the
type-level LLM firewall.
