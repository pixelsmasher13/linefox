-- SQLite does not support DROP COLUMN in older versions; recreate the table without the new columns
CREATE TABLE automation_schedules_backup AS SELECT id, automation_id, recurrence_type, recurrence_days, execution_hour, execution_minute, timezone, is_active, next_run_at, last_run_at, created_at, updated_at FROM automation_schedules;
DROP TABLE automation_schedules;
ALTER TABLE automation_schedules_backup RENAME TO automation_schedules;
