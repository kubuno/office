-- Tableurs (search_path = office, public)

CREATE TABLE IF NOT EXISTS spreadsheets (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id    UUID NOT NULL,
    title       VARCHAR(500) NOT NULL DEFAULT 'Sans titre',
    is_starred  BOOLEAN NOT NULL DEFAULT FALSE,
    is_trashed  BOOLEAN NOT NULL DEFAULT FALSE,
    trashed_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_office_ss_owner ON spreadsheets(owner_id) WHERE is_trashed = FALSE;

CREATE TRIGGER spreadsheets_updated_at
    BEFORE UPDATE ON spreadsheets
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

CREATE TABLE IF NOT EXISTS spreadsheet_sheets (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    spreadsheet_id  UUID NOT NULL REFERENCES spreadsheets(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL DEFAULT 'Feuille 1',
    position        INTEGER NOT NULL DEFAULT 0,
    data            JSONB NOT NULL DEFAULT '{"cells":{}}',
    col_widths      JSONB NOT NULL DEFAULT '{}',
    row_heights     JSONB NOT NULL DEFAULT '{}',
    frozen_rows     INTEGER NOT NULL DEFAULT 0,
    frozen_cols     INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_office_sh_ss ON spreadsheet_sheets(spreadsheet_id);

CREATE TRIGGER spreadsheet_sheets_updated_at
    BEFORE UPDATE ON spreadsheet_sheets
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

CREATE TABLE IF NOT EXISTS spreadsheet_versions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    spreadsheet_id  UUID NOT NULL REFERENCES spreadsheets(id) ON DELETE CASCADE,
    author_id       UUID NOT NULL,
    snapshot        JSONB NOT NULL DEFAULT '{}',
    label           VARCHAR(255),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_office_sv_ss ON spreadsheet_versions(spreadsheet_id);
