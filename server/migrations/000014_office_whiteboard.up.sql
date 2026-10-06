CREATE SCHEMA IF NOT EXISTS office_wb;

CREATE OR REPLACE FUNCTION office_wb.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

CREATE TABLE office_wb.boards (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id        UUID NOT NULL,
    title           VARCHAR(500) NOT NULL DEFAULT 'Nouveau tableau',
    description     TEXT,
    thumbnail_path  TEXT,
    share_token     VARCHAR(64) UNIQUE DEFAULT md5(random()::text || clock_timestamp()::text),
    is_public       BOOLEAN NOT NULL DEFAULT FALSE,
    background      VARCHAR(10) NOT NULL DEFAULT 'dots',
    collaborators   JSONB NOT NULL DEFAULT '[]',
    element_count   INTEGER NOT NULL DEFAULT 0,
    frame_count     INTEGER NOT NULL DEFAULT 0,
    is_trashed      BOOLEAN NOT NULL DEFAULT FALSE,
    trashed_at      TIMESTAMPTZ,
    last_edited_at  TIMESTAMPTZ,
    last_edited_by  UUID,
    is_starred      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_wb_boards_owner ON office_wb.boards(owner_id, updated_at DESC);
CREATE INDEX idx_wb_boards_token ON office_wb.boards(share_token) WHERE is_public = TRUE;

CREATE TRIGGER boards_updated_at
    BEFORE UPDATE ON office_wb.boards
    FOR EACH ROW EXECUTE FUNCTION office_wb.set_updated_at();

-- Snapshot Yjs complet (un seul par board)
CREATE TABLE office_wb.yjs_snapshots (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    board_id     UUID NOT NULL REFERENCES office_wb.boards(id) ON DELETE CASCADE,
    snapshot     BYTEA NOT NULL,
    update_count INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_wb_snapshot_board ON office_wb.yjs_snapshots(board_id);

-- Updates incrémentaux Yjs
CREATE TABLE office_wb.yjs_updates (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    board_id    UUID NOT NULL REFERENCES office_wb.boards(id) ON DELETE CASCADE,
    update_data BYTEA NOT NULL,
    origin      VARCHAR(100),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_wb_updates_board ON office_wb.yjs_updates(board_id, created_at ASC);

-- Cadres (métadonnées dupliquées depuis Yjs)
CREATE TABLE office_wb.frames (
    id             UUID PRIMARY KEY,
    board_id       UUID NOT NULL REFERENCES office_wb.boards(id) ON DELETE CASCADE,
    title          VARCHAR(255) NOT NULL DEFAULT 'Cadre',
    position       INTEGER NOT NULL DEFAULT 0,
    thumbnail_path TEXT,
    slide_id       UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_wb_frames_board ON office_wb.frames(board_id, position);
