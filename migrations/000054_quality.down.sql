ALTER TABLE pm_cost_entry DROP CONSTRAINT IF EXISTS pm_cost_entry_coq_check;
ALTER TABLE pm_cost_entry DROP COLUMN IF EXISTS coq_category;
DROP TABLE IF EXISTS pm_quality_check;
DROP TABLE IF EXISTS pm_quality_measurement;
DROP TABLE IF EXISTS pm_quality_metric;
