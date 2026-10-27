use super::*;
use std::sync::{Arc, Mutex};
pub(crate) struct ReadPool {
    root: PathBuf,
    _lease: Arc<File>,
    idle: Mutex<Vec<Connection>>,
}
pub(crate) struct ReadLease {
    db: Option<Connection>,
    pool: Arc<ReadPool>,
}
impl ReadPool {
    /// 借用读取连接并冻结版本来源，供媒体线程脱离写入端读取。
    pub fn media_source(
        self: &Arc<Self>,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<media::Source> {
        let lease = self.checkout()?;
        media::Source::from_read_connection(
            lease.connection()?,
            &self.root,
            self._lease.clone(),
            asset,
            version,
        )
    }
    /// 建立共享读取连接池，并持有资源库租约以保护读取期间的库生命周期。
    pub fn new(root: PathBuf, lease: Arc<File>) -> Arc<Self> {
        Arc::new(Self {
            root,
            _lease: lease,
            idle: Mutex::new(Vec::new()),
        })
    }
    /// 获取可用读取连接；必要时建立新连接并校验资源库身份。
    pub fn checkout(self: &Arc<Self>) -> Result<ReadLease> {
        let cached = self
            .idle
            .lock()
            .map_err(|_| Error::Invalid("读取连接池锁失效".into()))?
            .pop();
        let db = match cached {
            Some(db) => db,
            None => {
                let db = Connection::open_with_flags(
                    safe_path(&self.root, "catalog.db")?,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                db.busy_timeout(std::time::Duration::from_secs(5))?;
                db.execute_batch("PRAGMA cache_size=-8192; PRAGMA temp_store=FILE; PRAGMA temp.cache_size=-2048;")?;
                db
            }
        };
        Ok(ReadLease {
            db: Some(db),
            pool: self.clone(),
        })
    }
}
impl ReadLease {
    /// 借用当前读取租约中的数据库连接，连接已释放时返回错误。
    pub fn connection(&self) -> Result<&Connection> {
        self.db
            .as_ref()
            .ok_or_else(|| Error::Invalid("读取连接已归还".into()))
    }
}
impl Drop for ReadLease {
    /// 回滚遗留事务并清理临时表，再把可复用读取连接归还连接池。
    fn drop(&mut self) {
        let Some(db) = self.db.take() else { return };
        if !db.is_autocommit() && db.execute_batch("ROLLBACK").is_err() {
            return;
        }
        let tables = (|| -> rusqlite::Result<Vec<String>> {
            db.prepare("SELECT name FROM sqlite_temp_master WHERE type='table'")?
                .query_map([], |r| r.get(0))?
                .collect()
        })();
        let Ok(tables) = tables else { return };
        for name in tables {
            let quoted = name.replace('"', "\"\"");
            if db
                .execute_batch(&format!("DROP TABLE \"{quoted}\";"))
                .is_err()
            {
                return;
            }
        }
        if let Ok(mut idle) = self.pool.idle.lock()
            && idle.len() < 4
        {
            idle.push(db)
        }
    }
}
