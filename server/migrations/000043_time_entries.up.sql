-- Time entries: a dated log of hours worked on a task. The sum of a task's
-- entries is rolled up into tasks.spent_hours (kept distinct from the estimate).
CREATE TABLE IF NOT EXISTS time_entries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id     UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL,
    spent_on    DATE NOT NULL DEFAULT CURRENT_DATE,
    hours       DOUBLE PRECISION NOT NULL DEFAULT 0,
    activity    VARCHAR(40) NOT NULL DEFAULT 'development',
    comment     TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_time_entries_task ON time_entries(task_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_project ON time_entries(project_id);
