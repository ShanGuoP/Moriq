-- 保存后台任务的逐项失败原因，供查看和重试。
CREATE TABLE background_job_errors(job_id TEXT NOT NULL REFERENCES background_jobs(id) ON DELETE CASCADE,seq INTEGER NOT NULL,version_id TEXT NOT NULL,message TEXT NOT NULL,PRIMARY KEY(job_id,seq));
PRAGMA user_version=22;
