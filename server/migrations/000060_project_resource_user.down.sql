DROP INDEX IF EXISTS idx_resources_user;
ALTER TABLE project_resources DROP COLUMN IF EXISTS user_id;
