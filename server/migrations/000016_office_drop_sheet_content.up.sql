-- Tableur : le contenu des feuilles (cellules, largeurs de colonnes/lignes,
-- volets figés) vit dans le FICHIER de contenu du tableur (module files), pas en
-- base — exactement comme les documents (migration 000011). Ces colonnes étaient
-- vestigiales : le code actuel ne les lit ni ne les écrit (cf. content_files +
-- cf::get_sheet_data / set_sheet_data). On les retire pour qu'aucun contenu de
-- fichier ne subsiste en base. La table garde la métadonnée de feuille (name,
-- position).
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS data;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS col_widths;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS row_heights;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS frozen_rows;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS frozen_cols;
