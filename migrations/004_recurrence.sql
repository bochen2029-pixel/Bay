-- 004_recurrence.sql — I-21 recurring tasks (v0.3, FUTURE_WORK spec).
--
-- One nullable column: the item's recurrence rule, a minimal RFC 5545
-- RRULE subset (FREQ=DAILY|WEEKLY|MONTHLY[;INTERVAL=n]), validated and
-- canonicalized by domain/recurrence.rs before it is ever written.
-- NULL = not recurring (the default for every existing and new item).
--
-- Projection-only change: the source of truth is the event log
-- (ITEM_RECURRENCE_SET sets/clears it; ITEM_CREATED carries it so a
-- spawned instance keeps recurring). ALTER ADD COLUMN on the rebuilt
-- 002 items table is safe; scripts/verify-schema.py handles ALTERed
-- tables with the column-set check.

ALTER TABLE items ADD COLUMN recurrence TEXT;
