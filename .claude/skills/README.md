# Skills — Bay v0.2.0 revamp run

Thirteen standard skills adapted from solo-enterprise-architect v7.
Each is a single `SKILL.md` in its own directory. Tiered loading: the
core skills (`bootstrap`, `save`, `verify`, `close-run`, `stuck`,
`speculate`) are always-loaded; the rest are on-demand reference,
pulled only when the relevant skill fires.

## Skill registry

| Skill | Trigger | Tier | Function |
|---|---|---|---|
| `/bootstrap` | Session start | core | Five-step bootstrap + run-lock |
| `/save` | Save cadence | core | Atomic save point + metrics increment |
| `/verify` | Continuous cadence / every 3 modules | core | Cold verifier + joint-wrong + lifecycle |
| `/close-run` | Run end | core | Active-review queue, final commit, metrics, tag, lock release |
| `/stuck` | Stuck-signal trip | core | BLOCKER + fan-out rule + SUSPECT |
| `/speculate` | Ambiguous reversible decision | core | Log QUESTIONS, inline tag, lifecycle |
| `/checkpoint` | Optional pre-compaction | ref | Render COMPACTION_BRIEF, force save |
| `/runway` | On demand | ref | Elapsed, cadence, speculation budget, BLOCKER depth, rework_rate |
| `/calibrate` | Substrate change / monthly | ref | Read run-metrics; report trends + complacency |
| `/audit <q>` | Whole-system question | ref | 1M audit play |
| `/refactor-impact <c>` | Contract change w/ consumers | ref | Cross-module refactor |
| `/cold-start-check` | Every 5–10 modules | ref | Canon self-sufficiency |
| `/inspect <resource>` | Measure-first | ref | Pre-flight inspection (chunker at C:\chunker\ is sub-skill) |

## Precedence

When multiple skills could trigger, prefer the most specific
(narrower trigger / longer description). Conflicts declared here
(currently none).
