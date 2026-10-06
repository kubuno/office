-- Link a project resource to a real directory user (a member of an organizational
-- unit), so a task can be assigned to that person. Loose reference by design: like
-- projects.owner_id, we store the user id without a cross-schema FK to core.users,
-- keeping the module self-contained. NULL = a free-form resource (subcontractor,
-- role placeholder) as before.
ALTER TABLE project_resources ADD COLUMN user_id UUID;

-- A given directory member is added at most once per project.
CREATE UNIQUE INDEX idx_resources_user ON project_resources (project_id, user_id) WHERE user_id IS NOT NULL;
