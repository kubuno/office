-- Trash retention needs a DATE, not just a flag.
--
-- Six of the module's nine entity types already recorded `trashed_at` next to
-- `is_trashed`; reports, scripts and formulas only ever recorded the flag. An
-- instance-wide retention ("delete for good after N days") cannot be applied to
-- a row that never says WHEN it was thrown away, so the column is added here
-- rather than the retention quietly skipping three editors.
--
-- Existing trashed rows are stamped with the current time rather than left NULL:
-- their real trashing date is unrecoverable, and NULL would mean "never expires",
-- which is the opposite of what an administrator turning retention on expects.
-- Starting their countdown today is the honest reading — nothing is deleted
-- earlier than N days from this migration.

ALTER TABLE office_data.reports    ADD COLUMN IF NOT EXISTS trashed_at TIMESTAMPTZ;
ALTER TABLE office_script.scripts  ADD COLUMN IF NOT EXISTS trashed_at TIMESTAMPTZ;
ALTER TABLE office_maths.formulas  ADD COLUMN IF NOT EXISTS trashed_at TIMESTAMPTZ;

UPDATE office_data.reports   SET trashed_at = NOW() WHERE is_trashed = TRUE AND trashed_at IS NULL;
UPDATE office_script.scripts SET trashed_at = NOW() WHERE is_trashed = TRUE AND trashed_at IS NULL;
UPDATE office_maths.formulas SET trashed_at = NOW() WHERE is_trashed = TRUE AND trashed_at IS NULL;

-- The cleaner scans "trashed and old enough"; the partial index keeps that scan
-- proportional to the trash rather than to the whole table.
CREATE INDEX IF NOT EXISTS idx_odata_reports_trashed_at
    ON office_data.reports (trashed_at) WHERE is_trashed = TRUE;
CREATE INDEX IF NOT EXISTS idx_oscript_scripts_trashed_at
    ON office_script.scripts (trashed_at) WHERE is_trashed = TRUE;
CREATE INDEX IF NOT EXISTS idx_omaths_formulas_trashed_at
    ON office_maths.formulas (trashed_at) WHERE is_trashed = TRUE;
