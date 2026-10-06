-- ── Whiteboard : snapshot Yjs (document durable) → fichier .kbwbd ─────────────
-- Le snapshot binaire quitte la base. Le journal d'updates incrémentaux
-- (yjs_updates) reste comme tampon temps réel, consolidé dans le fichier.
ALTER TABLE office_wb.boards ADD COLUMN IF NOT EXISTS file_id UUID;
DROP TABLE IF EXISTS office_wb.yjs_snapshots;
