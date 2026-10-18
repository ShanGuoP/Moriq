use super::*;
use serde_json::{Value, json};
type SavedReceipt = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);
impl Library {
    pub fn synco_pending(&self) -> Result<Vec<SyncoTask>> {
        self.db.prepare("SELECT t.id,a.name,t.state,t.error,t.request,t.ack FROM synco_tasks t JOIN assets a ON a.id=t.asset_id WHERE t.state IN ('queued','importing','editing','rendering','ready') OR t.ack IS NOT NULL ORDER BY t.created_at LIMIT 50")?.query_map([],|r|Ok(SyncoTask{id:r.get(0)?,name:r.get(1)?,state:r.get(2)?,error:r.get(3)?,request:r.get(4)?,ack:r.get(5)?}))?.collect::<std::result::Result<_,_>>().map_err(Error::from)
    }
    pub fn versions(&self, id: AssetId) -> Result<Vec<AssetVersion>> {
        let rows=self.db.prepare("SELECT id,origin,parent_version_id FROM asset_versions WHERE asset_id=?1 ORDER BY rowid")?
            .query_map([id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        rows.into_iter()
            .map(|(id, origin, parent)| {
                Ok(AssetVersion {
                    id: AssetVersionId(parse_id(&id)?),
                    origin,
                    parent: parent
                        .map(|p| parse_id(&p).map(AssetVersionId))
                        .transpose()?,
                })
            })
            .collect()
    }
    pub fn display_version(&mut self, asset: AssetId, version: AssetVersionId) -> Result<()> {
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM asset_versions WHERE id=?1 AND asset_id=?2)",
            params![version.to_string(), asset.to_string()],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::Invalid("版本不属于此素材，已停止切换".into()));
        }
        self.db.execute(
            "UPDATE assets SET display_version_id=?1 WHERE id=?2",
            params![version.to_string(), asset.to_string()],
        )?;
        self.snapshot = None;
        Ok(())
    }
    pub fn synco_tasks(&self) -> Result<Vec<SyncoTask>> {
        self.db.prepare("SELECT t.id,a.name,t.state,t.error,t.request,t.ack FROM synco_tasks t JOIN assets a ON a.id=t.asset_id ORDER BY t.created_at DESC LIMIT 100")?
            .query_map([],|r|Ok(SyncoTask{id:r.get(0)?,name:r.get(1)?,state:r.get(2)?,error:r.get(3)?,request:r.get(4)?,ack:r.get(5)?}))?.collect::<std::result::Result<_,_>>().map_err(Error::from)
    }
    pub fn prepare_synco(&mut self, ids: &[AssetId], project: Option<ProjectId>) -> Result<()> {
        if ids.is_empty() || ids.len() > 100 {
            return Err(Error::Invalid("每批请选择 1～100 张素材发送 Synco".into()));
        }
        for id in ids {
            let path = self.asset_path(*id)?;
            let (version,format,hash):(String,String,String)=self.db.query_row("SELECT v.id,v.format,v.sha256 FROM asset_versions v JOIN assets a ON a.display_version_id=v.id WHERE a.id=?1 AND a.deleted_at IS NULL",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            if !["jpg", "png", "webp", "bmp"].contains(&format.as_str()) {
                return Err(Error::Invalid(
                    "Synco v1 输入限 JPEG、PNG、WebP 和 BMP".into(),
                ));
            }
            let task = Uuid::new_v4().to_string();
            let relative = format!("interop/outgoing/{task}.{format}");
            fs::create_dir_all(safe_path(&self.root, "interop/outgoing")?)?;
            let dest = safe_path(&self.root, &relative)?;
            organize::copy_owned(&path, &dest)?;
            if organize::hash_file(&dest)? != hash {
                return Err(Error::Invalid("发送期间源文件发生变化，交接已停止".into()));
            }
            let project_name = if let Some(project) = project {
                self.db.query_row(
                    "SELECT name FROM projects WHERE id=?1",
                    [project.to_string()],
                    |r| r.get::<_, String>(0),
                )?
            } else {
                self.manifest.name.clone()
            };
            let spec = json!({"task_id":task,"client_id":format!("moriq-{}",self.id()),"source_project_id":format!("{}:{}",self.id(),project.map(|p|p.to_string()).unwrap_or_else(||"library".into())),"project_name":project_name.chars().take(60).collect::<String>(),"source_image_id":format!("{}:{}",self.id(),id),"input":{"path":dest.to_str().ok_or_else(||Error::Invalid("Synco 协议不能表示此文件路径".into()))?,"sha256":hash},"output":{"format":"jpeg","quality":95},"open_in_editor":true});
            self.db.execute("INSERT INTO synco_tasks(id,asset_id,sent_version_id,request,state,created_at) VALUES(?1,?2,?3,?4,'queued',?5)",params![task,id.to_string(),version,spec.to_string(),now_ms()?])?;
        }
        Ok(())
    }
    pub fn synco_error(&self, task: &str, error: &str) -> Result<bool> {
        let updated = self.db.execute(
            "UPDATE synco_tasks SET error=?1 WHERE id=?2",
            params![error, task],
        )?;
        Ok(updated > 0)
    }
    pub fn synco_acked(&self, task: &str) -> Result<()> {
        self.db.execute("UPDATE synco_tasks SET ack=NULL,state=CASE WHEN state='ready' THEN 'delivered' ELSE state END,error='' WHERE id=?1",[task])?;
        Ok(())
    }
    /// Persist receipt and immutable version before the coordinator is allowed to ACK.
    pub fn receive_synco(&mut self, task: &str, response: &Value) -> Result<Option<Value>> {
        let row:Option<SavedReceipt>=self.db.query_row("SELECT asset_id,sent_version_id,request,result_id,returned_version_id,ack FROM synco_tasks WHERE id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        // Deletion may finish while a status request is in flight. Never resurrect it.
        let Some((asset, parent, request, old_result, old_version, old_ack)) = row else {
            return Ok(None);
        };
        let spec: Value =
            serde_json::from_str(&request).map_err(|e| Error::Invalid(e.to_string()))?;
        for field in [
            "task_id",
            "client_id",
            "source_project_id",
            "source_image_id",
        ] {
            if response[field] != spec[field] {
                return Err(Error::Invalid(format!("Synco 回执 {field} 身份不匹配")));
            }
        }
        let state = response["state"]
            .as_str()
            .ok_or_else(|| Error::Invalid("Synco 缺少任务状态".into()))?;
        if ![
            "importing",
            "editing",
            "rendering",
            "ready",
            "delivered",
            "failed",
            "cancelled",
        ]
        .contains(&state)
        {
            return Err(Error::Invalid("未知的 Synco 任务状态".into()));
        }
        if state == "delivered" && old_version.is_none() {
            return Err(Error::Invalid(
                "Synco 已确认交付，但当前库缺少回传版本。请打开接收结果的原库或从完整备份恢复"
                    .into(),
            ));
        }
        if state == "ready" {
            let result = response["result_id"]
                .as_str()
                .filter(|r| !r.is_empty() && r.len() <= 256)
                .ok_or_else(|| Error::Invalid("Synco 缺少结果 ID".into()))?;
            if old_version.is_some() {
                if old_result.as_deref() != Some(result) {
                    return Err(Error::Invalid("同一 Synco 任务返回了冲突结果".into()));
                }
                return old_ack
                    .map(|ack| {
                        serde_json::from_str(&ack).map_err(|e| Error::Invalid(e.to_string()))
                    })
                    .transpose();
            }
            let output = &response["output"];
            let format = output["format"].as_str().unwrap_or("");
            if !["jpeg", "jpg", "png"].contains(&format) {
                return Err(Error::Invalid("Synco 回传格式应为 JPEG 或 PNG".into()));
            }
            let hash = output["sha256"]
                .as_str()
                .filter(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
                .ok_or_else(|| Error::Invalid("回传校验值无效".into()))?;
            let source = PathBuf::from(
                output["path"]
                    .as_str()
                    .ok_or_else(|| Error::Invalid("Synco 缺少成图路径".into()))?,
            );
            let bytes = output["bytes"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("Synco 缺少成图大小".into()))?;
            if !source.is_absolute()
                || bytes == 0
                || bytes > MAX_BYTES
                || fs::metadata(&source)?.len() != bytes
            {
                return Err(Error::Invalid("Synco 成图路径或大小无效".into()));
            }
            let version = AssetVersionId::default();
            let extension = if format == "png" { "png" } else { "jpg" };
            let staged = safe_path(&self.root, &format!("staging/{version}.synco"))?;
            organize::copy_owned(&source, &staged)?;
            if organize::hash_file(&staged)? != hash {
                return Err(Error::Invalid("Synco 成图校验失败，尚未确认接收".into()));
            }
            let reader = ImageReader::open(&staged)?.with_guessed_format()?;
            if reader.format()
                != Some(if extension == "png" {
                    ImageFormat::Png
                } else {
                    ImageFormat::Jpeg
                })
            {
                return Err(Error::Invalid("Synco 成图魔数与声明不一致".into()));
            }
            let (width, height) = reader.into_dimensions()?;
            if width == 0
                || height == 0
                || Some(u64::from(width)) != output["width"].as_u64()
                || Some(u64::from(height)) != output["height"].as_u64()
            {
                return Err(Error::Invalid("Synco 成图尺寸超出限制或声明不符".into()));
            }
            let mut reader = ImageReader::open(&staged)?.with_guessed_format()?;
            reader.limits(decode_limits());
            let mut decoder = reader.into_decoder()?;
            let orientation = decoder.orientation()?;
            let mut decoded = image::DynamicImage::from_decoder(decoder)?;
            decoded.apply_orientation(orientation);
            let relative = format!("versions/{version}.{extension}");
            fs::create_dir_all(safe_path(&self.root, "versions")?)?;
            fs::hard_link(&staged, safe_path(&self.root, &relative)?)?;
            let thumb = safe_path(&self.root, &format!("thumbnails/256/{version}.png"))?;
            let mut temp = tempfile::NamedTempFile::new_in(safe_path(&self.root, "staging")?)?;
            let thumbnail_image = resize::fit(&decoded, 256)?;
            thumbnail_image.write_to(&mut temp, ImageFormat::Png)?;
            temp.as_file().sync_all()?;
            temp.persist_noclobber(thumb)
                .map_err(|e| Error::Io(e.error))?;
            let ack = json!({"client_id":spec["client_id"],"result_id":result,"sha256":hash});
            let tx = self.db.transaction()?;
            tx.execute("INSERT INTO asset_versions(id,asset_id,parent_version_id,origin,path,sha256,format,width,height,bytes) VALUES(?1,?2,?3,'synco',?4,?5,?6,?7,?8,?9)",params![version.to_string(),asset,parent,relative,hash,extension,width,height,bytes])?;
            tx.execute(
                "UPDATE asset_versions SET dominant_color=?1 WHERE id=?2",
                params![
                    organize::dominant_color(&thumbnail_image),
                    version.to_string()
                ],
            )?;
            tx.execute("UPDATE synco_tasks SET state='ready',result_id=?1,returned_version_id=?2,ack=?3,error='' WHERE id=?4",params![result,version.to_string(),ack.to_string(),task])?;
            tx.commit()?;
            self.snapshot = None;
            if let Err(e) = fs::remove_file(staged) {
                tracing::warn!(%e,"received Synco staging cleanup deferred");
            }
            return Ok(Some(ack));
        }
        let ack = if ["failed", "cancelled"].contains(&state) {
            Some(json!({"client_id":spec["client_id"]}))
        } else {
            None
        };
        self.db.execute(
            "UPDATE synco_tasks SET state=?1,error=?2,ack=?3 WHERE id=?4",
            params![
                state,
                response["last_error"].to_string(),
                ack.as_ref().map(Value::to_string),
                task
            ],
        )?;
        Ok(ack)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Library, AssetId, SyncoTask, Value) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        image::RgbImage::from_pixel(32, 24, image::Rgb([20, 80, 130]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "交接库").unwrap();
        library.import(&source, None).unwrap();
        let asset = library.page(&Query::default(), 0, 1).unwrap().assets[0].id;
        library.prepare_synco(&[asset], None).unwrap();
        let task = library.synco_pending().unwrap().remove(0);
        let request: Value = serde_json::from_str(&task.request).unwrap();
        (dir, library, asset, task, request)
    }
    fn ready(dir: &Path, request: &Value) -> Value {
        let source = dir.join("Synco结果.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([220, 150, 80]))
            .save(&source)
            .unwrap();
        let mut result = request.clone();
        result["state"] = json!("ready");
        result["result_id"] = json!("result-fixture");
        result["output"] = json!({"format":"png","path":source,"sha256":organize::hash_file(&source).unwrap(),"bytes":fs::metadata(source).unwrap().len(),"width":24,"height":16});
        result
    }
    #[test]
    fn receipt_reopen_and_ack_retry_never_overwrite_or_duplicate_a_version() {
        let (dir, mut library, asset, task, request) = fixture();
        let original = library.asset_path(asset).unwrap();
        let bytes = fs::read(&original).unwrap();
        let first = library.versions(asset).unwrap()[0].id;
        let response = ready(dir.path(), &request);
        let ack = library.receive_synco(&task.id, &response).unwrap().unwrap();
        assert_eq!(ack["result_id"], "result-fixture");
        assert_eq!(library.versions(asset).unwrap().len(), 2);
        assert_eq!(library.info().unwrap().count, 1);
        assert_eq!(library.asset_path(asset).unwrap(), original);
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let root = library.root().to_path_buf();
        drop(library);
        let mut library = Library::open(&root).unwrap();
        assert_eq!(
            library.receive_synco(&task.id, &response).unwrap().unwrap(),
            ack
        );
        assert_eq!(library.versions(asset).unwrap().len(), 2);
        library.synco_acked(&task.id).unwrap();
        assert!(library.synco_pending().unwrap().is_empty());
        assert!(
            library
                .receive_synco(&task.id, &response)
                .unwrap()
                .is_none()
        );
        assert_eq!(library.versions(asset).unwrap().len(), 2);
        let versions = library.versions(asset).unwrap();
        assert_eq!(versions[1].parent, Some(first));
        library.display_version(asset, versions[1].id).unwrap();
        assert_ne!(library.asset_path(asset).unwrap(), original);
        library.display_version(asset, first).unwrap();
        assert_eq!(fs::read(library.asset_path(asset).unwrap()).unwrap(), bytes);
        let mut conflicting = response;
        conflicting["result_id"] = json!("different-result");
        assert!(library.receive_synco(&task.id, &conflicting).is_err());
    }
    #[test]
    fn wrong_identity_hash_dimensions_and_failure_never_create_version_or_ack() {
        let (dir, mut library, asset, task, request) = fixture();
        let response = ready(dir.path(), &request);
        for (key, value) in [
            ("source_image_id", json!("wrong")),
            ("client_id", json!("another-client")),
        ] {
            let mut wrong = response.clone();
            wrong[key] = value;
            assert!(library.receive_synco(&task.id, &wrong).is_err());
        }
        let mut wrong = response.clone();
        wrong["output"]["sha256"] = json!("0".repeat(64));
        assert!(library.receive_synco(&task.id, &wrong).is_err());
        let mut wrong = response.clone();
        wrong["output"]["width"] = json!(123);
        assert!(library.receive_synco(&task.id, &wrong).is_err());
        assert_eq!(library.versions(asset).unwrap().len(), 1);
        assert!(library.synco_pending().unwrap()[0].ack.is_none());
        let mut failed = request;
        failed["state"] = json!("failed");
        let ack = library.receive_synco(&task.id, &failed).unwrap().unwrap();
        assert!(ack.get("result_id").is_none());
        assert_eq!(library.versions(asset).unwrap().len(), 1);
        library.synco_acked(&task.id).unwrap();
        assert!(library.synco_pending().unwrap().is_empty());
    }
}
