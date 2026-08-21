-- Project baselines: a saved snapshot of the planned schedule at a point in time,
-- to compare planned vs. actual on the Gantt (OpenProject-style baseline comparison).
-- The per-task snapshot is stored as a JSONB array — it is read wholesale to overlay
-- ghost bars and compute variance columns, never queried row by row.
CREATE TABLE IF NOT EXISTS project_baselines (
    id            UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id    UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name          VARCHAR(200) NOT NULL,
    -- The project's start_date at capture time, so absolute planned dates stay
    -- comparable even if the project start later moves.
    project_start DATE,
    -- [{ task_id, name, early_start, early_finish, duration_days }]
    tasks         JSONB NOT NULL DEFAULT '[]',
    captured_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    captured_by   UUID
);
CREATE INDEX IF NOT EXISTS idx_project_baselines_project ON project_baselines(project_id, captured_at DESC);
