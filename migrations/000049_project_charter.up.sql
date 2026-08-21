-- The project charter: what the project is for, who authorises it, and what
-- "done well" will mean. The document that exists before any task does.
--
-- One per project. Its distinguishing trait is that it can be APPROVED — after
-- which it stops being editable: changing an approved charter is a deliberate
-- act that produces a dated revision, not a silent edit.
CREATE TABLE IF NOT EXISTS pm_charter (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id               UUID NOT NULL UNIQUE REFERENCES projects(id) ON DELETE CASCADE,
    purpose                  TEXT NOT NULL DEFAULT '',
    business_case            TEXT NOT NULL DEFAULT '',
    objectives               TEXT NOT NULL DEFAULT '',
    success_criteria         TEXT NOT NULL DEFAULT '',
    high_level_requirements  TEXT NOT NULL DEFAULT '',
    -- PMBOK keeps assumptions and constraints in their own log; folded in here so
    -- a project does not need a second document to record them.
    assumptions              TEXT NOT NULL DEFAULT '',
    constraints              TEXT NOT NULL DEFAULT '',
    risks_summary            TEXT NOT NULL DEFAULT '',
    budget_summary           TEXT NOT NULL DEFAULT '',
    sponsor                  VARCHAR(200) NOT NULL DEFAULT '',
    pm_name                  VARCHAR(200) NOT NULL DEFAULT '',
    pm_authority             TEXT NOT NULL DEFAULT '',
    status                   VARCHAR(10) NOT NULL DEFAULT 'draft',
    approved_by              UUID,
    approved_at              TIMESTAMPTZ,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_charter_status_check') THEN
        ALTER TABLE pm_charter ADD CONSTRAINT pm_charter_status_check
            CHECK (status IN ('draft', 'approved'));
    END IF;
END $$;

-- Append-only history. Each entry is the charter as it stood when someone chose
-- to reopen it, so an approved version is never overwritten.
CREATE TABLE IF NOT EXISTS pm_charter_revision (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    charter_id  UUID NOT NULL REFERENCES pm_charter(id) ON DELETE CASCADE,
    snapshot    JSONB NOT NULL,
    reason      VARCHAR(300) NOT NULL DEFAULT '',
    revised_by  UUID,
    revised_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_charter_revision ON pm_charter_revision(charter_id, revised_at DESC);

-- The high-level milestones the charter commits to, before any detailed plan
-- exists. They can later be turned into real milestones in the schedule.
CREATE TABLE IF NOT EXISTS pm_charter_milestone (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    charter_id  UUID NOT NULL REFERENCES pm_charter(id) ON DELETE CASCADE,
    name        VARCHAR(300) NOT NULL,
    target_date DATE,
    position    INT NOT NULL DEFAULT 0,
    -- The task created from it, so pressing "generate" twice does not duplicate.
    task_id     UUID
);
CREATE INDEX IF NOT EXISTS idx_charter_milestone ON pm_charter_milestone(charter_id, position);
