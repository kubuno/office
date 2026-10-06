-- Quality: what "good enough" means in numbers, and the evidence that it was met.
--
-- Deliberately not a second issue log — defects already live there — and not a
-- second acceptance criterion: a deliverable already states what it must satisfy.
-- What was missing is the measurable side: a target with a tolerance, measured
-- again and again, so "the quality is fine" stops being an opinion.

CREATE TABLE IF NOT EXISTS pm_quality_metric (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id    UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code          VARCHAR(40)  NOT NULL DEFAULT '',
    name          VARCHAR(300) NOT NULL,
    description   TEXT         NOT NULL DEFAULT '',
    -- How the number is obtained. A metric nobody can reproduce is a slogan.
    method        TEXT         NOT NULL DEFAULT '',
    unit          VARCHAR(40)  NOT NULL DEFAULT '',
    target        DOUBLE PRECISION,
    -- The band around the target that still counts as conforming. Both bounds
    -- optional: some metrics only have a floor ("at least 80 % coverage"), some
    -- only a ceiling ("under 300 ms").
    tolerance_min DOUBLE PRECISION,
    tolerance_max DOUBLE PRECISION,
    -- Which way is better, for metrics with a single bound.
    direction     VARCHAR(8)   NOT NULL DEFAULT 'higher',
    frequency     VARCHAR(12)  NOT NULL DEFAULT 'sprint',
    owner_id      UUID,
    -- The deliverable or work package this metric qualifies, when it qualifies one.
    deliverable_id UUID REFERENCES pm_deliverable(id) ON DELETE SET NULL,
    task_id       UUID REFERENCES tasks(id) ON DELETE SET NULL,
    is_active     BOOLEAN      NOT NULL DEFAULT TRUE,
    position      INT          NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- One reading. Kept as a series rather than a latest value: a metric that sits
-- inside tolerance while drifting steadily towards the edge is the interesting
-- case, and a single number cannot show it.
CREATE TABLE IF NOT EXISTS pm_quality_measurement (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    metric_id   UUID NOT NULL REFERENCES pm_quality_metric(id) ON DELETE CASCADE,
    measured_on DATE NOT NULL DEFAULT CURRENT_DATE,
    value       DOUBLE PRECISION NOT NULL,
    notes       VARCHAR(300) NOT NULL DEFAULT '',
    recorded_by UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A quality audit: the checks actually carried out on a deliverable, and what
-- they found. This is what turns "accepted" from a declaration into evidence.
CREATE TABLE IF NOT EXISTS pm_quality_check (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id     UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    deliverable_id UUID REFERENCES pm_deliverable(id) ON DELETE CASCADE,
    task_id        UUID REFERENCES tasks(id) ON DELETE CASCADE,
    label          VARCHAR(300) NOT NULL,
    -- pending → the check has not been carried out yet.
    result         VARCHAR(10)  NOT NULL DEFAULT 'pending',
    evidence       TEXT         NOT NULL DEFAULT '',
    checked_on     DATE,
    checked_by     UUID,
    -- The issue opened when the check failed, so a failure leads somewhere.
    issue_id       UUID REFERENCES pm_issue(id) ON DELETE SET NULL,
    position       INT          NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- Cost of quality, hung on the expenses that already exist rather than recorded
-- twice. Prevention and appraisal are what a project spends to avoid failure;
-- internal and external failure are what it pays when prevention did not work.
ALTER TABLE pm_cost_entry ADD COLUMN IF NOT EXISTS coq_category VARCHAR(20);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_quality_direction_check') THEN
        ALTER TABLE pm_quality_metric ADD CONSTRAINT pm_quality_direction_check
            CHECK (direction IN ('higher', 'lower', 'target'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_quality_frequency_check') THEN
        ALTER TABLE pm_quality_metric ADD CONSTRAINT pm_quality_frequency_check
            CHECK (frequency IN ('continuous', 'daily', 'weekly', 'sprint', 'monthly', 'milestone', 'once'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_quality_check_result_check') THEN
        ALTER TABLE pm_quality_check ADD CONSTRAINT pm_quality_check_result_check
            CHECK (result IN ('pending', 'pass', 'fail', 'waived'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_cost_entry_coq_check') THEN
        ALTER TABLE pm_cost_entry ADD CONSTRAINT pm_cost_entry_coq_check
            CHECK (coq_category IS NULL OR coq_category IN
                   ('prevention', 'appraisal', 'internal_failure', 'external_failure'));
    END IF;
    -- A check must qualify something.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_quality_check_target_check') THEN
        ALTER TABLE pm_quality_check ADD CONSTRAINT pm_quality_check_target_check
            CHECK (deliverable_id IS NOT NULL OR task_id IS NOT NULL);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_quality_metric_project ON pm_quality_metric(project_id, position);
CREATE INDEX IF NOT EXISTS idx_quality_measure_metric ON pm_quality_measurement(metric_id, measured_on);
CREATE INDEX IF NOT EXISTS idx_quality_check_project  ON pm_quality_check(project_id, position);
CREATE INDEX IF NOT EXISTS idx_quality_check_deliv    ON pm_quality_check(deliverable_id);
