-- Drops the V9 seed so V11's rules replace it (their patterns differ, so ReplacingMergeTree
-- wouldn't). Only parts existing now are affected: V11's insert lands after.
ALTER TABLE title_rules DELETE WHERE 1
