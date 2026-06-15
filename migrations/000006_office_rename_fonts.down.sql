ALTER TABLE IF EXISTS fonts RENAME TO user_fonts;
DROP INDEX IF EXISTS idx_office_fonts_user;
CREATE INDEX idx_office_uf_user ON user_fonts(user_id);
ALTER TABLE user_fonts RENAME CONSTRAINT uq_office_font TO uq_user_font;
