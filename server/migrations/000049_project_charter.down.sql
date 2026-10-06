DROP TABLE IF EXISTS pm_charter_milestone;
DROP TABLE IF EXISTS pm_charter_revision;
ALTER TABLE pm_charter DROP CONSTRAINT IF EXISTS pm_charter_status_check;
DROP TABLE IF EXISTS pm_charter;
