DROP INDEX IF EXISTS idx_projects_parent;
ALTER TABLE projects DROP COLUMN IF EXISTS parent_id;
