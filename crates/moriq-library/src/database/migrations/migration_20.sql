-- 登记孤立文件可恢复隔离操作及其原位置。
CREATE TABLE orphan_quarantine(id TEXT PRIMARY KEY,original TEXT NOT NULL,target TEXT NOT NULL UNIQUE,bytes INTEGER NOT NULL,state TEXT NOT NULL CHECK(state IN('prepared','quarantined','restoring','purging')),created_at INTEGER NOT NULL);
CREATE INDEX orphan_quarantine_state ON orphan_quarantine(state,id);
PRAGMA user_version=20;
