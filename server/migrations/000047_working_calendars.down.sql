ALTER TABLE tasks    DROP COLUMN IF EXISTS calendar_id;
ALTER TABLE projects DROP COLUMN IF EXISTS calendar_id;
DROP TABLE IF EXISTS pm_calendar_exceptions;
DROP TABLE IF EXISTS pm_calendars;
