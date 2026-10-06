-- Money: what each package was budgeted, what has actually been spent, and what
-- that says about where the project will land.
--
-- Earned value only works if three numbers are kept apart: what was planned to be
-- done by now, what has actually been done (valued at its budget), and what it
-- cost. Comparing spend to budget alone tells you nothing — a project that has
-- spent half its money may have done a tenth of the work.

-- The budget at completion of a work package. Left NULL rather than zero: a
-- package nobody costed is not a package costed at nothing, and the difference
-- decides whether the project can be measured at all.
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS budget_cost DOUBLE PRECISION;

-- What an hour of this resource costs. Turns the hours already being logged into
-- an actual cost without anyone re-entering them.
ALTER TABLE project_resources ADD COLUMN IF NOT EXISTS hourly_rate DOUBLE PRECISION;

-- Direct costs that are not somebody's time: licences, hardware, subcontracting.
CREATE TABLE IF NOT EXISTS pm_cost_entry (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- Charged to a work package when it belongs to one; project-level otherwise.
    task_id     UUID REFERENCES tasks(id) ON DELETE SET NULL,
    incurred_on DATE NOT NULL DEFAULT CURRENT_DATE,
    amount      DOUBLE PRECISION NOT NULL DEFAULT 0,
    category    VARCHAR(20) NOT NULL DEFAULT 'other',
    description VARCHAR(300) NOT NULL DEFAULT '',
    created_by  UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- How this project measures money.
CREATE TABLE IF NOT EXISTS pm_cost_config (
    project_id          UUID PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    currency            VARCHAR(8) NOT NULL DEFAULT 'EUR',
    -- Applied to logged hours when the resource has no rate of its own.
    default_hourly_rate DOUBLE PRECISION,
    -- The date the measurement is taken at. Every planned value depends on it:
    -- "are we behind?" is meaningless without saying behind as of when. NULL
    -- means today.
    status_date         DATE,
    -- Which forecast the project stands behind. The three give very different
    -- answers, and picking one on purpose beats showing three and choosing later.
    eac_method          VARCHAR(12) NOT NULL DEFAULT 'cpi',
    -- Set when the remaining work has been re-estimated from the bottom up,
    -- which no index can beat.
    manual_etc          DOUBLE PRECISION,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_cost_entry_category_check') THEN
        ALTER TABLE pm_cost_entry ADD CONSTRAINT pm_cost_entry_category_check
            CHECK (category IN ('labour', 'subcontract', 'licence', 'hardware',
                                'travel', 'other'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pm_cost_config_eac_check') THEN
        ALTER TABLE pm_cost_config ADD CONSTRAINT pm_cost_config_eac_check
            -- cpi     : BAC / CPI          — current cost performance continues
            -- budget  : AC + (BAC − EV)    — the rest goes to plan
            -- cpi_spi : AC + (BAC − EV) / (CPI × SPI) — both continue
            -- manual  : AC + a re-estimate of what is left
            CHECK (eac_method IN ('cpi', 'budget', 'cpi_spi', 'manual'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_cost_entry_project ON pm_cost_entry(project_id, incurred_on DESC);
CREATE INDEX IF NOT EXISTS idx_cost_entry_task    ON pm_cost_entry(task_id);
