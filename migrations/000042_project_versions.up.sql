-- Versions (release milestones): a named container that groups tasks for a
-- roadmap. A task may belong to at most one version. Distinct from a milestone
-- task (which is a dated point) — a milestone can be one of a version's tasks.
CREATE TABLE IF NOT EXISTS project_versions (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        VARCHAR(200) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    start_date  DATE,
    due_date    DATE,
    status      VARCHAR(10) NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'locked', 'closed')),
    position    INT NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_project_versions_project ON project_versions(project_id);

-- A task's optional version (roadmap membership). Detached to NULL if the version goes.
ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS version_id UUID REFERENCES project_versions(id) ON DELETE SET NULL;
