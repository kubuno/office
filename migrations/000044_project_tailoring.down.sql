DROP TABLE IF EXISTS pm_project_settings;
ALTER TABLE projects DROP CONSTRAINT IF EXISTS projects_methodology_check;
ALTER TABLE projects DROP COLUMN IF EXISTS methodology;
