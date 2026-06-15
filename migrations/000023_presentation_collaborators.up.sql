-- Partage utilisateur-à-utilisateur des présentations (même modèle que document/spreadsheet).
CREATE TABLE IF NOT EXISTS presentation_collaborators (
    presentation_id UUID NOT NULL REFERENCES presentations(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL,
    permission      VARCHAR(20) NOT NULL DEFAULT 'edit'
                        CHECK (permission IN ('view', 'comment', 'edit')),
    added_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (presentation_id, user_id)
);
