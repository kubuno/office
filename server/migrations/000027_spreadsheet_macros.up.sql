-- Macros « container-bound » : stockées DANS le classeur (voyagent avec lui à la
-- duplication/export, supprimées avec lui). Remplace la table office_script.macros
-- (qui ne liait les macros au document que par un id externe) pour les macros liées.
-- Forme : [{ "id": "<uuid>", "name": "<nom>", "source": "<code js>" }]
ALTER TABLE spreadsheets
    ADD COLUMN IF NOT EXISTS macros JSONB NOT NULL DEFAULT '[]'::jsonb;
