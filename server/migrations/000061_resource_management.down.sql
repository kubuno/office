DROP TABLE IF EXISTS resource_time_off;
DROP TABLE IF EXISTS resource_skills;
ALTER TABLE project_resources DROP COLUMN IF EXISTS cost_per_use;
ALTER TABLE project_resources DROP COLUMN IF EXISTS overtime_rate;
ALTER TABLE project_resources DROP COLUMN IF EXISTS unit_label;
ALTER TABLE project_resources DROP COLUMN IF EXISTS kind;
