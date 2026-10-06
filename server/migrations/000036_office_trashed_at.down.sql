DROP INDEX IF EXISTS office_maths.idx_omaths_formulas_trashed_at;
DROP INDEX IF EXISTS office_script.idx_oscript_scripts_trashed_at;
DROP INDEX IF EXISTS office_data.idx_odata_reports_trashed_at;

ALTER TABLE office_maths.formulas  DROP COLUMN IF EXISTS trashed_at;
ALTER TABLE office_script.scripts  DROP COLUMN IF EXISTS trashed_at;
ALTER TABLE office_data.reports    DROP COLUMN IF EXISTS trashed_at;
