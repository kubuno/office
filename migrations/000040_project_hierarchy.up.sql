-- Project hierarchy: a project may have one optional parent project (OpenProject's
-- "subproject of" — a project IS the folder of its children, GCP's Folder→Project
-- without a separate folder type). A pure tree; cycles are prevented in the app.
-- Deleting a parent detaches its children to the root rather than cascading.
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS parent_id UUID REFERENCES projects(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_projects_parent ON projects(parent_id) WHERE parent_id IS NOT NULL;
