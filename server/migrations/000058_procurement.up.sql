-- Procurement: what the project buys rather than builds, and who carries the risk.
--
-- The one thing a contract register must record and almost none do is the
-- **contract type**, because it decides who pays when the estimate turns out to
-- be wrong. A fixed price puts that on the supplier; a cost-reimbursable puts it
-- squarely back on the project. A register that lists amounts without saying
-- which is which describes commitments it cannot price the risk of.

CREATE TABLE IF NOT EXISTS pm_procurement (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id   UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    code         VARCHAR(40)  NOT NULL DEFAULT '',
    title        VARCHAR(300) NOT NULL,
    -- What is being bought, in enough detail to be tendered against.
    statement_of_work TEXT    NOT NULL DEFAULT '',
    -- Why it is bought rather than built. The decision nobody writes down and
    -- everyone re-litigates a year later.
    make_or_buy_note  TEXT    NOT NULL DEFAULT '',
    contract_type VARCHAR(16) NOT NULL DEFAULT 'fixed_price',
    supplier_name VARCHAR(200) NOT NULL DEFAULT '',
    supplier_contact VARCHAR(300) NOT NULL DEFAULT '',
    -- The supplier as a stakeholder, when it is one — it usually should be.
    stakeholder_id UUID REFERENCES pm_stakeholder(id) ON DELETE SET NULL,
    -- What the project committed to. NULL while still being tendered.
    value        DOUBLE PRECISION,
    -- The cap on a time-and-material contract. Without it the buyer's exposure
    -- is unbounded, which is the whole point of recording it.
    not_to_exceed DOUBLE PRECISION,
    status       VARCHAR(12)  NOT NULL DEFAULT 'planned',
    awarded_on   DATE,
    starts_on    DATE,
    ends_on      DATE,
    -- Where the bought thing lands in the plan.
    deliverable_id UUID REFERENCES pm_deliverable(id) ON DELETE SET NULL,
    task_id      UUID REFERENCES tasks(id) ON DELETE SET NULL,
    risk_id      UUID REFERENCES pm_risk(id) ON DELETE SET NULL,
    terms        TEXT NOT NULL DEFAULT '',
    performance_note TEXT NOT NULL DEFAULT '',
    -- Filled at closure: a contract left open outlives the project.
    closed_on    DATE,
    closure_note TEXT NOT NULL DEFAULT '',
    position     INT NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- What is owed and when. A contract's value is a promise; the payments are what
-- actually leaves the account.
CREATE TABLE IF NOT EXISTS pm_procurement_payment (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    procurement_id UUID NOT NULL REFERENCES pm_procurement(id) ON DELETE CASCADE,
    label          VARCHAR(300) NOT NULL DEFAULT '',
    due_on         DATE,
    amount         DOUBLE PRECISION NOT NULL DEFAULT 0,
    status         VARCHAR(10) NOT NULL DEFAULT 'planned',
    invoice_ref    VARCHAR(120) NOT NULL DEFAULT '',
    paid_on        DATE,
    -- The expense this payment produced, so the money is counted once.
    cost_entry_id  UUID REFERENCES pm_cost_entry(id) ON DELETE SET NULL,
    position       INT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_procurement_type_check') THEN
        ALTER TABLE pm_procurement ADD CONSTRAINT pm_procurement_type_check
            -- Fixed price: the supplier carries the overrun.
            -- Cost reimbursable: the project does.
            -- Time and material: both, unless a cap says otherwise.
            CHECK (contract_type IN ('fixed_price', 'fixed_incentive', 'cost_plus_fee',
                                     'cost_plus_incentive', 'time_material', 'other'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_procurement_status_check') THEN
        ALTER TABLE pm_procurement ADD CONSTRAINT pm_procurement_status_check
            CHECK (status IN ('planned', 'tendering', 'awarded', 'active',
                              'closed', 'cancelled'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_payment_status_check') THEN
        ALTER TABLE pm_procurement_payment ADD CONSTRAINT pm_payment_status_check
            CHECK (status IN ('planned', 'invoiced', 'paid', 'disputed', 'cancelled'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_procurement_project ON pm_procurement(project_id, position);
CREATE INDEX IF NOT EXISTS idx_payment_procurement ON pm_procurement_payment(procurement_id, due_on);
