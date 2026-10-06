-- Change control: what was asked for after the plan was agreed, what it would
-- cost, and who said yes.
--
-- The point of a change log is not to record requests — it is to stop scope from
-- moving without anyone noticing. So the two things kept here that a task list
-- cannot hold are the **assessed impact** (on the schedule, the cost, the scope)
-- and the **decision**, with a name and a date on it.

CREATE TABLE IF NOT EXISTS pm_change_request (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code         VARCHAR(40)  NOT NULL DEFAULT '',
    title        VARCHAR(300) NOT NULL,
    description  TEXT         NOT NULL DEFAULT '',
    -- Why it is being asked for. A change nobody can justify is a preference.
    justification TEXT        NOT NULL DEFAULT '',
    category     VARCHAR(20)  NOT NULL DEFAULT 'scope',
    -- Corrective and preventive actions and defect repairs are changes too, and
    -- PMBOK counts them as such; keeping them here stops them bypassing control.
    kind         VARCHAR(20)  NOT NULL DEFAULT 'change',
    urgency      VARCHAR(10)  NOT NULL DEFAULT 'normal',
    requested_by UUID,
    -- The stakeholder who asked, when it is not an account on the instance.
    stakeholder_id UUID REFERENCES pm_stakeholder(id) ON DELETE SET NULL,
    requested_on DATE NOT NULL DEFAULT CURRENT_DATE,

    -- ── The assessment ──────────────────────────────────────────────────────
    -- Left NULL until somebody has actually done it: an unassessed change and a
    -- change assessed at zero say very different things, and approving the first
    -- is exactly what change control exists to prevent.
    impact_days   INT,
    impact_cost   DOUBLE PRECISION,
    impact_scope  TEXT NOT NULL DEFAULT '',
    impact_risk   TEXT NOT NULL DEFAULT '',
    impact_quality TEXT NOT NULL DEFAULT '',
    assessed_by   UUID,
    assessed_on   DATE,

    -- ── The decision ────────────────────────────────────────────────────────
    status        VARCHAR(16) NOT NULL DEFAULT 'submitted',
    decision_note TEXT        NOT NULL DEFAULT '',
    decided_by    UUID,
    decided_on    DATE,
    -- The baseline captured after the change was approved, so the plan it moved
    -- to can be named rather than inferred.
    baseline_id   UUID,

    task_id      UUID REFERENCES tasks(id) ON DELETE SET NULL,
    risk_id      UUID REFERENCES pm_risk(id) ON DELETE SET NULL,
    decision_id  UUID REFERENCES pm_decision(id) ON DELETE SET NULL,
    position     INT          NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_change_category_check') THEN
        ALTER TABLE pm_change_request ADD CONSTRAINT pm_change_category_check
            CHECK (category IN ('scope', 'schedule', 'cost', 'quality', 'resource',
                                'requirement', 'other'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_change_kind_check') THEN
        ALTER TABLE pm_change_request ADD CONSTRAINT pm_change_kind_check
            CHECK (kind IN ('change', 'corrective', 'preventive', 'defect_repair'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_change_urgency_check') THEN
        ALTER TABLE pm_change_request ADD CONSTRAINT pm_change_urgency_check
            CHECK (urgency IN ('low', 'normal', 'high', 'critical'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_change_status_check') THEN
        ALTER TABLE pm_change_request ADD CONSTRAINT pm_change_status_check
            CHECK (status IN ('submitted', 'assessing', 'approved', 'partially_approved',
                              'rejected', 'deferred', 'implemented', 'withdrawn'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_change_project ON pm_change_request(project_id, position);
CREATE INDEX IF NOT EXISTS idx_change_status  ON pm_change_request(project_id, status);
