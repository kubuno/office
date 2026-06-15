ALTER TABLE documents     DROP COLUMN IF EXISTS file_id;
ALTER TABLE spreadsheets  DROP COLUMN IF EXISTS file_id;
ALTER TABLE presentations DROP COLUMN IF EXISTS file_id;
ALTER TABLE projects      DROP COLUMN IF EXISTS file_id;
ALTER TABLE diagrams      DROP COLUMN IF EXISTS file_id;
