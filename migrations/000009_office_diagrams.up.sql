-- 000009_office_diagrams.up.sql
-- search_path = office, public (set at connection level)

CREATE TABLE diagrams (
    id              UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id        UUID        NOT NULL,
    title           VARCHAR(500) NOT NULL DEFAULT 'Diagramme sans titre',
    diagram_type    VARCHAR(20) NOT NULL DEFAULT 'freeform'
                        CHECK (diagram_type IN ('freeform','flowchart','network','uml','bpmn','mindmap','orgchart')),
    settings        JSONB NOT NULL DEFAULT '{
        "gridEnabled": true,
        "gridSize": 20,
        "snapToGrid": true,
        "snapToShapes": true,
        "showRulers": true,
        "bgColor": "#ffffff"
    }',
    is_starred      BOOL        NOT NULL DEFAULT FALSE,
    is_trashed      BOOL        NOT NULL DEFAULT FALSE,
    trashed_at      TIMESTAMPTZ,
    last_edited_by  UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_diagrams_owner   ON diagrams(owner_id);
CREATE INDEX idx_diagrams_updated ON diagrams(owner_id, updated_at DESC);

CREATE TRIGGER diagrams_updated_at
    BEFORE UPDATE ON diagrams
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

CREATE TABLE diagram_pages (
    id          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    diagram_id  UUID        NOT NULL REFERENCES diagrams(id) ON DELETE CASCADE,
    name        VARCHAR(255) NOT NULL DEFAULT 'Page 1',
    position    INT         NOT NULL DEFAULT 0,
    bg_color    VARCHAR(7)  NOT NULL DEFAULT '#ffffff',
    width       INT         NOT NULL DEFAULT 1654,
    height      INT         NOT NULL DEFAULT 1169,
    data        JSONB       NOT NULL DEFAULT '{"shapes":[],"connectors":[]}',
    is_hidden   BOOL        NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dpages_diagram ON diagram_pages(diagram_id, position);

CREATE TRIGGER diagram_pages_updated_at
    BEFORE UPDATE ON diagram_pages
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

CREATE TABLE custom_shapes (
    id          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id    UUID        NOT NULL,
    name        VARCHAR(255) NOT NULL,
    category    VARCHAR(100) NOT NULL DEFAULT 'Mes formes',
    shape_def   JSONB       NOT NULL,
    thumbnail   TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_custom_shapes_owner ON custom_shapes(owner_id);

CREATE TABLE diagram_shares (
    id              UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    diagram_id      UUID        NOT NULL REFERENCES diagrams(id) ON DELETE CASCADE,
    created_by      UUID        NOT NULL,
    token           VARCHAR(64) UNIQUE NOT NULL DEFAULT encode(uuid_send(gen_random_uuid()) || uuid_send(gen_random_uuid()), 'hex'),
    permission      VARCHAR(10) NOT NULL DEFAULT 'read'
                        CHECK (permission IN ('read', 'edit')),
    is_active       BOOL        NOT NULL DEFAULT TRUE,
    view_count      INT         NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_diagram_shares_token   ON diagram_shares(token) WHERE is_active = TRUE;
CREATE INDEX idx_diagram_shares_diagram ON diagram_shares(diagram_id);
