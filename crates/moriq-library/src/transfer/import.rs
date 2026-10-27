//! Human-readable managed layout; identities and version links remain in SQLite.
use super::*;
impl Library {
    pub fn import_file(
        &mut self,
        file: &ImportFile,
        project: Option<ProjectId>,
        referenced: bool,
    ) -> Result<bool> {
        let project = self.resolve_import_project(file, project)?;
        self.snapshot = None;
        self.import_inner(&file.path, project, false, referenced, Some(&file.relative))
    }
    pub(super) fn resolve_import_project(
        &self,
        file: &ImportFile,
        project: Option<ProjectId>,
    ) -> Result<Option<ProjectId>> {
        let project = if project.is_some() {
            project
        } else if let Some(name) = &file.project_name {
            let existing: Option<String> = self
                .db
                .query_row(
                    "SELECT id FROM projects WHERE name=?1 AND parent_id IS NULL",
                    [name],
                    |r| r.get(0),
                )
                .optional()?;
            Some(if let Some(id) = existing {
                ProjectId(parse_id(&id)?)
            } else {
                self.create_project(name)?
            })
        } else {
            None
        };
        Ok(project)
    }
    pub(super) fn managed_location(
        &mut self,
        source: &Path,
        project: Option<ProjectId>,
        relative: Option<&Path>,
        existing: Option<AssetVersionId>,
    ) -> Result<String> {
        let folder = if let Some(id) = project {
            let found: Option<String> = self
                .db
                .query_row(
                    "SELECT folder FROM project_storage WHERE project_id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(folder) = found {
                folder
            } else {
                let name: String = self.db.query_row(
                    "SELECT name FROM projects WHERE id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?;
                validate_component(&name)?;
                let mut candidate = name.clone();
                let mut sequence = 1u32;
                loop {
                    let used:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM project_storage WHERE folder=?1 COLLATE NOCASE)",[&candidate],|r|r.get(0))?;
                    if !used && !safe_path(&self.root, &format!("assets/{candidate}"))?.exists() {
                        break;
                    }
                    sequence += 1;
                    if sequence > 100000 {
                        return Err(Error::Invalid("项目目录名称冲突过多".into()));
                    }
                    candidate = format!("{name} ({sequence})");
                }
                self.db.execute(
                    "INSERT INTO project_storage VALUES(?1,?2)",
                    params![id.to_string(), candidate],
                )?;
                candidate
            }
        } else {
            "未分类".into()
        };
        let fallback = source
            .parent()
            .and_then(Path::file_name)
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(
                source
                    .file_name()
                    .ok_or_else(|| Error::Invalid("源文件缺少文件名".into()))?,
            );
        let relative = relative.unwrap_or(&fallback);
        let mut parts = Vec::new();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(Error::Invalid("导入的相对目录无效".into()));
            };
            let value = part.to_str().ok_or_else(|| {
                Error::Invalid("该文件名无法无损表示为库内路径，请先重命名源文件".into())
            })?;
            validate_component(value)?;
            parts.push(value.to_string());
        }
        if parts.is_empty() {
            return Err(Error::Invalid("导入文件缺少相对路径".into()));
        }
        // A folder imported as its own project appears once, not Project/Project/photo.jpg.
        let project_name = project
            .map(|id| {
                self.db.query_row(
                    "SELECT name FROM projects WHERE id=?1",
                    [id.to_string()],
                    |r| r.get::<_, String>(0),
                )
            })
            .transpose()?;
        if parts.len() > 1 && project_name.as_deref().is_some_and(|name| name == parts[0]) {
            parts.remove(0);
        }
        let filename = parts
            .pop()
            .ok_or_else(|| Error::Invalid("导入文件名缺失".into()))?;
        let parent = if parts.is_empty() {
            format!("assets/{folder}")
        } else {
            format!("assets/{folder}/{}", parts.join("/"))
        };
        let stem = Path::new(&filename)
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or(&filename);
        let extension = Path::new(&filename).extension().and_then(|v| v.to_str());
        for sequence in 1..=100000 {
            let name = if sequence == 1 {
                filename.clone()
            } else if let Some(ext) = extension {
                format!("{stem} ({sequence}).{ext}")
            } else {
                format!("{stem} ({sequence})")
            };
            let candidate = format!("{parent}/{name}");
            let occupied:Option<String>=self.db.query_row("SELECT id FROM asset_versions WHERE path=?1 COLLATE NOCASE UNION ALL SELECT version_id FROM \
                project_files WHERE path=?1 COLLATE NOCASE LIMIT 1",
[&candidate],
|r|r.get(0)).optional()?;

            if let (Some(id), Some(owned)) = (&occupied, existing)
                && *id == owned.to_string()
            {
                return Ok(candidate);
            }
            if occupied.is_none() && !safe_path(&self.root, &candidate)?.exists() {
                fs::create_dir_all(safe_path(&self.root, &parent)?)?;
                return Ok(candidate);
            }
        }
        Err(Error::Invalid(
            "同名文件过多，请分开导入到不同子目录".into(),
        ))
    }
    pub(super) fn duplicate_project_file(
        &mut self,
        source: &Path,
        staged: &Path,
        project: ProjectId,
        relative: Option<&Path>,
        version: AssetVersionId,
    ) -> Result<()> {
        let path = self.managed_location(source, Some(project), relative, Some(version))?;
        let destination = safe_path(&self.root, &path)?;
        match fs::hard_link(staged, &destination) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if organize::hash_file(&destination)? != organize::hash_file(staged)? {
                    return Err(Error::Invalid("项目同名副本已改变，请重新导入".into()));
                }
            }
            Err(e) => return Err(e.into()),
        }
        self.db.execute(
            "INSERT OR IGNORE INTO project_files VALUES(?1,?2,?3)",
            params![project.to_string(), version.to_string(), path],
        )?;
        Ok(())
    }
}
pub(crate) fn reserved_component(value: &str) -> bool {
    let base = value
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            base.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
}
pub(crate) fn validate_component(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || value.ends_with([' ', '.'])
        || value.chars().any(char::is_control)
        || value.encode_utf16().count() > 255
        || reserved_component(value)
    {
        return Err(Error::Invalid(
            "文件夹或文件名称包含无法安全保存的字符，请先重命名".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn picture(path: &Path, color: u8) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(24, 16, image::Rgb([color, 80, 140]))
            .save(path)
            .unwrap();
    }
    #[test]
    fn folder_import_preserves_project_tree_and_original_names() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("春季拍摄/相机一/照片 001.png");
        picture(&source, 40);
        let original = fs::read(&source).unwrap();
        let mut library = Library::create(dir.path(), "图片库").unwrap();
        library
            .import_file(
                &ImportFile {
                    path: source.clone(),
                    relative: "春季拍摄/相机一/照片 001.png".into(),
                    project_name: Some("春季拍摄".into()),
                },
                None,
                false,
            )
            .unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let target = library.root().join("assets/春季拍摄/相机一/照片 001.png");
        assert_eq!(library.asset_path(asset.id).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), original);
        assert_eq!(fs::read(source).unwrap(), original);
        assert_eq!(asset.name, "照片 001.png");
        assert_eq!(library.info().unwrap().projects[0].name, "春季拍摄");
        let root = library.root().to_path_buf();
        drop(library);
        let library = Library::open(&root).unwrap();
        assert_eq!(library.asset_path(asset.id).unwrap(), target);
    }
    #[test]
    fn name_collision_never_overwrites_and_duplicate_content_is_present_in_both_projects() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("一/照片.png");
        let b = dir.path().join("二/照片.png");
        picture(&a, 10);
        picture(&b, 230);
        let mut library = Library::create(dir.path(), "图片库").unwrap();
        let project = library.create_project("客户项目").unwrap();
        let item = |path: PathBuf| ImportFile {
            path,
            relative: "原文件夹/照片.png".into(),
            project_name: None,
        };
        library
            .import_file(&item(a.clone()), Some(project), false)
            .unwrap();
        library
            .import_file(&item(b.clone()), Some(project), false)
            .unwrap();
        let first = library.root().join("assets/客户项目/原文件夹/照片.png");
        let second = library.root().join("assets/客户项目/原文件夹/照片 (2).png");
        assert_eq!(fs::read(first).unwrap(), fs::read(&a).unwrap());
        assert_eq!(fs::read(second).unwrap(), fs::read(b).unwrap());
        let other = library.create_project("另一个项目").unwrap();
        assert!(
            library
                .import_file(&item(a.clone()), Some(other), false)
                .unwrap()
        );
        assert_eq!(
            fs::read(library.root().join("assets/另一个项目/原文件夹/照片.png")).unwrap(),
            fs::read(a).unwrap()
        );
        assert_eq!(library.info().unwrap().count, 3);
        assert_eq!(
            library
                .page(
                    &Query {
                        project: Some(other),
                        ..Default::default()
                    },
                    0,
                    5
                )
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn unsafe_relative_paths_are_rejected_without_installing_outside_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原图.png");
        picture(&source, 30);
        let mut library = Library::create(dir.path(), "库").unwrap();
        assert!(
            library
                .import_file(
                    &ImportFile {
                        path: source,
                        relative: "../../逃逸.png".into(),
                        project_name: None
                    },
                    None,
                    false
                )
                .is_err()
        );
        assert!(!dir.path().join("逃逸.png").exists());
        assert_eq!(library.info().unwrap().count, 0);
    }
}
