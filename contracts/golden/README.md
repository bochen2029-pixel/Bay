# contracts/golden/ — Operator-authored ground truth

> **Read-only to the agent** except via the `SPEC:` + operator-action
> protocol (AUTONOMY_CHARTER §12). These are the only assertions in
> the system the agent did not author. They are the cheapest true
> externality (Externality Principle).

## What these are

Concrete input→expected-output pairs the operator (Bo Chen) authors at
contract-definition time, **before** implementation. 3–8 per boundary.
If the implementation passes its contract tests but fails a golden
case, that's a `JOINT_WRONG` finding — the most dangerous class,
because everything else looked green. The implementer's tests and code
agreed with each other and both disagreed with the operator's intent.

This is the v7 Externality Principle's highest-value-per-operator-minute
lever. Golden cases catch what no LLM verifier (cold-context or
otherwise) can: systematic errors where the implementer AND the
contract-reader misread the spec identically.

## Files

| File | Module | Status | Cases |
|---|---|---|---|
| `projection.json` | `apply_event_to_projection` + `rebuild_projection_inner` | proposed | 7 |
| `swap.json` | `swap_move_inner` | proposed | 6 |
| `caps.json` | `create_item` + `move_item` + `set_item_state` + `swap_move` | proposed | 12 |
| `rank.json` | `rank_between` | frozen (mirrors `scripts/rank-fixtures.json`) | 42 (in scripts/) |

## Status lifecycle

- `proposed` — agent-authored as a proposal; operator reviews and freezes.
- `frozen` — operator-owned; the agent may not edit without `SPEC:` tag
  + logged operator action. Treated like `AUTONOMY_CHARTER.md` itself.

Once frozen, the `_status` field in each JSON file changes to
`"frozen"` and the file becomes read-only to the agent. The
`scripts/check-golden.py` CI check enforces:
1. Every critical-module contract has ≥1 golden case (else fail).
2. No diff touches a `contracts/golden/*.json` file without a `SPEC:`
   tag in the commit message (else fail) — unless `_status: proposed`.

## The JOINT_WRONG detector

When the continuous verifier (`/verify`) runs, it's dispatched with
the contract + golden cases + implementation diff, but NOT the
implementer's reasoning. If the implementation passes contract tests
but fails a golden case, the verifier logs a `JOINT_WRONG:` finding.
This is the most dangerous finding class — it means the tests and
code agreed with each other and both disagreed with the human.

`run-metrics.jsonl` tracks `joint_wrong_findings`. Nonzero proves the
Externality Principle earned its keep.
