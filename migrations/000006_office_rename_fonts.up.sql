-- Renommage user_fonts → fonts pour uniformité de nommage dans le schéma office
ALTER TABLE IF EXISTS user_fonts RENAME TO fonts;

-- Mettre à jour l'index
DROP INDEX IF EXISTS idx_office_uf_user;
CREATE INDEX idx_office_fonts_user ON fonts(user_id);

-- Renommer la contrainte unique
ALTER TABLE fonts RENAME CONSTRAINT uq_user_font TO uq_office_font;
