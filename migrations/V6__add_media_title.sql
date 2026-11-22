-- What plays, from macOS Now Playing (MediaRemote): the media's title and artist/channel.
ALTER TABLE mac_media
    ADD COLUMN IF NOT EXISTS title LowCardinality(String) DEFAULT '' AFTER app_name,
    ADD COLUMN IF NOT EXISTS artist LowCardinality(String) DEFAULT '' AFTER title;
