-- Baselines become comparable data instead of an opaque blob.
--
-- A baseline used to keep its tasks in a JSONB array, which the interface had to
-- unpack and join by hand. Variance, earned value and any future report need to
-- query it — so the snapshot moves into a real table, and the existing blobs are
-- carried over rather than lost.
CREATE TABLE IF NOT EXISTS pm_baseline_task_snapshot (
    baseline_id      UUID NOT NULL REFERENCES project_baselines(id) ON DELETE CASCADE,
    -- Deliberately NOT a foreign key: a baseline records what was planned. Deleting
    -- the task afterwards must not rewrite that history.
    task_id          UUID NOT NULL,
    name             VARCHAR(500) NOT NULL DEFAULT '',
    -- Day offsets from the baseline's own `project_start`, like the schedule uses.
    planned_start    INT,
    planned_finish   INT,
    planned_duration INT NOT NULL DEFAULT 0,
    -- Effort promised at capture time; the basis for earned value later.
    planned_work     DOUBLE PRECISION,
    PRIMARY KEY (baseline_id, task_id)
);

-- The baseline a project is judged against. Only one per project.
ALTER TABLE project_baselines ADD COLUMN IF NOT EXISTS is_primary BOOLEAN NOT NULL DEFAULT FALSE;
CREATE UNIQUE INDEX IF NOT EXISTS idx_baseline_primary
    ON project_baselines(project_id) WHERE is_primary;

-- Carry the existing snapshots across.
INSERT INTO pm_baseline_task_snapshot
    (baseline_id, task_id, name, planned_start, planned_finish, planned_duration)
SELECT b.id,
       (t->>'task_id')::uuid,
       COALESCE(t->>'name', ''),
       (t->>'early_start')::int,
       (t->>'early_finish')::int,
       COALESCE((t->>'duration_days')::int, 0)
FROM project_baselines b, jsonb_array_elements(b.tasks) AS t
WHERE t->>'task_id' IS NOT NULL
ON CONFLICT DO NOTHING;

-- Give every project that already has baselines a primary one: the most recent.
UPDATE project_baselines b SET is_primary = TRUE
WHERE b.id = (
    SELECT x.id FROM project_baselines x
    WHERE x.project_id = b.project_id
    ORDER BY x.captured_at DESC LIMIT 1
);
