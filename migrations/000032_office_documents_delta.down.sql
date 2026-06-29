DROP TRIGGER IF EXISTS trg_documents_tombstone ON documents;
DROP TRIGGER IF EXISTS trg_documents_change_seq ON documents;
DROP FUNCTION IF EXISTS office_doc_tombstone();
DROP FUNCTION IF EXISTS office_bump_doc_change_seq();
DROP TABLE IF EXISTS document_tombstones;
ALTER TABLE documents DROP COLUMN IF EXISTS change_seq;
DROP SEQUENCE IF EXISTS document_change_seq;
