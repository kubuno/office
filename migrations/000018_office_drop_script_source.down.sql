ALTER TABLE office_script.scripts ADD COLUMN IF NOT EXISTS source_code TEXT NOT NULL DEFAULT '// Kubuno Script\n';
ALTER TABLE office_script.scripts DROP COLUMN IF EXISTS file_id;
