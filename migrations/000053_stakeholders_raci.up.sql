-- Who the project has to deal with, and who answers for what.
--
-- Two artefacts that only work together: a register says how much power and how
-- much interest each stakeholder has and how engaged they are today versus how
-- engaged the project needs them to be; a RACI says, task by task, who does the
-- work and who answers for it.

CREATE TABLE IF NOT EXISTS pm_stakeholder (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id    UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name          VARCHAR(200) NOT NULL,
    organisation  VARCHAR(200) NOT NULL DEFAULT '',
    role_title    VARCHAR(200) NOT NULL DEFAULT '',
    contact_email VARCHAR(320) NOT NULL DEFAULT '',
    category      VARCHAR(20)  NOT NULL DEFAULT 'internal',
    -- The two axes of the power/interest grid, 1 to 5.
    power         INT NOT NULL DEFAULT 3,
    interest      INT NOT NULL DEFAULT 3,
    -- Engagement today, and engagement the project needs. The gap between the two
    -- is the only actionable part: a register that records only the present state
    -- describes a situation instead of asking for anything.
    engagement_current VARCHAR(12) NOT NULL DEFAULT 'neutral',
    engagement_desired VARCHAR(12) NOT NULL DEFAULT 'supportive',
    expectations       TEXT NOT NULL DEFAULT '',
    influence_notes    TEXT NOT NULL DEFAULT '',
    communication_notes TEXT NOT NULL DEFAULT '',
    -- The account this stakeholder has on the instance, when they have one. A
    -- plain identifier with no foreign key: the directory a deployment uses is
    -- not this module's business, and a hard link would tie the two together.
    user_id       UUID,
    position      INT NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One role per stakeholder per task. Responsible does the work, Accountable
-- answers for it, Consulted is asked beforehand, Informed is told afterwards.
CREATE TABLE IF NOT EXISTS pm_raci (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id     UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id        UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    stakeholder_id UUID NOT NULL REFERENCES pm_stakeholder(id) ON DELETE CASCADE,
    role           CHAR(1) NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (task_id, stakeholder_id)
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_stakeholder_category_check') THEN
        ALTER TABLE pm_stakeholder ADD CONSTRAINT pm_stakeholder_category_check
            CHECK (category IN ('internal', 'external', 'sponsor', 'customer',
                                'supplier', 'regulator', 'team'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_stakeholder_scale_check') THEN
        ALTER TABLE pm_stakeholder ADD CONSTRAINT pm_stakeholder_scale_check
            CHECK (power BETWEEN 1 AND 5 AND interest BETWEEN 1 AND 5);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_stakeholder_engagement_check') THEN
        ALTER TABLE pm_stakeholder ADD CONSTRAINT pm_stakeholder_engagement_check
            CHECK (engagement_current IN ('unaware', 'resistant', 'neutral', 'supportive', 'leading')
               AND engagement_desired IN ('unaware', 'resistant', 'neutral', 'supportive', 'leading'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_raci_role_check') THEN
        ALTER TABLE pm_raci ADD CONSTRAINT pm_raci_role_check
            CHECK (role IN ('R', 'A', 'C', 'I'));
    END IF;
END $$;

-- The rule that makes a RACI worth drawing: exactly one person answers for a
-- task. Two accountable people is nobody accountable. Enforced by the database
-- so no path can create a second one.
CREATE UNIQUE INDEX IF NOT EXISTS uq_raci_one_accountable
    ON pm_raci(task_id) WHERE role = 'A';

CREATE INDEX IF NOT EXISTS idx_stakeholder_project ON pm_stakeholder(project_id, position);
CREATE INDEX IF NOT EXISTS idx_raci_project ON pm_raci(project_id);
CREATE INDEX IF NOT EXISTS idx_raci_task    ON pm_raci(task_id);
CREATE INDEX IF NOT EXISTS idx_raci_holder  ON pm_raci(stakeholder_id);
