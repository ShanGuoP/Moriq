-- 记录永久删除任务与受管文件暂存关系，支持故障恢复。
CREATE TABLE purge_jobs(id TEXT PRIMARY KEY,committed INTEGER NOT NULL DEFAULT 0 CHECK(committed IN(0,1)));
CREATE TABLE purge_files(job_id TEXT NOT NULL REFERENCES purge_jobs(id) ON DELETE CASCADE,source TEXT NOT NULL,target TEXT NOT NULL,PRIMARY KEY(job_id,source),UNIQUE(target));
PRAGMA user_version=9;
