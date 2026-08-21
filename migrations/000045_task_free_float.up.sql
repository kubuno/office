-- Free float: how long a task can slip without pushing any of its successors.
-- Distinct from total float, which measures slack against the project end: a task
-- can have plenty of total float and none of its own.
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS free_float INT;
