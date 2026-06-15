CREATE TABLE IF NOT EXISTS document_versions (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    document_id  UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    author_id    UUID NOT NULL,
    content_json JSONB NOT NULL,
    word_count   INTEGER NOT NULL DEFAULT 0,
    label        VARCHAR(255),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_office_versions_doc ON document_versions(document_id, created_at DESC);

CREATE TABLE IF NOT EXISTS document_comments (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    author_id   UUID NOT NULL,
    parent_id   UUID REFERENCES document_comments(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    is_resolved BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_office_comments_doc ON document_comments(document_id);

CREATE TRIGGER comments_updated_at
    BEFORE UPDATE ON document_comments
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();
