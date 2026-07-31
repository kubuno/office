DROP INDEX IF EXISTS idx_office_ss_source_file;
ALTER TABLE spreadsheets DROP COLUMN IF EXISTS source_format;
ALTER TABLE spreadsheets DROP COLUMN IF EXISTS source_file_id;
