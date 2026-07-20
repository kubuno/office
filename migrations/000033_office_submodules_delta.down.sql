DROP TRIGGER IF EXISTS trg_wb_boards_tombstone ON office_wb.boards;
DROP FUNCTION IF EXISTS office_wb.board_tombstone();
DROP TABLE IF EXISTS office_wb.board_tombstones;
DROP TRIGGER IF EXISTS trg_wb_boards_change_seq ON office_wb.boards;
DROP FUNCTION IF EXISTS office_wb.bump_board_change_seq();
ALTER TABLE office_wb.boards DROP COLUMN IF EXISTS change_seq;
DROP SEQUENCE IF EXISTS office_wb.board_change_seq;

DROP TRIGGER IF EXISTS trg_dpages_bump_parent ON diagram_pages;
DROP FUNCTION IF EXISTS office_dpage_bump_parent();
DROP TRIGGER IF EXISTS trg_diagrams_tombstone ON diagrams;
DROP FUNCTION IF EXISTS office_diag_tombstone();
DROP TABLE IF EXISTS diagram_tombstones;
DROP TRIGGER IF EXISTS trg_diagrams_change_seq ON diagrams;
DROP FUNCTION IF EXISTS office_bump_diag_change_seq();
ALTER TABLE diagrams DROP COLUMN IF EXISTS change_seq;
DROP SEQUENCE IF EXISTS diagram_change_seq;

DROP TRIGGER IF EXISTS trg_presentations_tombstone ON presentations;
DROP FUNCTION IF EXISTS office_pres_tombstone();
DROP TABLE IF EXISTS presentation_tombstones;
DROP TRIGGER IF EXISTS trg_presentations_change_seq ON presentations;
DROP FUNCTION IF EXISTS office_bump_pres_change_seq();
ALTER TABLE presentations DROP COLUMN IF EXISTS change_seq;
DROP SEQUENCE IF EXISTS presentation_change_seq;

DROP TRIGGER IF EXISTS trg_sheets_bump_parent ON spreadsheet_sheets;
DROP FUNCTION IF EXISTS office_sheet_bump_parent();
DROP TRIGGER IF EXISTS trg_spreadsheets_tombstone ON spreadsheets;
DROP FUNCTION IF EXISTS office_ss_tombstone();
DROP TABLE IF EXISTS spreadsheet_tombstones;
DROP TRIGGER IF EXISTS trg_spreadsheets_change_seq ON spreadsheets;
DROP FUNCTION IF EXISTS office_bump_ss_change_seq();
ALTER TABLE spreadsheets DROP COLUMN IF EXISTS change_seq;
DROP SEQUENCE IF EXISTS spreadsheet_change_seq;
