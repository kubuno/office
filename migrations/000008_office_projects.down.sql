-- 000008_office_projects.down.sql
DROP TABLE IF EXISTS task_assignments;
DROP TABLE IF EXISTS task_dependencies;
DROP TABLE IF EXISTS tasks;
DROP TABLE IF EXISTS project_resources;
DROP TABLE IF EXISTS projects;
DROP FUNCTION IF EXISTS office_mark_cpm_dirty();
