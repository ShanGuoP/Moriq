-- 保存后台任务附加参数，供任务恢复时重建执行输入。
CREATE TABLE background_job_payload(job_id TEXT PRIMARY KEY REFERENCES background_jobs(id) ON DELETE CASCADE,json TEXT NOT NULL);
PRAGMA user_version=18;
