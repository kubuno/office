-- Lien vers le fichier SOURCE importé (.docx/.odt) d'un document.
-- Sans cela, /office/open-by-file ne retrouvait jamais le document déjà créé pour
-- un fichier importé (documents.file_id pointe vers le fichier de CONTENU généré,
-- pas vers le fichier source), et recréait donc une copie à chaque ouverture.
ALTER TABLE documents ADD COLUMN IF NOT EXISTS source_file_id UUID;
CREATE INDEX IF NOT EXISTS idx_office_docs_source_file ON documents(source_file_id)
    WHERE source_file_id IS NOT NULL;
