-- The YouTube video being played, https://www.youtube.com/watch?v=<id>; empty for anything else.
ALTER TABLE mac_media ADD COLUMN IF NOT EXISTS url String DEFAULT '' AFTER artist;
