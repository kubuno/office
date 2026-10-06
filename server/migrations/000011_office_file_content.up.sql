-- Move all office content out of the database and into the Files module.
-- Content (document JSON, sheet data, slide elements, diagram page data) is now
-- stored as JSON files via the Files IPC.  The DB tables keep only metadata.

-- Documents: drop content columns, add draft tracking
ALTER TABLE documents DROP COLUMN IF EXISTS content_json;
ALTER TABLE documents DROP COLUMN IF EXISTS content_text;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS draft_file_id UUID;

-- Spreadsheets: drop sheet data columns, add draft tracking
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS data;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS col_widths;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS row_heights;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS frozen_rows;
ALTER TABLE spreadsheet_sheets DROP COLUMN IF EXISTS frozen_cols;
ALTER TABLE spreadsheets ADD COLUMN IF NOT EXISTS draft_file_id UUID;

-- Presentations: drop slide content columns, add draft tracking
ALTER TABLE slides DROP COLUMN IF EXISTS elements;
ALTER TABLE slides DROP COLUMN IF EXISTS background;
ALTER TABLE slides DROP COLUMN IF EXISTS notes;
ALTER TABLE slides DROP COLUMN IF EXISTS transition;
ALTER TABLE presentations ADD COLUMN IF NOT EXISTS draft_file_id UUID;

-- Diagrams: drop page data column, add draft tracking
ALTER TABLE diagram_pages DROP COLUMN IF EXISTS data;
ALTER TABLE diagrams ADD COLUMN IF NOT EXISTS draft_file_id UUID;

-- Editing sessions: real-time presence tracking
CREATE TABLE IF NOT EXISTS office.editing_sessions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type  VARCHAR(20) NOT NULL
                     CHECK (entity_type IN ('document','spreadsheet','presentation','diagram')),
    entity_id    UUID NOT NULL,
    user_id      UUID NOT NULL,
    display_name VARCHAR(255),
    color        VARCHAR(7) NOT NULL DEFAULT '#1a73e8',
    started_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_ping_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (entity_type, entity_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_office_editing_sessions_entity
    ON office.editing_sessions(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_office_editing_sessions_user
    ON office.editing_sessions(user_id);
