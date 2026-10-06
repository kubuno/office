-- Date constraints on a task, and the deadline it is judged against.
--
-- Until now a task could only be pushed by its predecessors. Real plans also have
-- fixed points: a delivery that cannot start before a permit is granted, a
-- milestone that must land on a contractual date. The eight types below are the
-- standard vocabulary (MS Project / PMI): two flexible, four semi-flexible that
-- bound one end, two inflexible that pin it.
ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS constraint_type VARCHAR(4) NOT NULL DEFAULT 'ASAP',
    -- Required by every type except ASAP and ALAP.
    ADD COLUMN IF NOT EXISTS constraint_date DATE,
    -- A deadline does NOT move the schedule: it only says when the task was due,
    -- so a plan can show it is late without pretending the dates changed.
    ADD COLUMN IF NOT EXISTS deadline_date DATE,
    ADD COLUMN IF NOT EXISTS deadline_missed BOOLEAN NOT NULL DEFAULT FALSE;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tasks_constraint_type_check') THEN
        ALTER TABLE tasks ADD CONSTRAINT tasks_constraint_type_check
            CHECK (constraint_type IN ('ASAP','ALAP','SNET','SNLT','FNET','FNLT','MSO','MFO'));
    END IF;
END $$;
