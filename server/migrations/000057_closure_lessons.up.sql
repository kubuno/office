-- Closing a project, and what it taught.
--
-- The closure record is short on purpose: almost everything a closure report
-- should say is already recorded elsewhere — deliverables and whether they were
-- accepted, changes and whether they were decided, risks that occurred, issues
-- still open, requirements never verified. What is missing is the act of
-- confronting the project with all of it before declaring it over, and that is
-- what this holds.

CREATE TABLE IF NOT EXISTS pm_closure (
    project_id       UUID PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    status           VARCHAR(10) NOT NULL DEFAULT 'open',
    -- Against the charter: was the purpose actually served?
    objectives_met   TEXT NOT NULL DEFAULT '',
    -- Who accepted the whole, as opposed to each deliverable.
    acceptance_note  TEXT NOT NULL DEFAULT '',
    -- What was handed to whom, and who runs it now.
    handover_note    TEXT NOT NULL DEFAULT '',
    -- Contracts, licences, accesses: the loose ends that outlive a project.
    loose_ends       TEXT NOT NULL DEFAULT '',
    final_note       TEXT NOT NULL DEFAULT '',
    -- Recorded when the project was closed with checks still failing, so the
    -- decision to close anyway is visible rather than lost.
    override_reason  TEXT NOT NULL DEFAULT '',
    closed_on        DATE,
    closed_by        UUID,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- What the project taught. Kept per project but written to be read by the next
-- one — hence the recommendation, which is the only part that travels.
CREATE TABLE IF NOT EXISTS pm_lesson (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id     UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code           VARCHAR(40)  NOT NULL DEFAULT '',
    title          VARCHAR(300) NOT NULL,
    category       VARCHAR(20)  NOT NULL DEFAULT 'process',
    -- Whether it went well or badly. A register of failures alone teaches people
    -- to hide them.
    outcome        VARCHAR(10)  NOT NULL DEFAULT 'negative',
    situation      TEXT NOT NULL DEFAULT '',
    what_happened  TEXT NOT NULL DEFAULT '',
    -- The part that travels to the next project. Without it a lesson is an
    -- anecdote.
    recommendation TEXT NOT NULL DEFAULT '',
    -- Where it came from, so a lesson can be traced to what produced it.
    task_id        UUID REFERENCES tasks(id) ON DELETE SET NULL,
    risk_id        UUID REFERENCES pm_risk(id) ON DELETE SET NULL,
    issue_id       UUID REFERENCES pm_issue(id) ON DELETE SET NULL,
    change_id      UUID REFERENCES pm_change_request(id) ON DELETE SET NULL,
    status         VARCHAR(10) NOT NULL DEFAULT 'draft',
    recorded_by    UUID,
    recorded_on    DATE NOT NULL DEFAULT CURRENT_DATE,
    position       INT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_closure_status_check') THEN
        ALTER TABLE pm_closure ADD CONSTRAINT pm_closure_status_check
            CHECK (status IN ('open', 'closing', 'closed'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_lesson_category_check') THEN
        ALTER TABLE pm_lesson ADD CONSTRAINT pm_lesson_category_check
            CHECK (category IN ('process', 'technical', 'people', 'supplier',
                                'estimation', 'communication', 'risk', 'other'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_lesson_outcome_check') THEN
        ALTER TABLE pm_lesson ADD CONSTRAINT pm_lesson_outcome_check
            CHECK (outcome IN ('positive', 'negative', 'mixed'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_lesson_status_check') THEN
        ALTER TABLE pm_lesson ADD CONSTRAINT pm_lesson_status_check
            CHECK (status IN ('draft', 'validated', 'shared'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_lesson_project ON pm_lesson(project_id, position);
