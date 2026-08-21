ALTER TABLE tasks DROP COLUMN IF EXISTS version_id;
DROP INDEX IF EXISTS idx_project_versions_project;
DROP TABLE IF EXISTS project_versions;
