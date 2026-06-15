-- ── Sous-module Maths (édition de formules mathématiques/logiques en LaTeX) ──

CREATE SCHEMA IF NOT EXISTS office_maths;

CREATE OR REPLACE FUNCTION office_maths.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

-- ── Formules ──────────────────────────────────────────────────────────────────
-- Le code LaTeX vit dans un fichier .kbmath (JSON gzip) du module Files ; la base
-- ne garde que la métadonnée. file_id référence ce fichier de contenu.

CREATE TABLE IF NOT EXISTS office_maths.formulas (
    id            UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id      UUID NOT NULL,
    name          VARCHAR(255) NOT NULL DEFAULT 'Nouvelle formule',
    description   TEXT,
    file_id       UUID,
    is_trashed    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_omaths_formulas_owner   ON office_maths.formulas(owner_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_omaths_formulas_trashed ON office_maths.formulas(owner_id, is_trashed);

DROP TRIGGER IF EXISTS formulas_updated_at ON office_maths.formulas;
CREATE TRIGGER formulas_updated_at
    BEFORE UPDATE ON office_maths.formulas
    FOR EACH ROW EXECUTE FUNCTION office_maths.set_updated_at();
