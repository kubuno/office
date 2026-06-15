-- Partage utilisateur-à-utilisateur des tableaux blancs (même modèle que
-- presentation_collaborators / spreadsheet_collaborators).
CREATE TABLE IF NOT EXISTS office_wb.board_collaborators (
    board_id    UUID NOT NULL REFERENCES office_wb.boards(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL,
    permission  VARCHAR(20) NOT NULL DEFAULT 'edit'
                    CHECK (permission IN ('view', 'comment', 'edit')),
    added_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (board_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_wb_board_collab_user ON office_wb.board_collaborators(user_id);
