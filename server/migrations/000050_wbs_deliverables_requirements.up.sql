-- Scope: what the project promises, how it is broken down, and how each promise
-- can be traced from the need that raised it to the work that fulfils it.

-- The WBS dictionary. One entry per work package, describing what the package
-- covers — and, just as importantly, what it does not.
CREATE TABLE IF NOT EXISTS pm_wbs_dictionary (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id              UUID NOT NULL UNIQUE REFERENCES tasks(id) ON DELETE CASCADE,
    code_of_account      VARCHAR(60)  NOT NULL DEFAULT '',
    statement_of_work    TEXT         NOT NULL DEFAULT '',
    acceptance_criteria  TEXT         NOT NULL DEFAULT '',
    assumptions          TEXT         NOT NULL DEFAULT '',
    -- What is explicitly out of scope. The line a work package is measured against
    -- when someone asks for "just one more thing".
    exclusions           TEXT         NOT NULL DEFAULT '',
    quality_requirements TEXT         NOT NULL DEFAULT '',
    risks                TEXT         NOT NULL DEFAULT '',
    responsible          VARCHAR(200) NOT NULL DEFAULT '',
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- Deliverables: what the project actually hands over, followed through to
-- acceptance. A deliverable nobody accepted is not done, however green its task.
CREATE TABLE IF NOT EXISTS pm_deliverable (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id          UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- The work package that produces it. Kept if the task goes away: the promise
    -- outlives the plan that was going to fulfil it.
    task_id             UUID REFERENCES tasks(id) ON DELETE SET NULL,
    code                VARCHAR(40)  NOT NULL DEFAULT '',
    name                VARCHAR(300) NOT NULL,
    description         TEXT         NOT NULL DEFAULT '',
    acceptance_criteria TEXT         NOT NULL DEFAULT '',
    due_date            DATE,
    status              VARCHAR(12)  NOT NULL DEFAULT 'planned',
    accepted_by         UUID,
    accepted_at         TIMESTAMPTZ,
    rejection_reason    TEXT         NOT NULL DEFAULT '',
    position            INT          NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- Requirements, with the attributes a traceability matrix is built from.
CREATE TABLE IF NOT EXISTS pm_requirement (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id          UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code                VARCHAR(40)  NOT NULL DEFAULT '',
    title               VARCHAR(300) NOT NULL,
    description         TEXT         NOT NULL DEFAULT '',
    req_type            VARCHAR(20)  NOT NULL DEFAULT 'functional',
    -- MoSCoW: the vocabulary that forces a real ranking instead of everything
    -- being "high".
    priority            VARCHAR(10)  NOT NULL DEFAULT 'should',
    -- Where it comes from — the stakeholder, the business need, the regulation.
    source              VARCHAR(200) NOT NULL DEFAULT '',
    rationale           TEXT         NOT NULL DEFAULT '',
    status              VARCHAR(12)  NOT NULL DEFAULT 'proposed',
    verification_method VARCHAR(15)  NOT NULL DEFAULT 'test',
    verification_notes  TEXT         NOT NULL DEFAULT '',
    verified_at         DATE,
    position            INT          NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- The traceability matrix itself: a requirement is traced to the deliverable that
-- satisfies it, to the work package that builds it, or to both.
CREATE TABLE IF NOT EXISTS pm_requirement_link (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    requirement_id UUID NOT NULL REFERENCES pm_requirement(id) ON DELETE CASCADE,
    deliverable_id UUID REFERENCES pm_deliverable(id) ON DELETE CASCADE,
    task_id        UUID REFERENCES tasks(id) ON DELETE CASCADE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_deliverable_status_check') THEN
        ALTER TABLE pm_deliverable ADD CONSTRAINT pm_deliverable_status_check
            CHECK (status IN ('planned', 'in_progress', 'delivered', 'accepted', 'rejected'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_requirement_type_check') THEN
        ALTER TABLE pm_requirement ADD CONSTRAINT pm_requirement_type_check
            CHECK (req_type IN ('business', 'stakeholder', 'functional',
                                'non_functional', 'transition', 'quality', 'project'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_requirement_priority_check') THEN
        ALTER TABLE pm_requirement ADD CONSTRAINT pm_requirement_priority_check
            CHECK (priority IN ('must', 'should', 'could', 'wont'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_requirement_status_check') THEN
        ALTER TABLE pm_requirement ADD CONSTRAINT pm_requirement_status_check
            CHECK (status IN ('proposed', 'approved', 'implemented',
                              'verified', 'deferred', 'rejected'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_requirement_verif_check') THEN
        ALTER TABLE pm_requirement ADD CONSTRAINT pm_requirement_verif_check
            CHECK (verification_method IN ('test', 'inspection', 'demonstration', 'analysis'));
    END IF;
    -- A link that traces to nothing is not a link.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_requirement_link_target_check') THEN
        ALTER TABLE pm_requirement_link ADD CONSTRAINT pm_requirement_link_target_check
            CHECK (deliverable_id IS NOT NULL OR task_id IS NOT NULL);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_deliverable_project  ON pm_deliverable(project_id, position);
CREATE INDEX IF NOT EXISTS idx_deliverable_task     ON pm_deliverable(task_id);
CREATE INDEX IF NOT EXISTS idx_requirement_project  ON pm_requirement(project_id, position);
CREATE INDEX IF NOT EXISTS idx_req_link_req         ON pm_requirement_link(requirement_id);
CREATE INDEX IF NOT EXISTS idx_req_link_deliverable ON pm_requirement_link(deliverable_id);
CREATE INDEX IF NOT EXISTS idx_req_link_task        ON pm_requirement_link(task_id);

-- The same requirement traced twice to the same target says nothing new.
-- Partial indexes because a NULL side must not block the other one.
CREATE UNIQUE INDEX IF NOT EXISTS uq_req_link_deliverable
    ON pm_requirement_link(requirement_id, deliverable_id) WHERE deliverable_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_req_link_task
    ON pm_requirement_link(requirement_id, task_id) WHERE task_id IS NOT NULL;
