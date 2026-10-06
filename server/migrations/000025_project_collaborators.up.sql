-- Partage utilisateur-à-utilisateur des projets (même modèle que les autres entités).
CREATE TABLE IF NOT EXISTS project_collaborators (
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL,
    permission  VARCHAR(20) NOT NULL DEFAULT 'edit'
                    CHECK (permission IN ('view', 'comment', 'edit')),
    added_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_project_collab_user ON project_collaborators(user_id);
