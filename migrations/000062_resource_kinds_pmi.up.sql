-- Broaden the resource taxonomy to the PMI categories: human (person, contractor),
-- physical (equipment, facility, material), technological/informational (software,
-- infrastructure, information) and financial (financial, cost).
ALTER TABLE project_resources DROP CONSTRAINT IF EXISTS project_resources_kind_check;
ALTER TABLE project_resources ADD CONSTRAINT project_resources_kind_check
    CHECK (kind IN ('person','contractor','equipment','facility','material','software','infrastructure','information','financial','cost'));
