-- ── Sous-module Data (BI/Reporting) ─────────────────────────────────────────

CREATE SCHEMA IF NOT EXISTS office_data;

CREATE OR REPLACE FUNCTION office_data.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

-- ── Sources de données ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.datasources (
    id                UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id          UUID NOT NULL,
    name              VARCHAR(255) NOT NULL,
    description       TEXT,
    source_type       VARCHAR(50) NOT NULL DEFAULT 'internal',
    config            JSONB NOT NULL DEFAULT '{}',
    credentials_enc   TEXT,
    connection_status VARCHAR(10) NOT NULL DEFAULT 'untested',
    last_tested_at    TIMESTAMPTZ,
    connection_error  TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_odata_ds_owner ON office_data.datasources(owner_id);

CREATE TRIGGER datasources_updated_at
    BEFORE UPDATE ON office_data.datasources
    FOR EACH ROW EXECUTE FUNCTION office_data.set_updated_at();

-- ── Datasets ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.datasets (
    id               UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id         UUID NOT NULL,
    datasource_id    UUID REFERENCES office_data.datasources(id) ON DELETE SET NULL,
    name             VARCHAR(255) NOT NULL,
    description      TEXT,
    -- Requête SQL brute (mode avancé) OU pipeline de steps JSON
    raw_sql          TEXT,
    query_steps      JSONB NOT NULL DEFAULT '[]',
    -- Schéma résultant calculé à l'exécution
    schema_cache     JSONB NOT NULL DEFAULT '[]',
    row_count        BIGINT,
    -- Résultats mis en cache (pour petits datasets < 100k lignes)
    data_cache       JSONB,
    last_refresh_at  TIMESTAMPTZ,
    refresh_error    TEXT,
    refresh_schedule VARCHAR(100),
    status           VARCHAR(15) NOT NULL DEFAULT 'empty',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_odata_datasets_owner ON office_data.datasets(owner_id);

CREATE TRIGGER datasets_updated_at
    BEFORE UPDATE ON office_data.datasets
    FOR EACH ROW EXECUTE FUNCTION office_data.set_updated_at();

-- ── Mesures ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.measures (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id        UUID NOT NULL,
    dataset_id      UUID NOT NULL REFERENCES office_data.datasets(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    description     TEXT,
    expression      TEXT NOT NULL,
    result_type     VARCHAR(20) NOT NULL DEFAULT 'number',
    format_string   VARCHAR(100),
    display_folder  VARCHAR(255),
    is_valid        BOOLEAN NOT NULL DEFAULT FALSE,
    compile_error   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (dataset_id, name)
);

CREATE INDEX IF NOT EXISTS idx_odata_measures_dataset ON office_data.measures(dataset_id);

CREATE TRIGGER measures_updated_at
    BEFORE UPDATE ON office_data.measures
    FOR EACH ROW EXECUTE FUNCTION office_data.set_updated_at();

-- ── Relations entre datasets ─────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.relations (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id        UUID NOT NULL,
    from_dataset_id UUID NOT NULL REFERENCES office_data.datasets(id) ON DELETE CASCADE,
    from_column     VARCHAR(255) NOT NULL,
    to_dataset_id   UUID NOT NULL REFERENCES office_data.datasets(id) ON DELETE CASCADE,
    to_column       VARCHAR(255) NOT NULL,
    cardinality     VARCHAR(20) NOT NULL DEFAULT 'many_to_one',
    cross_filter    VARCHAR(10) NOT NULL DEFAULT 'single',
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Rapports ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.reports (
    id             UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id       UUID NOT NULL,
    title          VARCHAR(500) NOT NULL DEFAULT 'Nouveau rapport',
    description    TEXT,
    theme          JSONB NOT NULL DEFAULT '{
        "primaryColor": "#1a73e8",
        "fontFamily": "Google Sans, Arial, sans-serif",
        "background": "#f8f9fa",
        "chartPalette": ["#1a73e8","#ea4335","#fbbc04","#34a853","#ff6d00","#a142f4"]
    }',
    page_count     INTEGER NOT NULL DEFAULT 1,
    dataset_ids    UUID[] NOT NULL DEFAULT '{}',
    share_token    VARCHAR(64) UNIQUE,
    is_public      BOOLEAN NOT NULL DEFAULT FALSE,
    is_trashed     BOOLEAN NOT NULL DEFAULT FALSE,
    is_starred     BOOLEAN NOT NULL DEFAULT FALSE,
    thumbnail_url  TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_odata_reports_owner ON office_data.reports(owner_id, updated_at DESC);

CREATE TRIGGER reports_updated_at
    BEFORE UPDATE ON office_data.reports
    FOR EACH ROW EXECUTE FUNCTION office_data.set_updated_at();

-- ── Pages de rapport ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.report_pages (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    report_id   UUID NOT NULL REFERENCES office_data.reports(id) ON DELETE CASCADE,
    title       VARCHAR(255) NOT NULL DEFAULT 'Page 1',
    position    INTEGER NOT NULL DEFAULT 0,
    width       INTEGER NOT NULL DEFAULT 1200,
    height      INTEGER NOT NULL DEFAULT 800,
    background  VARCHAR(50),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_odata_pages_report ON office_data.report_pages(report_id, position);

-- ── Widgets ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_data.widgets (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    page_id     UUID NOT NULL REFERENCES office_data.report_pages(id) ON DELETE CASCADE,
    report_id   UUID NOT NULL,
    widget_type VARCHAR(50) NOT NULL,
    x           INTEGER NOT NULL DEFAULT 0,
    y           INTEGER NOT NULL DEFAULT 0,
    width       INTEGER NOT NULL DEFAULT 400,
    height      INTEGER NOT NULL DEFAULT 300,
    config      JSONB NOT NULL DEFAULT '{}',
    z_index     INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_odata_widgets_page ON office_data.widgets(page_id);

CREATE TRIGGER widgets_updated_at
    BEFORE UPDATE ON office_data.widgets
    FOR EACH ROW EXECUTE FUNCTION office_data.set_updated_at();
