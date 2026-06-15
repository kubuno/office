DROP INDEX IF EXISTS idx_office_docs_source_file;
ALTER TABLE documents DROP COLUMN IF EXISTS source_file_id;
