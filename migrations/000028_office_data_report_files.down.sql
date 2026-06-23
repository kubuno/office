DROP INDEX IF EXISTS office_data.idx_odata_reports_file;
ALTER TABLE office_data.reports DROP COLUMN IF EXISTS file_id;
