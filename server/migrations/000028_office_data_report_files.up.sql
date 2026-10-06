-- Back reports with a .kbdrp file in the drive (source of truth = DB; the file
-- makes the report visible/openable from the file browser, like other editors).
ALTER TABLE office_data.reports ADD COLUMN IF NOT EXISTS file_id UUID;
CREATE INDEX IF NOT EXISTS idx_odata_reports_file ON office_data.reports(file_id);
