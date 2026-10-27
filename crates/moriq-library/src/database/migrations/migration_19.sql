ALTER TABLE synco_tasks ADD COLUMN copy_removed INTEGER NOT NULL DEFAULT 0;
CREATE INDEX synco_copy_cleanup ON synco_tasks(copy_removed,state);
PRAGMA user_version=19;
