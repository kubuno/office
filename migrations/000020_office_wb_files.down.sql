CREATE TABLE IF NOT EXISTS office_wb.yjs_snapshots (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    board_id     UUID NOT NULL REFERENCES office_wb.boards(id) ON DELETE CASCADE,
    snapshot     BYTEA NOT NULL,
    update_count INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (board_id)
);
ALTER TABLE office_wb.boards DROP COLUMN IF EXISTS file_id;
