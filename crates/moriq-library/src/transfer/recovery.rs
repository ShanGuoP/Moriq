//! Recovery prepares immutable media on workers and commits only the unchanged journal entry.
use super::*;
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Clone, PartialEq, Eq)]
struct Record {
    operation: String,
    path: String,
    platform: String,
    source: Vec<u8>,
    project: Option<String>,
    referenced: bool,
    relative: Option<String>,
}
fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    Ok(Record {
        operation: row.get(0)?,
        path: row.get(1)?,
        platform: row.get(2)?,
        source: row.get(3)?,
        project: row.get(4)?,
        referenced: row.get(5)?,
        relative: row.get(6)?,
    })
}
const RECORDS: &str = "SELECT j.id,j.path,c.platform,c.source,c.project_id,c.referenced,c.relative_path
    FROM operation_journal j JOIN import_context c ON c.operation_id=j.id WHERE j.phase='interrupted'";

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "recovery_tests.rs"]
mod tests;

pub struct Plan {
    owner: LibraryId,
    root: PathBuf,
    record: Record,
    _lease: Arc<File>,
}
pub struct Work {
    plan: Plan,
    original: PathBuf,
    path: PathBuf,
    _snapshot: tempfile::TempDir,
}
pub struct Ready {
    work: Work,
    media: ingest::Prepared,
}
impl Plan {
    pub fn label(&self) -> String {
        format!("操作 {}", self.record.operation)
    }

    /// Header probes and snapshot links run under worker admission, never on the writer.
    pub fn freeze(self, cancel: Arc<AtomicBool>) -> Result<Work> {
        let _scope = cancel::Scope::enter(cancel);
        cancel::check()?;
        let original = organize::decode_path(&self.record.platform, &self.record.source)?;
        if self.record.referenced {
            organize::reject_network_reference(&original)?;
        }
        let staged = safe_path(
            &self.root,
            &format!("staging/{}.part", self.record.operation),
        )?;
        let owned = if staged.is_file() {
            staged
        } else {
            safe_path(&self.root, &self.record.path)?
        };
        let receipt = fs::read(safe_path(
            &self.root,
            &format!("staging/{}.ready", self.record.operation),
        )?)?;
        let expected = receipt
            .as_slice()
            .try_into()
            .ok()
            .map(u64::from_le_bytes)
            .ok_or_else(|| Error::Invalid("恢复副本未完成，请保留原片并重新导入".into()))?;
        if expected == 0
            || expected > native::file_limit(&original)
            || fs::metadata(&owned)?.len() != expected
        {
            return Err(Error::Invalid(
                "恢复副本不完整，请保留原片并重新导入".into(),
            ));
        }
        // Private same-volume links survive staging cleanup. Backup content excludes backups/.
        let snapshot = tempfile::Builder::new()
            .prefix(".recovery-")
            .tempdir_in(safe_path(&self.root, "backups")?)?;
        let path = snapshot.path().join("input");
        fs::hard_link(owned, &path)?;
        if fs::metadata(&path)?.len() != expected {
            return Err(Error::Invalid("恢复期间副本长度改变".into()));
        }
        cancel::check()?;
        Ok(Work {
            plan: self,
            original,
            path,
            _snapshot: snapshot,
        })
    }
}
impl Work {
    pub fn memory_mib(&self) -> Result<u32> {
        ingest::memory_for(&self.path, &self.original)
    }
    pub fn prepare(self, cancel: Arc<AtomicBool>) -> Result<Ready> {
        let _scope = cancel::Scope::enter(cancel);
        cancel::check()?;
        let mut prepared =
            ingest::prepare(&self.path, &self.plan.record.operation, &self.original)?;
        if self.plan.record.referenced {
            if organize::hash_file(&self.original)? != prepared.hash {
                return Err(Error::Invalid(
                    "引用原片已更改，请手动重新导入，恢复副本保持不变".into(),
                ));
            }
            prepared.reference_verified = true;
        }
        cancel::check()?;
        Ok(Ready {
            work: self,
            media: prepared,
        })
    }
}
impl Library {
    pub fn recovery_plans(&self) -> Result<Vec<Plan>> {
        let records = self
            .db
            .prepare(&format!("{RECORDS} ORDER BY j.id LIMIT 100"))?
            .query_map([], record)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(records
            .into_iter()
            .map(|record| Plan {
                owner: self.id(),
                root: self.root.clone(),
                record,
                _lease: self._lock.clone(),
            })
            .collect())
    }

    /// Only journal validation, final installation and recovery bookkeeping remain here.
    pub fn finish_recovery(&mut self, ready: Ready) -> Result<bool> {
        let Ready {
            work,
            media: prepared,
        } = ready;
        if work.plan.owner != self.id() || work.plan.root != self.root {
            return Err(Error::Invalid("恢复所属资源库已切换".into()));
        }
        let expected = &work.plan.record;
        let current = self
            .db
            .query_row(
                &format!("{RECORDS} AND j.id=?1"),
                [&expected.operation],
                record,
            )
            .optional()?;
        if current.as_ref() != Some(expected) {
            return Err(Error::Invalid("恢复记录已改变，旧结果已停止".into()));
        }
        let project = expected
            .project
            .as_deref()
            .map(|value| parse_id(value).map(ProjectId))
            .transpose()?;
        let imported = self.install_prepared(
            &work.path,
            AssetId::default(),
            AssetVersionId::default(),
            &expected.operation,
            &work.original,
            project,
            false,
            expected.referenced,
            expected.relative.as_deref().map(Path::new),
            prepared,
        )?;
        self.db.execute(
            "UPDATE operation_journal SET phase='committed' WHERE id=?1",
            [&expected.operation],
        )?;
        for suffix in ["part", "thumb", "ready"] {
            let path = safe_path(
                &self.root,
                &format!("staging/{}.{suffix}", expected.operation),
            )?;
            if let Err(error) = fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, "recovery staging cleanup deferred");
            }
        }
        self.forget_import(&expected.operation)?;
        self.snapshot = None;
        Ok(imported)
    }

    /// Synchronous tools adapt the same recovery engine; the app uses worker phases.
    pub fn recover_imports(&mut self) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        for plan in self.recovery_plans()? {
            let label = plan.label();
            let cancel = Arc::new(AtomicBool::new(false));
            let result = (|| {
                let work = plan.freeze(cancel.clone())?;
                self.finish_recovery(work.prepare(cancel)?)
            })();
            match result {
                Ok(true) => report.imported += 1,
                Ok(false) => report.duplicates += 1,
                Err(error) => {
                    report.failed += 1;
                    report.failures.push(format!("{label}：{error}"));
                }
            }
        }
        Ok(report)
    }
}
