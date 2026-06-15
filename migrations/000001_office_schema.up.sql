CREATE TABLE IF NOT EXISTS documents (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id        UUID NOT NULL,
    title           VARCHAR(500) NOT NULL DEFAULT 'Sans titre',
    icon            VARCHAR(10),
    cover_url       VARCHAR(1000),
    content_json    JSONB NOT NULL DEFAULT '{"type":"doc","content":[]}',
    content_text    TEXT NOT NULL DEFAULT '',
    word_count      INTEGER NOT NULL DEFAULT 0,
    is_starred      BOOLEAN NOT NULL DEFAULT FALSE,
    is_trashed      BOOLEAN NOT NULL DEFAULT FALSE,
    trashed_at      TIMESTAMPTZ,
    parent_id       UUID REFERENCES documents(id) ON DELETE SET NULL,
    position        FLOAT NOT NULL DEFAULT 0,
    last_editor_id  UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_office_docs_owner  ON documents(owner_id) WHERE is_trashed = FALSE;
CREATE INDEX idx_office_docs_parent ON documents(parent_id);
CREATE INDEX idx_office_docs_fts    ON documents
    USING GIN(to_tsvector('simple', title || ' ' || content_text));

CREATE OR REPLACE FUNCTION office_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER documents_updated_at
    BEFORE UPDATE ON documents
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

CREATE TABLE IF NOT EXISTS document_collaborators (
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL,
    permission  VARCHAR(20) NOT NULL DEFAULT 'view'
                    CHECK (permission IN ('view', 'comment', 'edit')),
    added_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (document_id, user_id)
);
