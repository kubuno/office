-- Professional resource management: resource types, a richer cost model, skills
-- and per-resource availability (time off). Builds on project_resources.

-- Resource type. 'person' and 'equipment' are time-based (Work in MS Project terms:
-- they consume time and have a capacity); 'material' is a consumable measured in a
-- unit; 'cost' is a fixed amount tied to an assignment.
ALTER TABLE project_resources ADD COLUMN kind VARCHAR(16) NOT NULL DEFAULT 'person'
    CHECK (kind IN ('person', 'equipment', 'material', 'cost'));

-- Unit label for a material resource (e.g. 'tonne', 'litre'); NULL for the others.
ALTER TABLE project_resources ADD COLUMN unit_label VARCHAR(32);

-- Richer cost model. `hourly_rate` stays the STANDARD rate (or unit price for a
-- material); these add overtime and a flat per-use charge (MS Project: Cost/Use).
ALTER TABLE project_resources ADD COLUMN overtime_rate DOUBLE PRECISION;
ALTER TABLE project_resources ADD COLUMN cost_per_use  DOUBLE PRECISION;

-- Skills / tags on a resource — used to filter, group and match a resource to work.
CREATE TABLE resource_skills (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_id UUID NOT NULL REFERENCES project_resources(id) ON DELETE CASCADE,
    skill       VARCHAR(64) NOT NULL,
    UNIQUE (resource_id, skill)
);
CREATE INDEX idx_resource_skills_resource ON resource_skills(resource_id);

-- Per-resource availability: leave / time off / days the resource cannot work.
-- Feeds the workload heatmap so an allocation over a day off reads as overload.
CREATE TABLE resource_time_off (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_id UUID NOT NULL REFERENCES project_resources(id) ON DELETE CASCADE,
    from_date   DATE NOT NULL,
    to_date     DATE NOT NULL,
    reason      VARCHAR(120) NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (to_date >= from_date)
);
CREATE INDEX idx_resource_time_off_resource ON resource_time_off(resource_id);
