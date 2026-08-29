ALTER TABLE project_resources DROP CONSTRAINT IF EXISTS project_resources_kind_check;
ALTER TABLE project_resources ADD CONSTRAINT project_resources_kind_check
    CHECK (kind IN ('person','equipment','material','cost'));
