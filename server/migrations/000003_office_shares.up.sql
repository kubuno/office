CREATE TABLE IF NOT EXISTS document_shares (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    token       VARCHAR(64) UNIQUE NOT NULL,
    permission  VARCHAR(20) NOT NULL DEFAULT 'view'
                    CHECK (permission IN ('view', 'comment', 'edit')),
    expires_at  TIMESTAMPTZ,
    created_by  UUID NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at  TIMESTAMPTZ
);

CREATE INDEX idx_office_shares_token ON document_shares(token) WHERE revoked_at IS NULL;
CREATE INDEX idx_office_shares_doc   ON document_shares(document_id);

CREATE TABLE IF NOT EXISTS document_templates (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name         VARCHAR(255) NOT NULL,
    description  TEXT,
    category     VARCHAR(100) NOT NULL DEFAULT 'general',
    icon         VARCHAR(10),
    content_json JSONB NOT NULL DEFAULT '{"type":"doc","content":[]}',
    is_builtin   BOOLEAN NOT NULL DEFAULT FALSE,
    created_by   UUID,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO document_templates (name, description, category, icon, content_json, is_builtin) VALUES
('Document vide', 'Point de départ vierge', 'general', '📄',
 '{"type":"doc","content":[{"type":"paragraph"}]}',
 TRUE),
('Compte rendu', 'Réunion : participants, ordre du jour, actions', 'meeting', '📋',
 '{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Compte rendu de réunion"}]},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Participants"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph"}]}]},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Ordre du jour"}]},{"type":"orderedList","content":[{"type":"listItem","content":[{"type":"paragraph"}]}]},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Notes"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Actions de suivi"}]},{"type":"taskList","content":[{"type":"taskItem","attrs":{"checked":false},"content":[{"type":"paragraph"}]}]}]}',
 TRUE),
('Rapport', 'Structure formelle : résumé, analyse, conclusion', 'work', '📊',
 '{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Titre du rapport"}]},{"type":"paragraph","content":[{"type":"text","marks":[{"type":"italic"}],"text":"Auteur • Date • Version"}]},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Résumé exécutif"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Introduction"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Analyse"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Conclusion"}]},{"type":"paragraph"}]}',
 TRUE),
('Spécification technique', 'Vue d''ensemble, architecture, API', 'tech', '⚙️',
 '{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Spécification technique"}]},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Vue d''ensemble"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Architecture"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"API"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Sécurité"}]},{"type":"paragraph"},{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Déploiement"}]},{"type":"paragraph"}]}',
 TRUE);
