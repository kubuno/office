-- What might go wrong (and what might go right), and what already has.
--
-- A risk is about the future and carries a probability; an issue is present tense
-- and carries a resolution. Keeping them apart is the point: a register full of
-- things that have already happened stops being a forecast.

CREATE TABLE IF NOT EXISTS pm_risk (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id     UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code           VARCHAR(40)  NOT NULL DEFAULT '',
    title          VARCHAR(300) NOT NULL,
    description    TEXT         NOT NULL DEFAULT '',
    -- Top level of the risk breakdown structure.
    category       VARCHAR(20)  NOT NULL DEFAULT 'technical',
    -- A risk is not only a threat: an opportunity is managed the same way, and
    -- a register that only records bad news trains people to look one way.
    kind           VARCHAR(12)  NOT NULL DEFAULT 'threat',
    -- Qualitative scales, 1 to 5.
    probability    INT          NOT NULL DEFAULT 3,
    impact         INT          NOT NULL DEFAULT 3,
    -- Kept by the database so the ranking cannot disagree with its two factors.
    score          INT GENERATED ALWAYS AS (probability * impact) STORED,
    -- Quantitative side, both optional: expected monetary value needs a chance
    -- and a sum, and most risks are ranked without ever being priced.
    -- Double precision, like the module's other numbers: these are forecasts of
    -- a risk's cost, not ledger entries — precision to the cent would be a lie
    -- about how well the amount is known.
    probability_pct DOUBLE PRECISION,
    monetary_impact DOUBLE PRECISION,
    status         VARCHAR(12)  NOT NULL DEFAULT 'identified',
    -- The person accountable for watching it, not for the work it threatens.
    owner_id       UUID,
    -- The early warning: what tells you it is about to happen.
    trigger_signs  TEXT         NOT NULL DEFAULT '',
    response_strategy VARCHAR(12) NOT NULL DEFAULT 'accept',
    response_plan  TEXT         NOT NULL DEFAULT '',
    -- What is still there once the response has been carried out.
    residual_notes TEXT         NOT NULL DEFAULT '',
    -- A secondary risk: one created by responding to another.
    parent_risk_id UUID REFERENCES pm_risk(id) ON DELETE SET NULL,
    task_id        UUID REFERENCES tasks(id) ON DELETE SET NULL,
    identified_at  DATE,
    closed_at      TIMESTAMPTZ,
    position       INT          NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- The issue log: what is happening now, including risks that came true.
CREATE TABLE IF NOT EXISTS pm_issue (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code         VARCHAR(40)  NOT NULL DEFAULT '',
    title        VARCHAR(300) NOT NULL,
    description  TEXT         NOT NULL DEFAULT '',
    severity     INT          NOT NULL DEFAULT 3,
    status       VARCHAR(12)  NOT NULL DEFAULT 'open',
    owner_id     UUID,
    due_date     DATE,
    resolution   TEXT         NOT NULL DEFAULT '',
    resolved_at  TIMESTAMPTZ,
    -- The risk this issue is the realisation of, when it had been foreseen.
    risk_id      UUID REFERENCES pm_risk(id) ON DELETE SET NULL,
    task_id      UUID REFERENCES tasks(id) ON DELETE SET NULL,
    position     INT          NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_kind_check') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_kind_check
            CHECK (kind IN ('threat', 'opportunity'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_category_check') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_category_check
            CHECK (category IN ('technical', 'external', 'organizational', 'management', 'commercial'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_scale_check') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_scale_check
            CHECK (probability BETWEEN 1 AND 5 AND impact BETWEEN 1 AND 5);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_status_check') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_status_check
            CHECK (status IN ('identified', 'analysing', 'responding', 'occurred', 'closed'));
    END IF;
    -- Threats are avoided, mitigated, transferred; opportunities are exploited,
    -- enhanced, shared. Both can be accepted, and both can be escalated when the
    -- decision is above the project's authority. The pairing with `kind` is
    -- enforced in the handler, where a readable message can be returned.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_response_check') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_response_check
            CHECK (response_strategy IN ('avoid', 'mitigate', 'transfer',
                                         'exploit', 'enhance', 'share',
                                         'accept', 'escalate'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_pct_check') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_pct_check
            CHECK (probability_pct IS NULL OR (probability_pct >= 0 AND probability_pct <= 100));
    END IF;
    -- A risk cannot be its own cause.
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_risk_not_self_parent') THEN
        ALTER TABLE pm_risk ADD CONSTRAINT pm_risk_not_self_parent
            CHECK (parent_risk_id IS NULL OR parent_risk_id <> id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_issue_status_check') THEN
        ALTER TABLE pm_issue ADD CONSTRAINT pm_issue_status_check
            CHECK (status IN ('open', 'in_progress', 'resolved', 'closed'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_issue_severity_check') THEN
        ALTER TABLE pm_issue ADD CONSTRAINT pm_issue_severity_check
            CHECK (severity BETWEEN 1 AND 5);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_risk_project ON pm_risk(project_id, score DESC, position);
CREATE INDEX IF NOT EXISTS idx_risk_parent  ON pm_risk(parent_risk_id);
CREATE INDEX IF NOT EXISTS idx_risk_task    ON pm_risk(task_id);
CREATE INDEX IF NOT EXISTS idx_issue_project ON pm_issue(project_id, status, position);
CREATE INDEX IF NOT EXISTS idx_issue_risk    ON pm_issue(risk_id);
