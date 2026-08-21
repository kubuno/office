-- Working calendars: which days a plan actually works.
--
-- Until now a five-day task simply spanned five calendar days, weekends included,
-- so every date past the first week was wrong for anyone working Monday to Friday.
--
-- Kept deliberately generic — a week's shape plus the days that depart from it —
-- so the same table can later describe a resource's availability or an
-- instance-wide business calendar, not just a project's.
CREATE TABLE IF NOT EXISTS pm_calendars (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        VARCHAR(80) NOT NULL DEFAULT 'Calendrier',
    -- One flag per weekday, Monday first: readable in the database, and it maps
    -- straight onto the seven checkboxes the editor shows.
    mon BOOLEAN NOT NULL DEFAULT TRUE,
    tue BOOLEAN NOT NULL DEFAULT TRUE,
    wed BOOLEAN NOT NULL DEFAULT TRUE,
    thu BOOLEAN NOT NULL DEFAULT TRUE,
    fri BOOLEAN NOT NULL DEFAULT TRUE,
    sat BOOLEAN NOT NULL DEFAULT FALSE,
    sun BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Days that depart from the weekly pattern: a public holiday closes an open day,
-- a catch-up Saturday opens a closed one.
CREATE TABLE IF NOT EXISTS pm_calendar_exceptions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    calendar_id UUID NOT NULL REFERENCES pm_calendars(id) ON DELETE CASCADE,
    day         DATE NOT NULL,
    is_working  BOOLEAN NOT NULL DEFAULT FALSE,
    note        VARCHAR(120) NOT NULL DEFAULT '',
    UNIQUE (calendar_id, day)
);

CREATE INDEX IF NOT EXISTS idx_pm_calendars_project ON pm_calendars(project_id);

-- The project's default calendar, and the per-task override for the work that
-- follows a different rhythm (a night shift, a subcontractor's week).
ALTER TABLE projects ADD COLUMN IF NOT EXISTS calendar_id UUID REFERENCES pm_calendars(id) ON DELETE SET NULL;
ALTER TABLE tasks    ADD COLUMN IF NOT EXISTS calendar_id UUID REFERENCES pm_calendars(id) ON DELETE SET NULL;
