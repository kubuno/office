-- Origin format of a document opened from (or imported as) a foreign file.
-- Kept so that saving proposes the format the user started from — Word's
-- behaviour — instead of silently defaulting to our own. NULL = native document.
ALTER TABLE documents ADD COLUMN IF NOT EXISTS source_format TEXT;
