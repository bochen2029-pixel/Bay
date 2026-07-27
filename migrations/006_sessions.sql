-- 006_sessions.sql — VISION v0.3 execution core, part 2: sessions.
--
-- The log finally records WORK, not just management (VISION §3.2):
-- SESSION_STARTED / SESSION_ENDED events project into this table the
-- same way item events project into `items` — pure, rebuildable,
-- append-only at the source. A session is one continuous stretch of
-- attention on one item; the open session IS the "Now" slot.
--
--   outcome  'done'        → the item was finished (the same
--                            transaction co-writes ITEM_STATE_CHANGED,
--                            and recurrence spawns ride along)
--            'progress'    → honest pause, work advanced
--            'interrupted' → focus broke; `reason` records why
--                            (meeting | person | self_switch | blocked
--                             | energy) — the personal interruption
--                            taxonomy no time-tracker gets honestly.
--
-- Invariant: AT MOST ONE OPEN SESSION (ended_at IS NULL) — enforced
-- mechanically by a partial UNIQUE index over a constant expression:
-- any second open row collides on the constant. Same three-layer
-- discipline as the tier caps (command check + this index + tests).

CREATE TABLE sessions (
  id          TEXT PRIMARY KEY,
  item_id     TEXT NOT NULL,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER,
  outcome     TEXT CHECK (outcome IS NULL OR outcome IN ('done','progress','interrupted')),
  reason      TEXT CHECK (reason IS NULL OR reason IN ('meeting','person','self_switch','blocked','energy')),
  note        TEXT,
  CHECK (ended_at IS NULL OR ended_at >= started_at),
  CHECK ((ended_at IS NULL) = (outcome IS NULL)),
  CHECK (reason IS NULL OR outcome = 'interrupted')
);

CREATE UNIQUE INDEX idx_sessions_one_open ON sessions((1)) WHERE ended_at IS NULL;
CREATE INDEX idx_sessions_item ON sessions(item_id);
