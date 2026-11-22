-- domain: the front browser tab's domain (app blocks of a browser); app_category: the app's own
-- declared category (LSApplicationCategoryType, e.g. public.app-category.developer-tools).
ALTER TABLE mac_usage
    ADD COLUMN IF NOT EXISTS domain LowCardinality(String) DEFAULT '' AFTER title,
    ADD COLUMN IF NOT EXISTS app_category LowCardinality(String) DEFAULT '' AFTER domain;
