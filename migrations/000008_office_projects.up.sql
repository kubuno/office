-- 000008_office_projects.up.sql
-- search_path = office, public (set at connection level)

CREATE TABLE projects (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id         UUID        NOT NULL,
    title            VARCHAR(500) NOT NULL DEFAULT 'Nouveau projet',
    description      TEXT        NOT NULL DEFAULT '',
    color            VARCHAR(20) NOT NULL DEFAULT '#1a73e8',
    status           VARCHAR(20) NOT NULL DEFAULT 'active'
                         CHECK (status IN ('active', 'on_hold', 'completed', 'cancelled')),
    start_date       DATE,
    end_date         DATE,
    is_starred       BOOL        NOT NULL DEFAULT FALSE,
    is_trashed       BOOL        NOT NULL DEFAULT FALSE,
    trashed_at       TIMESTAMPTZ,
    last_edited_by   UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_projects_owner   ON projects(owner_id);
CREATE INDEX idx_projects_starred ON projects(owner_id, is_starred) WHERE is_starred = TRUE AND is_trashed = FALSE;
CREATE INDEX idx_projects_updated ON projects(owner_id, updated_at DESC);

CREATE TABLE project_resources (
    id          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id  UUID        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        VARCHAR(255) NOT NULL,
    role        VARCHAR(255) NOT NULL DEFAULT '',
    color       VARCHAR(20) NOT NULL DEFAULT '#5f6368',
    capacity    FLOAT8      NOT NULL DEFAULT 1.0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_resources_project ON project_resources(project_id);

CREATE TABLE tasks (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id       UUID        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    parent_id        UUID        REFERENCES tasks(id) ON DELETE CASCADE,
    position         INT         NOT NULL DEFAULT 0,
    wbs              VARCHAR(50) NOT NULL DEFAULT '',
    name             VARCHAR(500) NOT NULL,
    description      TEXT        NOT NULL DEFAULT '',
    status           VARCHAR(20) NOT NULL DEFAULT 'not_started'
                         CHECK (status IN ('not_started', 'in_progress', 'completed', 'cancelled', 'on_hold')),
    priority         VARCHAR(10) NOT NULL DEFAULT 'medium'
                         CHECK (priority IN ('low', 'medium', 'high', 'critical')),
    task_type        VARCHAR(15) NOT NULL DEFAULT 'task'
                         CHECK (task_type IN ('task', 'milestone', 'summary')),
    start_date       DATE,
    end_date         DATE,
    duration_days    INT         NOT NULL DEFAULT 1,
    progress         INT         NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    -- CPM fields
    early_start      INT,
    early_finish     INT,
    late_start       INT,
    late_finish      INT,
    total_float      INT,
    is_critical      BOOL        NOT NULL DEFAULT FALSE,
    -- Computed after CPM
    cpm_dirty        BOOL        NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tasks_project  ON tasks(project_id, position);
CREATE INDEX idx_tasks_parent   ON tasks(parent_id);

CREATE TABLE task_dependencies (
    id              UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id      UUID        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    from_task_id    UUID        NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    to_task_id      UUID        NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    dep_type        VARCHAR(5)  NOT NULL DEFAULT 'FS'
                        CHECK (dep_type IN ('FS', 'SS', 'FF', 'SF')),
    lag_days        INT         NOT NULL DEFAULT 0,
    UNIQUE (from_task_id, to_task_id)
);

CREATE INDEX idx_deps_project   ON task_dependencies(project_id);
CREATE INDEX idx_deps_from      ON task_dependencies(from_task_id);
CREATE INDEX idx_deps_to        ON task_dependencies(to_task_id);

CREATE TABLE task_assignments (
    id           UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    task_id      UUID        NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    resource_id  UUID        NOT NULL REFERENCES project_resources(id) ON DELETE CASCADE,
    units        FLOAT8      NOT NULL DEFAULT 1.0,
    UNIQUE (task_id, resource_id)
);

CREATE INDEX idx_assignments_task     ON task_assignments(task_id);
CREATE INDEX idx_assignments_resource ON task_assignments(resource_id);

-- Trigger: updated_at on projects
CREATE TRIGGER projects_updated_at
    BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

-- Trigger: updated_at on tasks
CREATE TRIGGER tasks_updated_at
    BEFORE UPDATE ON tasks
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

-- Trigger: mark CPM as dirty when tasks or dependencies change
CREATE OR REPLACE FUNCTION office_mark_cpm_dirty()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_TABLE_NAME = 'task_dependencies' THEN
        UPDATE tasks SET cpm_dirty = TRUE
        WHERE project_id = COALESCE(NEW.project_id, OLD.project_id);
    ELSE
        IF TG_OP = 'UPDATE' AND (
            OLD.duration_days IS DISTINCT FROM NEW.duration_days OR
            OLD.start_date IS DISTINCT FROM NEW.start_date OR
            OLD.task_type IS DISTINCT FROM NEW.task_type
        ) THEN
            NEW.cpm_dirty = TRUE;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tasks_cpm_dirty
    BEFORE UPDATE OF duration_days, start_date, task_type ON tasks
    FOR EACH ROW EXECUTE FUNCTION office_mark_cpm_dirty();

CREATE TRIGGER deps_cpm_dirty
    AFTER INSERT OR DELETE OR UPDATE ON task_dependencies
    FOR EACH ROW EXECUTE FUNCTION office_mark_cpm_dirty();
