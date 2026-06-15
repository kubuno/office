-- Collaboration temps réel (Yjs) générique pour les éditeurs Office.
-- L'état Yjs (CRDT) vit ICI (snapshot consolidé + journal d'updates incrémentaux,
-- binaires opaques) ; le fichier .kb*** visible reste le snapshot JSON (export /
-- recherche / aperçu), écrit par les clients. Remplace l'ancien brouillon .drafts.
-- entity_type : 'document' | 'spreadsheet' | 'presentation' | 'diagram' (extensible).

CREATE TABLE IF NOT EXISTS collab_updates (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_type VARCHAR(40) NOT NULL,
    entity_id   UUID        NOT NULL,
    update_data BYTEA       NOT NULL,   -- update Yjs binaire (opaque)
    origin      VARCHAR(100),           -- user_id (audit)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_collab_updates_entity
    ON collab_updates (entity_type, entity_id, created_at ASC);

CREATE TABLE IF NOT EXISTS collab_snapshots (
    entity_type VARCHAR(40) NOT NULL,
    entity_id   UUID        NOT NULL,
    snapshot    BYTEA       NOT NULL,   -- état Yjs consolidé (concaténation d'updates)
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (entity_type, entity_id)
);
