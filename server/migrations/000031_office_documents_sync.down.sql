DROP TABLE IF EXISTS idempotency_keys;
ALTER TABLE documents DROP COLUMN IF EXISTS content_etag;
ALTER TABLE documents DROP COLUMN IF EXISTS etag;
