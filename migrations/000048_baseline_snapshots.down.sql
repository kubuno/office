DROP INDEX IF EXISTS idx_baseline_primary;
ALTER TABLE project_baselines DROP COLUMN IF EXISTS is_primary;
DROP TABLE IF EXISTS pm_baseline_task_snapshot;
