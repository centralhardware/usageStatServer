-- Safari navigations (Biome Safari.Navigations): registrable domain, URL, storefront country. Over events, read FINAL.
CREATE OR REPLACE VIEW safari_navigations AS
SELECT source_device, start_at AS at, fields['1'] AS domain, fields['8'] AS url, fields['5'] AS country
FROM events FINAL
WHERE stream = 'biome:Safari.Navigations';
