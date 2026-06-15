-- Partage utilisateur-à-utilisateur des tableurs (même modèle que document_collaborators).
CREATE TABLE IF NOT EXISTS spreadsheet_collaborators (
    spreadsheet_id UUID NOT NULL REFERENCES spreadsheets(id) ON DELETE CASCADE,
    user_id        UUID NOT NULL,
    permission     VARCHAR(20) NOT NULL DEFAULT 'edit'
                       CHECK (permission IN ('view', 'comment', 'edit')),
    added_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (spreadsheet_id, user_id)
);
