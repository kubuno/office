ALTER TABLE office_data.datasets ADD COLUMN IF NOT EXISTS raw_sql TEXT;
ALTER TABLE office_data.datasets ADD COLUMN IF NOT EXISTS query_steps JSONB NOT NULL DEFAULT '[]';
ALTER TABLE office_data.datasets ADD COLUMN IF NOT EXISTS schema_cache JSONB NOT NULL DEFAULT '[]';
ALTER TABLE office_data.datasets ADD COLUMN IF NOT EXISTS data_cache JSONB;
ALTER TABLE office_data.datasets DROP COLUMN IF EXISTS file_id;
