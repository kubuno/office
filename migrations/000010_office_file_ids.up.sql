-- Associe chaque document Office à un fichier dans le module files.
-- Nullable — renseigné au prochain save après migration (lazy registration).

ALTER TABLE documents     ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE spreadsheets  ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE presentations ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE projects      ADD COLUMN IF NOT EXISTS file_id UUID;
ALTER TABLE diagrams      ADD COLUMN IF NOT EXISTS file_id UUID;
