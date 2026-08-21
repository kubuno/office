-- Resources attached to a cloud project: which Kubuno modules the project "uses"
-- (the sovereign counterpart of a cloud console's enabled services/APIs). Modules
-- are discovered dynamically, so `module_id` is a bare string with no FK to a core
-- table — the module need not be installed in this schema.
CREATE TABLE IF NOT EXISTS project_modules (
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    module_id   VARCHAR(100) NOT NULL,
    added_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    added_by    UUID,
    PRIMARY KEY (project_id, module_id)
);
CREATE INDEX IF NOT EXISTS idx_project_modules_project ON project_modules(project_id);
