DROP TABLE IF EXISTS pm_cost_config;
DROP TABLE IF EXISTS pm_cost_entry;
ALTER TABLE project_resources DROP COLUMN IF EXISTS hourly_rate;
ALTER TABLE tasks DROP COLUMN IF EXISTS budget_cost;
