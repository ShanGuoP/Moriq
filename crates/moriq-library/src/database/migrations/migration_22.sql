CREATE TABLE background_job_errors(job_id TEXT NOT NULL REFERENCES background_jobs(id) ON DELETE CASCADE,seq INTEGER NOT NULL,version_id TEXT NOT NULL,message TEXT NOT NULL,PRIMARY KEY(job_id,seq));
PRAGMA user_version=22;
