ALTER TABLE tasks DROP CONSTRAINT IF EXISTS tasks_constraint_type_check;
ALTER TABLE tasks
    DROP COLUMN IF EXISTS constraint_type,
    DROP COLUMN IF EXISTS constraint_date,
    DROP COLUMN IF EXISTS deadline_date,
    DROP COLUMN IF EXISTS deadline_missed;
