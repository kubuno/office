-- ── Data/datasets : définition + caches → fichier .kbdst (compressé) ──────────
-- Le cache de résultats (data_cache) potentiellement volumineux quitte la base.
ALTER TABLE office_data.datasets ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE office_data.datasets DROP COLUMN IF EXISTS raw_sql;
ALTER TABLE office_data.datasets DROP COLUMN IF EXISTS query_steps;
ALTER TABLE office_data.datasets DROP COLUMN IF EXISTS schema_cache;
ALTER TABLE office_data.datasets DROP COLUMN IF EXISTS data_cache;
