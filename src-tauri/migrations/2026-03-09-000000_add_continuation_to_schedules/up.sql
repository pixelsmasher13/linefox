-- Add continuation/persistent-run columns to automation_schedules
ALTER TABLE automation_schedules ADD COLUMN persistent_run_id INTEGER REFERENCES automations(id) ON DELETE SET NULL;
ALTER TABLE automation_schedules ADD COLUMN continuation_prompt TEXT;
