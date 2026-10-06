-- ── Script : source_code déplacé vers le module files (.kbscr) ────────────────
-- La source ne vit plus en base ; seule la référence file_id + compiled_code
-- (artefact transitoire régénéré) restent.

ALTER TABLE office_script.scripts ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE office_script.scripts DROP COLUMN IF EXISTS source_code;
