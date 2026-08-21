-- Project types: a project is now either a "management" project (the existing
-- Gantt/CPM planning tool) or a "cloud" project (a container of Kubuno resources
-- with an immutable identifier and key/value labels, modelled on cloud consoles).
-- Existing rows are all management projects.
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS kind VARCHAR(20) NOT NULL DEFAULT 'management'
        CHECK (kind IN ('management', 'cloud'));

-- Human-chosen, URL-safe identifier. Derived from the name at creation, then
-- IMMUTABLE and unique across the instance — the counterpart of a cloud project
-- ID. NULL for management projects, which are addressed by their UUID only.
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS slug VARCHAR(30);

-- Enforce uniqueness only where a slug exists (management projects keep NULL).
CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_slug ON projects(slug) WHERE slug IS NOT NULL;

-- Key/value labels for organising and filtering projects (max enforced in app).
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}';
