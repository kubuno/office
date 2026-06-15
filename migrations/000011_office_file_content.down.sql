DROP TABLE IF EXISTS office.editing_sessions;

ALTER TABLE diagrams DROP COLUMN IF EXISTS draft_file_id;
ALTER TABLE diagram_pages ADD COLUMN IF NOT EXISTS data JSONB NOT NULL DEFAULT '{"shapes":[],"connectors":[]}';

ALTER TABLE presentations DROP COLUMN IF EXISTS draft_file_id;
ALTER TABLE slides ADD COLUMN IF NOT EXISTS elements   JSONB NOT NULL DEFAULT '[]';
ALTER TABLE slides ADD COLUMN IF NOT EXISTS background JSONB NOT NULL DEFAULT '{"type":"color","color":"#ffffff"}';
ALTER TABLE slides ADD COLUMN IF NOT EXISTS notes      TEXT  NOT NULL DEFAULT '';
ALTER TABLE slides ADD COLUMN IF NOT EXISTS transition JSONB NOT NULL DEFAULT '{"type":"none","duration":0.3}';

ALTER TABLE spreadsheets DROP COLUMN IF EXISTS draft_file_id;
ALTER TABLE spreadsheet_sheets ADD COLUMN IF NOT EXISTS data        JSONB   NOT NULL DEFAULT '{"cells":{}}';
ALTER TABLE spreadsheet_sheets ADD COLUMN IF NOT EXISTS col_widths  JSONB   NOT NULL DEFAULT '{}';
ALTER TABLE spreadsheet_sheets ADD COLUMN IF NOT EXISTS row_heights JSONB   NOT NULL DEFAULT '{}';
ALTER TABLE spreadsheet_sheets ADD COLUMN IF NOT EXISTS frozen_rows INTEGER NOT NULL DEFAULT 0;
ALTER TABLE spreadsheet_sheets ADD COLUMN IF NOT EXISTS frozen_cols INTEGER NOT NULL DEFAULT 0;

ALTER TABLE documents DROP COLUMN IF EXISTS draft_file_id;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS content_json JSONB NOT NULL DEFAULT '{"type":"doc","content":[]}';
ALTER TABLE documents ADD COLUMN IF NOT EXISTS content_text TEXT  NOT NULL DEFAULT '';
