-- Append-only. Nothing updates or deletes rows here.
CREATE TABLE events (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  ts        INTEGER NOT NULL,                    -- unix ms
  type      TEXT NOT NULL,                       -- enum below
  item_id   TEXT,                                -- nullable for non-item events
  payload   TEXT NOT NULL                        -- JSON
);
CREATE INDEX idx_events_item ON events(item_id);
CREATE INDEX idx_events_ts   ON events(ts);

-- Projection. Rebuildable from events at any time.
CREATE TABLE items (
  id              TEXT PRIMARY KEY,
  content         TEXT NOT NULL,
  tier            TEXT NOT NULL CHECK (tier IN ('inbox','A','B','C')),
  rank            TEXT NOT NULL,                 -- lexicographic fractional indexing
  state           TEXT NOT NULL CHECK (state IN ('active','blocked','done')),
  blocked_reason  TEXT,
  start_at        INTEGER,                       -- unix ms, nullable
  due_at          INTEGER,                       -- unix ms, nullable
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  deleted         INTEGER NOT NULL DEFAULT 0     -- soft-delete flag
);
CREATE INDEX idx_items_tier_rank ON items(tier, rank) WHERE deleted = 0;
