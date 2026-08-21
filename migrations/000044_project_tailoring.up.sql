-- Tailoring: a project shows only the artifacts it actually uses.
--
-- Without this, every capability added to the module would pile into the same
-- ribbon for every project, however simple. A row here overrides the default for
-- one artifact of one project; absence of a row means "use the default", so the
-- table stays empty until someone actually changes something.
CREATE TABLE IF NOT EXISTS pm_project_settings (
    project_id   UUID        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    artifact_key VARCHAR(40) NOT NULL,
    enabled      BOOLEAN     NOT NULL DEFAULT TRUE,
    -- Per-artifact options (thresholds, units…). Kept open on purpose: each
    -- artifact reads the keys it knows and ignores the rest.
    config       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, artifact_key)
);

-- How the project is run. Drives which views are offered by default: a predictive
-- project opens on the schedule, an agile one on the board.
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS methodology VARCHAR(12) NOT NULL DEFAULT 'predictive';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'projects_methodology_check'
    ) THEN
        ALTER TABLE projects
            ADD CONSTRAINT projects_methodology_check
            CHECK (methodology IN ('predictive', 'agile', 'hybrid'));
    END IF;
END $$;
