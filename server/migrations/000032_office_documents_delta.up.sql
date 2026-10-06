-- Delta primitives for documents pull: monotonic change_seq + tombstones.
-- Maintained by triggers → no handler changes needed.

CREATE SEQUENCE IF NOT EXISTS document_change_seq;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS change_seq BIGINT NOT NULL DEFAULT nextval('document_change_seq');
CREATE INDEX IF NOT EXISTS idx_office_docs_change_seq ON documents(owner_id, change_seq);

-- Every UPDATE bumps change_seq (INSERT gets it via the column DEFAULT).
CREATE OR REPLACE FUNCTION office_bump_doc_change_seq() RETURNS trigger AS $$
BEGIN
    NEW.change_seq := nextval('document_change_seq');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_documents_change_seq ON documents;
CREATE TRIGGER trg_documents_change_seq BEFORE UPDATE ON documents
    FOR EACH ROW EXECUTE FUNCTION office_bump_doc_change_seq();

-- Hard deletes recorded as tombstones so the delta can report removals.
CREATE TABLE IF NOT EXISTS document_tombstones (
    id         UUID        PRIMARY KEY,
    owner_id   UUID        NOT NULL,
    change_seq BIGINT      NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_office_tomb_change_seq ON document_tombstones(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_doc_tombstone() RETURNS trigger AS $$
BEGIN
    INSERT INTO document_tombstones (id, owner_id, change_seq)
    VALUES (OLD.id, OLD.owner_id, nextval('document_change_seq'))
    ON CONFLICT (id) DO UPDATE SET change_seq = EXCLUDED.change_seq, deleted_at = NOW();
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_documents_tombstone ON documents;
CREATE TRIGGER trg_documents_tombstone AFTER DELETE ON documents
    FOR EACH ROW EXECUTE FUNCTION office_doc_tombstone();
