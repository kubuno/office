-- Who is told what, by whom, how often — and what was decided.
--
-- The plan is only worth keeping if it can be checked against the stakeholder
-- register: the useful question is not "what do we send?" but "who receives
-- nothing?". A stakeholder with power and interest and no line in this plan is
-- the same defect as a requirement nothing realises.

CREATE TABLE IF NOT EXISTS pm_communication (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        VARCHAR(300) NOT NULL,
    -- Why it exists. A report nobody can name a purpose for is a habit.
    purpose     TEXT         NOT NULL DEFAULT '',
    channel     VARCHAR(20)  NOT NULL DEFAULT 'email',
    format      VARCHAR(300) NOT NULL DEFAULT '',
    frequency   VARCHAR(12)  NOT NULL DEFAULT 'weekly',
    -- Who prepares and sends it.
    owner_id    UUID,
    -- When it is next expected. Past and unsent is the only figure that asks for
    -- anything.
    next_due    DATE,
    is_active   BOOLEAN      NOT NULL DEFAULT TRUE,
    position    INT          NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- Who receives it. The join with the register is what lets the plan be audited.
CREATE TABLE IF NOT EXISTS pm_communication_audience (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    communication_id UUID NOT NULL REFERENCES pm_communication(id) ON DELETE CASCADE,
    stakeholder_id   UUID NOT NULL REFERENCES pm_stakeholder(id) ON DELETE CASCADE,
    UNIQUE (communication_id, stakeholder_id)
);

-- What was actually sent, as opposed to what was planned. The gap between the
-- two is the point of keeping a log at all.
CREATE TABLE IF NOT EXISTS pm_communication_log (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id       UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    communication_id UUID REFERENCES pm_communication(id) ON DELETE SET NULL,
    sent_on          DATE NOT NULL DEFAULT CURRENT_DATE,
    summary          VARCHAR(300) NOT NULL DEFAULT '',
    sent_by          UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The decision log. A project makes choices that outlive the reason for them;
-- six months on, nobody remembers what was ruled out and why.
CREATE TABLE IF NOT EXISTS pm_decision (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code         VARCHAR(40)  NOT NULL DEFAULT '',
    title        VARCHAR(300) NOT NULL,
    -- The question that had to be settled.
    context      TEXT         NOT NULL DEFAULT '',
    -- What was chosen, and why. Kept apart from the alternatives on purpose:
    -- a decision without its discarded options cannot be revisited honestly.
    decision     TEXT         NOT NULL DEFAULT '',
    rationale    TEXT         NOT NULL DEFAULT '',
    alternatives TEXT         NOT NULL DEFAULT '',
    consequences TEXT         NOT NULL DEFAULT '',
    status       VARCHAR(12)  NOT NULL DEFAULT 'proposed',
    decided_on   DATE,
    decided_by   UUID,
    -- Who took it, when it is a stakeholder rather than an account.
    stakeholder_id UUID REFERENCES pm_stakeholder(id) ON DELETE SET NULL,
    task_id      UUID REFERENCES tasks(id) ON DELETE SET NULL,
    risk_id      UUID REFERENCES pm_risk(id) ON DELETE SET NULL,
    -- A decision that replaces an earlier one, so the log reads as a history
    -- rather than a pile of contradictions.
    supersedes_id UUID REFERENCES pm_decision(id) ON DELETE SET NULL,
    position     INT          NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_comm_channel_check') THEN
        ALTER TABLE pm_communication ADD CONSTRAINT pm_comm_channel_check
            CHECK (channel IN ('email', 'meeting', 'report', 'dashboard', 'chat', 'workshop', 'other'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_comm_frequency_check') THEN
        ALTER TABLE pm_communication ADD CONSTRAINT pm_comm_frequency_check
            CHECK (frequency IN ('daily', 'weekly', 'biweekly', 'monthly', 'quarterly',
                                 'milestone', 'on_demand', 'once'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_decision_status_check') THEN
        ALTER TABLE pm_decision ADD CONSTRAINT pm_decision_status_check
            CHECK (status IN ('proposed', 'decided', 'superseded', 'rejected'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_decision_not_self') THEN
        ALTER TABLE pm_decision ADD CONSTRAINT pm_decision_not_self
            CHECK (supersedes_id IS NULL OR supersedes_id <> id);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_comm_project    ON pm_communication(project_id, position);
CREATE INDEX IF NOT EXISTS idx_comm_audience   ON pm_communication_audience(communication_id);
CREATE INDEX IF NOT EXISTS idx_comm_audience_h ON pm_communication_audience(stakeholder_id);
CREATE INDEX IF NOT EXISTS idx_comm_log        ON pm_communication_log(project_id, sent_on DESC);
CREATE INDEX IF NOT EXISTS idx_decision_project ON pm_decision(project_id, position);
CREATE INDEX IF NOT EXISTS idx_decision_super   ON pm_decision(supersedes_id);
