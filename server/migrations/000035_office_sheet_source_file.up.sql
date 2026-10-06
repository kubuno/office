-- Origin of a spreadsheet opened from (or imported as) a foreign file: which file
-- it came from, and in which format. Kept so that saving writes back to that file
-- in that format — Excel's behaviour — instead of only updating our own copy.
-- NULL = native spreadsheet. Mirrors documents.source_file_id / source_format.
ALTER TABLE spreadsheets ADD COLUMN IF NOT EXISTS source_file_id UUID;
ALTER TABLE spreadsheets ADD COLUMN IF NOT EXISTS source_format TEXT;
CREATE INDEX IF NOT EXISTS idx_office_ss_source_file ON spreadsheets(source_file_id)
    WHERE source_file_id IS NOT NULL;
