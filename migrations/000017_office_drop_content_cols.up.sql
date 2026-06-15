-- Stratégie : aucun contenu de fichier ne reste en base. Le contenu des
-- présentations (slides) et des diagrammes (pages) vit déjà dans le fichier de
-- contenu (module files) ; ces colonnes JSONB étaient vestigiales. On les
-- supprime (pas de migration des données — l'existant en base est jetable, le
-- nouveau contenu va dans files). Les tables gardent la métadonnée structurelle
-- (position, is_hidden, dimensions…).

-- Présentations : contenu des slides
ALTER TABLE slides DROP COLUMN IF EXISTS background;
ALTER TABLE slides DROP COLUMN IF EXISTS notes;
ALTER TABLE slides DROP COLUMN IF EXISTS elements;
ALTER TABLE slides DROP COLUMN IF EXISTS transition;

-- Diagrammes : contenu des pages
ALTER TABLE diagram_pages DROP COLUMN IF EXISTS data;
