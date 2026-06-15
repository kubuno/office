ALTER TABLE slides ADD COLUMN IF NOT EXISTS background JSONB NOT NULL DEFAULT '{"type":"color","color":"#ffffff"}';
ALTER TABLE slides ADD COLUMN IF NOT EXISTS notes      TEXT  NOT NULL DEFAULT '';
ALTER TABLE slides ADD COLUMN IF NOT EXISTS elements   JSONB NOT NULL DEFAULT '[]';
ALTER TABLE slides ADD COLUMN IF NOT EXISTS transition JSONB NOT NULL DEFAULT '{"type":"none","duration":0.3}';
ALTER TABLE diagram_pages ADD COLUMN IF NOT EXISTS data JSONB NOT NULL DEFAULT '{"shapes":[],"connectors":[]}';
