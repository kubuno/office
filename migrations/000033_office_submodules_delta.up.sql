-- Delta primitives for the office sub-modules pull sync (spreadsheets, presentations,
-- diagrams, whiteboard boards) — mirror of 000032 (documents): one monotonic
-- change_seq per entity + tombstones, maintained by triggers.
--
-- Child rows (sheets, pages) bump their PARENT's change_seq: the sync unit is the
-- parent entity + its content file. Slides need no child trigger — the existing
-- slide_count trigger (000007) already updates the presentations row on every
-- slide INSERT/UPDATE/DELETE, which fires the parent change_seq trigger.

-- ── Spreadsheets ────────────────────────────────────────────────────────────────
CREATE SEQUENCE IF NOT EXISTS spreadsheet_change_seq;
ALTER TABLE spreadsheets ADD COLUMN IF NOT EXISTS change_seq BIGINT NOT NULL DEFAULT nextval('spreadsheet_change_seq');
CREATE INDEX IF NOT EXISTS idx_office_ss_change_seq ON spreadsheets(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_bump_ss_change_seq() RETURNS trigger AS $$
BEGIN
    NEW.change_seq := nextval('spreadsheet_change_seq');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_spreadsheets_change_seq ON spreadsheets;
CREATE TRIGGER trg_spreadsheets_change_seq BEFORE UPDATE ON spreadsheets
    FOR EACH ROW EXECUTE FUNCTION office_bump_ss_change_seq();

CREATE TABLE IF NOT EXISTS spreadsheet_tombstones (
    id         UUID        PRIMARY KEY,
    owner_id   UUID        NOT NULL,
    change_seq BIGINT      NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_office_ss_tomb_change_seq ON spreadsheet_tombstones(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_ss_tombstone() RETURNS trigger AS $$
BEGIN
    INSERT INTO spreadsheet_tombstones (id, owner_id, change_seq)
    VALUES (OLD.id, OLD.owner_id, nextval('spreadsheet_change_seq'))
    ON CONFLICT (id) DO UPDATE SET change_seq = EXCLUDED.change_seq, deleted_at = NOW();
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_spreadsheets_tombstone ON spreadsheets;
CREATE TRIGGER trg_spreadsheets_tombstone AFTER DELETE ON spreadsheets
    FOR EACH ROW EXECUTE FUNCTION office_ss_tombstone();

-- Sheet metadata changes count as a parent change (the no-op UPDATE fires the
-- parent BEFORE UPDATE trigger, which assigns the new seq).
CREATE OR REPLACE FUNCTION office_sheet_bump_parent() RETURNS trigger AS $$
BEGIN
    UPDATE spreadsheets SET change_seq = change_seq
     WHERE id = COALESCE(NEW.spreadsheet_id, OLD.spreadsheet_id);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sheets_bump_parent ON spreadsheet_sheets;
CREATE TRIGGER trg_sheets_bump_parent AFTER INSERT OR UPDATE OR DELETE ON spreadsheet_sheets
    FOR EACH ROW EXECUTE FUNCTION office_sheet_bump_parent();

-- ── Presentations ───────────────────────────────────────────────────────────────
CREATE SEQUENCE IF NOT EXISTS presentation_change_seq;
ALTER TABLE presentations ADD COLUMN IF NOT EXISTS change_seq BIGINT NOT NULL DEFAULT nextval('presentation_change_seq');
CREATE INDEX IF NOT EXISTS idx_office_pres_change_seq ON presentations(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_bump_pres_change_seq() RETURNS trigger AS $$
BEGIN
    NEW.change_seq := nextval('presentation_change_seq');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_presentations_change_seq ON presentations;
CREATE TRIGGER trg_presentations_change_seq BEFORE UPDATE ON presentations
    FOR EACH ROW EXECUTE FUNCTION office_bump_pres_change_seq();

CREATE TABLE IF NOT EXISTS presentation_tombstones (
    id         UUID        PRIMARY KEY,
    owner_id   UUID        NOT NULL,
    change_seq BIGINT      NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_office_pres_tomb_change_seq ON presentation_tombstones(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_pres_tombstone() RETURNS trigger AS $$
BEGIN
    INSERT INTO presentation_tombstones (id, owner_id, change_seq)
    VALUES (OLD.id, OLD.owner_id, nextval('presentation_change_seq'))
    ON CONFLICT (id) DO UPDATE SET change_seq = EXCLUDED.change_seq, deleted_at = NOW();
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_presentations_tombstone ON presentations;
CREATE TRIGGER trg_presentations_tombstone AFTER DELETE ON presentations
    FOR EACH ROW EXECUTE FUNCTION office_pres_tombstone();

-- ── Diagrams ────────────────────────────────────────────────────────────────────
CREATE SEQUENCE IF NOT EXISTS diagram_change_seq;
ALTER TABLE diagrams ADD COLUMN IF NOT EXISTS change_seq BIGINT NOT NULL DEFAULT nextval('diagram_change_seq');
CREATE INDEX IF NOT EXISTS idx_office_diag_change_seq ON diagrams(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_bump_diag_change_seq() RETURNS trigger AS $$
BEGIN
    NEW.change_seq := nextval('diagram_change_seq');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_diagrams_change_seq ON diagrams;
CREATE TRIGGER trg_diagrams_change_seq BEFORE UPDATE ON diagrams
    FOR EACH ROW EXECUTE FUNCTION office_bump_diag_change_seq();

CREATE TABLE IF NOT EXISTS diagram_tombstones (
    id         UUID        PRIMARY KEY,
    owner_id   UUID        NOT NULL,
    change_seq BIGINT      NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_office_diag_tomb_change_seq ON diagram_tombstones(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_diag_tombstone() RETURNS trigger AS $$
BEGIN
    INSERT INTO diagram_tombstones (id, owner_id, change_seq)
    VALUES (OLD.id, OLD.owner_id, nextval('diagram_change_seq'))
    ON CONFLICT (id) DO UPDATE SET change_seq = EXCLUDED.change_seq, deleted_at = NOW();
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_diagrams_tombstone ON diagrams;
CREATE TRIGGER trg_diagrams_tombstone AFTER DELETE ON diagrams
    FOR EACH ROW EXECUTE FUNCTION office_diag_tombstone();

CREATE OR REPLACE FUNCTION office_dpage_bump_parent() RETURNS trigger AS $$
BEGIN
    UPDATE diagrams SET change_seq = change_seq
     WHERE id = COALESCE(NEW.diagram_id, OLD.diagram_id);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_dpages_bump_parent ON diagram_pages;
CREATE TRIGGER trg_dpages_bump_parent AFTER INSERT OR UPDATE OR DELETE ON diagram_pages
    FOR EACH ROW EXECUTE FUNCTION office_dpage_bump_parent();

-- ── Whiteboard boards (schema office_wb) ────────────────────────────────────────
CREATE SEQUENCE IF NOT EXISTS office_wb.board_change_seq;
ALTER TABLE office_wb.boards ADD COLUMN IF NOT EXISTS change_seq BIGINT NOT NULL DEFAULT nextval('office_wb.board_change_seq');
CREATE INDEX IF NOT EXISTS idx_wb_boards_change_seq ON office_wb.boards(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_wb.bump_board_change_seq() RETURNS trigger AS $$
BEGIN
    NEW.change_seq := nextval('office_wb.board_change_seq');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_wb_boards_change_seq ON office_wb.boards;
CREATE TRIGGER trg_wb_boards_change_seq BEFORE UPDATE ON office_wb.boards
    FOR EACH ROW EXECUTE FUNCTION office_wb.bump_board_change_seq();

CREATE TABLE IF NOT EXISTS office_wb.board_tombstones (
    id         UUID        PRIMARY KEY,
    owner_id   UUID        NOT NULL,
    change_seq BIGINT      NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_wb_tomb_change_seq ON office_wb.board_tombstones(owner_id, change_seq);

CREATE OR REPLACE FUNCTION office_wb.board_tombstone() RETURNS trigger AS $$
BEGIN
    INSERT INTO office_wb.board_tombstones (id, owner_id, change_seq)
    VALUES (OLD.id, OLD.owner_id, nextval('office_wb.board_change_seq'))
    ON CONFLICT (id) DO UPDATE SET change_seq = EXCLUDED.change_seq, deleted_at = NOW();
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_wb_boards_tombstone ON office_wb.boards;
CREATE TRIGGER trg_wb_boards_tombstone AFTER DELETE ON office_wb.boards
    FOR EACH ROW EXECUTE FUNCTION office_wb.board_tombstone();
