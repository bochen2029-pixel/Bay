---
name: refactor-impact
description: Before a contract change with 3+ consumers, load the changing contract + all consumers + their tests; produce coordinated edits with characterization tests preserving behavior. Cross-module refactor play.
---

# /refactor-impact <contract>

The cross-module refactor play. Use before any contract change with
3+ consumers.

## When to use

- Bumping a contract version (e.g. `IAllocationService` v2.0 → v2.1)
- Changing an event payload schema
- Renaming a widely-consumed type
- Restructuring a module boundary

## Load pattern

The changing contract + every consumer module's surface that touches
it + their contract tests. Budget: loading, ~300–500K.

## Output

Coordinated edits across modules with:
- Characterization tests confirming behavior preserved (or explicitly
  changed where intended)
- A migration plan if the change is breaking
- Per-consumer diff with the test implications

## For Bay

Likely refactor-impacts in v0.2.0:
- Phase 2d (`ProjectionEvent` type-level firewall) touches
  `apply_event_to_projection` + every caller → refactor-impact.
- I-17 (undo/redo) may add compensating-event logic that touches
  multiple event-type handlers.
- I-20 (LLM re-org) populates `resulting_event_ids` — touches the
  accept path + the event schema.
- I-21 (recurring) adds `ITEM_RECURRED` — touches `EventType` +
  `apply_event_to_projection` + the inspector + time-travel.
