-- The subsidiary management plans: how each area will be run, as opposed to what
-- it currently holds.
--
-- The registers built so far answer "what": which risks, which changes, which
-- costs. None of them answers "how" — above what score a risk must be escalated,
-- beyond what variance a cost is a problem rather than noise, below what impact
-- the project manager may decide alone. Those thresholds were nowhere, so every
-- artefact judged against a rule hard-coded in the module.
--
-- Which is why this is not a set of text boxes. The prose says how the area is
-- run; the few structured fields beside it are read by the artefacts themselves.

CREATE TABLE IF NOT EXISTS pm_management_plan (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    area        VARCHAR(20) NOT NULL,
    -- Tailoring again: a three-task project needs none of these, and an absent
    -- plan is not a plan left blank.
    is_active   BOOLEAN NOT NULL DEFAULT FALSE,

    -- ── How the area is run, in prose ───────────────────────────────────────
    approach    TEXT NOT NULL DEFAULT '',
    roles       TEXT NOT NULL DEFAULT '',
    procedures  TEXT NOT NULL DEFAULT '',
    tools       TEXT NOT NULL DEFAULT '',

    -- ── The parts the other artefacts read ──────────────────────────────────
    -- Cost and schedule: the variance beyond which a deviation is reported as a
    -- problem. Without it every project is judged against the same silent rule.
    variance_threshold_pct DOUBLE PRECISION,
    -- Risk: the probability × impact score above which a risk must be escalated
    -- rather than merely owned. This is the project's appetite, and it differs.
    risk_appetite_score    INT,
    -- Change: what the project manager may decide alone. Above either, the board
    -- must sit — the delegation nobody writes down and everyone disputes later.
    change_authority_amount DOUBLE PRECISION,
    change_authority_days   INT,
    review_frequency        VARCHAR(12) NOT NULL DEFAULT 'monthly',

    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, area)
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_plan_area_check') THEN
        ALTER TABLE pm_management_plan ADD CONSTRAINT pm_plan_area_check
            CHECK (area IN ('scope', 'requirements', 'schedule', 'cost', 'quality',
                            'resource', 'communications', 'risk', 'procurement',
                            'stakeholder', 'change', 'configuration'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_plan_frequency_check') THEN
        ALTER TABLE pm_management_plan ADD CONSTRAINT pm_plan_frequency_check
            CHECK (review_frequency IN ('weekly', 'biweekly', 'monthly', 'quarterly',
                                        'milestone', 'on_demand'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_plan_appetite_check') THEN
        ALTER TABLE pm_management_plan ADD CONSTRAINT pm_plan_appetite_check
            CHECK (risk_appetite_score IS NULL OR risk_appetite_score BETWEEN 1 AND 25);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_plan_project ON pm_management_plan(project_id, area);
