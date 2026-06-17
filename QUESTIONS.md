# QUESTIONS.md — Bay v0.2.0 revamp run

> Decisions made under uncertainty, with reasonable default applied,
> flagged for operator review on return. Append-only. Each entry has
> a lifecycle: OPEN → (CONFIRMED | UNWOUND | ABANDONED).

(none yet — run just started)

---

## Question lifecycle (v7)

Every speculation resolves to one of:
- **CONFIRMED** → promote to DECISIONS.md ADR; remove inline
  `SPECULATION:` tag, add DECISION ref.
- **UNWOUND** → `git revert` the choice; note why here.
- **ABANDONED** → delete the code path; close this entry.

The continuous verifier flags speculations older than 5 modules and
assigns a target lifecycle state.
