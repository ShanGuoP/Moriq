-- 增加回收站、笔记、评分索引、智能集合及活动记录。
ALTER TABLE assets ADD COLUMN deleted_at INTEGER;
ALTER TABLE assets ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE assets ADD COLUMN edited_at INTEGER NOT NULL DEFAULT 0;
CREATE INDEX assets_deleted ON assets(deleted_at,imported_at);
CREATE INDEX assets_rating ON assets(rating,imported_at);
CREATE TABLE smart_collections(id TEXT PRIMARY KEY,name TEXT NOT NULL UNIQUE,query TEXT NOT NULL);
CREATE TABLE activity(id INTEGER PRIMARY KEY,at INTEGER NOT NULL,kind TEXT NOT NULL,detail TEXT NOT NULL);
PRAGMA user_version=2;
