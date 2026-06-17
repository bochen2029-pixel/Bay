---
name: audit
description: Whole-system question. 1M-context surveillance play. Load full canon + all contracts + tests + module headers; answer a focused question with file:line references. Example questions — "where could a null propagate into the projection?", "which modules write to items outside db::write_events?", "does any code path bypass the LLM firewall?"
---

# /audit <question>

The 1M-context surveillance play. A focused question that requires
pattern-matching across the corpus.

## When to use

- "Where could X propagate?" (null, PHI, unchecked error, etc.)
- "Which modules do Y?" (write to items, emit events, touch the LLM)
- "Does any code path violate Z?" (the firewall, append-only, caps)
- "What's the full set of W?" (event types, commands, error codes)

## Load pattern

Full canon + all contracts + all tests + module headers. Skip module
internals unless the question needs them. Budget: surveillance, up to
~800K tokens on a 1M window.

## Output

Structured report with `file:line` references. Not prose. Each finding:
- The location
- What it does
- Why it answers (or partially answers) the question
- Severity (if a violation)

## For Bay

Common audits to run during the v0.2.0 revamp:
- `/audit "where could the LLM firewall be bypassed?"` — after Phase 2d.
- `/audit "which code paths write to items outside db::write_events?"`
- `/audit "does any path UPDATE or DELETE from events?"` — after Phase 2b.
- `/audit "where could cap enforcement be bypassed?"`
- `/audit "which modules does SPEC §6 list that don't exist?"` — Phase 3.
