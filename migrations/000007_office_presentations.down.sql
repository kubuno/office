-- 000007_office_presentations.down.sql

DROP TABLE IF EXISTS presentation_shares;
DROP TABLE IF EXISTS slides;
DROP TABLE IF EXISTS presentations;

DROP FUNCTION IF EXISTS office_update_slide_count();
