-- Work tracking on tasks: estimated effort and time spent (hours). Complements the
-- schedule (duration/CPM) with a workload dimension. NULL = not estimated.
ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS estimated_hours DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS spent_hours     DOUBLE PRECISION;
