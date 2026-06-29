-- Sync primitives for documents: per-doc etags (If-Match / delta) + idempotency store.

-- Stable opaque etags. Volatile default → each existing row gets a distinct value
-- on the rewrite; handlers then set a fresh etag on every write.
ALTER TABLE documents ADD COLUMN IF NOT EXISTS etag         TEXT NOT NULL DEFAULT gen_random_uuid()::text;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS content_etag TEXT NOT NULL DEFAULT gen_random_uuid()::text;

-- Idempotency store for create/patch replays (push from offline-first daemon).
-- Keyed by (user, client key) so a retried op returns the same body without re-doing work.
CREATE TABLE IF NOT EXISTS idempotency_keys (
    user_id    UUID        NOT NULL,
    key        TEXT        NOT NULL,
    status     INTEGER     NOT NULL,
    body       JSONB       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, key)
);
CREATE INDEX IF NOT EXISTS idx_office_idem_created ON idempotency_keys(created_at);
