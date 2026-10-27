CREATE TABLE import_context(operation_id TEXT PRIMARY KEY REFERENCES operation_journal(id),platform TEXT NOT NULL,source BLOB NOT NULL,project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,referenced INTEGER NOT NULL CHECK(referenced IN (0,1)));
PRAGMA user_version=4;
