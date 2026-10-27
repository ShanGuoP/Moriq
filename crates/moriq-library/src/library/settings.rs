use super::*;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    category_template: Option<AiProfile>,
    synco: SyncoPreferences,
    ai: AiPreferences,
    recent: Vec<Recent>,
    import: ImportPreferences,
    diagnostic_logging: bool,
    reduced_motion: bool,
    language: Language,
    appearance: Appearance,
}
#[derive(Serialize, Deserialize)]
struct Recent {
    name: String,
    platform: String,
    path: Vec<u8>,
}
/// 定位当前用户的应用设置文件，统一配置读取与写入位置。
fn location() -> Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
    Ok(base
        .ok_or_else(|| Error::Invalid("无法定位用户设置目录".into()))?
        .join("MORIQ"))
}
/// 从默认设置位置读取用户配置，沿用缺省值与已有兼容处理。
pub fn load() -> Result<UserSettings> {
    load_from(&location()?)
}
/// 保存 Synco 连接偏好，同时保留其他用户设置。
pub fn synco_preferences(value: SyncoPreferences) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.synco = value;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 更新模型加载和下载来源偏好，不覆盖无关设置。
pub fn ai_preferences(value: AiPreferences) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.ai = value;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 登记或更新最近使用的资源库记录。
pub fn remember(name: String, path: PathBuf) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.recent.retain(|(_, p)| *p != path);
    prefs.recent.insert(0, (name, path));
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 保存可复用的 AI 分类模板，供新资源库或项目使用。
pub fn category_template(mut profile: AiProfile) -> Result<AiProfile> {
    profile.validate().map_err(|e| Error::Invalid(e.into()))?;
    profile.revision = 0;
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.category_template = Some(profile.clone());
    save_to(&folder, &prefs)?;
    Ok(profile)
}
/// 从默认配置中的最近列表移除指定资源库，不删除其文件。
pub fn forget(path: &Path) -> Result<UserSettings> {
    forget_from(&location()?, path)
}
/// 从指定配置位置移除单条资源库记录，供独立配置和测试使用。
fn forget_from(folder: &Path, path: &Path) -> Result<UserSettings> {
    let mut prefs = load_from(folder)?;
    prefs.recent.retain(|(_, candidate)| candidate != path);
    save_to(folder, &prefs)?;
    Ok(prefs)
}
/// 持久化减少动态效果的偏好，并保留其余配置。
pub fn reduced_motion(enabled: bool) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.reduced_motion = enabled;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 保存主题、语言等界面偏好。
pub fn interface(language: Language, appearance: Appearance) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.language = language;
    prefs.appearance = appearance;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 更新诊断日志开关，供后续启动和诊断流程读取。
pub fn diagnostic_logging(enabled: bool) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.diagnostic_logging = enabled;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 保存导入并发和内存偏好，供应用层计算实际执行预算。
pub fn import_preferences(value: ImportPreferences) -> Result<UserSettings> {
    let value = value
        .validated()
        .ok_or_else(|| Error::Invalid("导入并发应为自动或 1～32，内存为 256～4096 MiB".into()))?;
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.import = value;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
/// 读取指定配置文件；损坏配置按既有恢复规则处理。
fn load_from(folder: &Path) -> Result<UserSettings> {
    let path = folder.join("settings.toml");
    if !path.exists() {
        return Ok(UserSettings::default());
    }
    if fs::metadata(&path)?.len() > 2 * 1024 * 1024 {
        return Err(Error::Invalid("设置文件过大，已停止读取".into()));
    }
    let stored: Stored = toml::from_str(&fs::read_to_string(path)?)
        .map_err(|e| Error::Invalid(format!("设置文件无法解析：{e}")))?;
    let recent = stored
        .recent
        .into_iter()
        .map(|r| Ok((r.name, organize::decode_path(&r.platform, &r.path)?)))
        .collect::<Result<_>>()?;
    if let Some(template) = &stored.category_template {
        template.validate().map_err(|e| Error::Invalid(e.into()))?;
    }
    Ok(UserSettings {
        category_template: stored.category_template,
        synco: stored.synco,
        ai: stored.ai,
        recent,
        diagnostic_logging: stored.diagnostic_logging,
        reduced_motion: stored.reduced_motion,
        language: stored.language,
        appearance: stored.appearance,
        import: stored
            .import
            .validated()
            .ok_or_else(|| Error::Invalid("导入设置范围无效".into()))?,
    })
}
/// 将设置写入指定位置，使用临时文件发布以避免半份配置。
fn save_to(folder: &Path, prefs: &UserSettings) -> Result<()> {
    fs::create_dir_all(folder)?;
    let recent = prefs
        .recent
        .iter()
        .map(|(name, path)| {
            let (platform, path) = organize::encode_path(path)?;
            Ok(Recent {
                name: name.clone(),
                platform: platform.into(),
                path,
            })
        })
        .collect::<Result<_>>()?;
    let stored = Stored {
        category_template: prefs.category_template.clone(),
        synco: prefs.synco.clone(),
        ai: prefs.ai,
        recent,
        diagnostic_logging: prefs.diagnostic_logging,
        reduced_motion: prefs.reduced_motion,
        language: prefs.language,
        appearance: prefs.appearance,
        import: prefs.import,
    };
    let serialized = toml::to_string_pretty(&stored).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut temp = tempfile::NamedTempFile::new_in(folder)?;
    temp.write_all(serialized.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(folder.join("settings.toml"))
        .map_err(|e| Error::Io(e.error))?;
    Ok(())
}

/// Read a library card without opening, migrating or taking ownership of that library.
/// Broken/offline libraries remain in the registry and use an empty cover.
///
/// 为资源库卡片解析可用封面来源。
pub fn card_cover(root: &Path) -> Result<Option<Pixels>> {
    let custom = safe_path(root, "metadata/cover.png")?;
    let path = if custom.is_file() {
        custom
    } else {
        let db = Connection::open_with_flags(
            safe_path(root, "catalog.db")?,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(std::time::Duration::from_millis(100))?;
        let version: Option<String> = db.query_row(
            "SELECT display_version_id FROM assets WHERE deleted_at IS NULL ORDER BY imported_at DESC,id LIMIT 1",
            [], |row| row.get(0),
        ).optional()?;
        let Some(version) = version else {
            return Ok(None);
        };
        let version =
            Uuid::parse_str(&version).map_err(|_| Error::Invalid("封面版本无效".into()))?;
        safe_path(root, &format!("thumbnails/256/{version}.png"))?
    };
    if !path.is_file() {
        return Ok(None);
    }
    if fs::metadata(&path)?.len() > 32 * 1024 * 1024 {
        return Err(Error::Invalid("封面超出读取预算".into()));
    }
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1920);
    limits.max_image_height = Some(1920);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    Ok(Some(pixels(resize::fit(&reader.decode()?, 320)?)))
}

/// Read-only card metadata; never opens a library for migration or writes.
///
/// 读取资源库卡片所需的摘要，避免为展示卡片打开完整业务会话。
pub fn card_summary(root: &Path) -> Result<(u64, u64)> {
    let db = Connection::open_with_flags(
        safe_path(root, "catalog.db")?,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    Ok(db.query_row("SELECT (SELECT count(*) FROM assets WHERE deleted_at IS NULL),updated_at FROM library_state WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?)))?)
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证界面设置兼容旧文件并保留其他偏好。
    fn interface_settings_migrate_old_files_and_preserve_other_preferences() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join("settings.toml"),
            "diagnostic_logging = true\nreduced_motion = true\n[import]\nworkers = 3\nmemory_mib = 1024\n").unwrap();
        let mut prefs = load_from(folder.path()).unwrap();
        assert_eq!(prefs.language, Language::SimplifiedChinese);
        assert_eq!(prefs.appearance, Appearance::Light);
        prefs.language = Language::English;
        prefs.appearance = Appearance::Dark;
        prefs
            .recent
            .push(("收藏".into(), folder.path().join("我的素材.moriq")));
        save_to(folder.path(), &prefs).unwrap();
        let mut reopened = load_from(folder.path()).unwrap();
        assert_eq!(reopened.language, Language::English);
        assert_eq!(reopened.appearance, Appearance::Dark);
        assert_eq!(reopened.recent, prefs.recent);
        assert_eq!(reopened.import, prefs.import);
        assert!(reopened.diagnostic_logging);
        assert!(reopened.reduced_motion);
        reopened.ai.use_mirror = false;
        save_to(folder.path(), &reopened).unwrap();
        let final_prefs = load_from(folder.path()).unwrap();
        assert_eq!(final_prefs.language, Language::English);
        assert_eq!(final_prefs.appearance, Appearance::Dark);
    }
    #[test]
    /// 验证最近列表保留较多资源库且允许同名不同路径。
    fn registry_retains_more_than_twelve_libraries_and_same_names() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = UserSettings {
            reduced_motion: true,
            recent: (0..24)
                .map(|index| {
                    (
                        "我的素材库".into(),
                        dir.path().join(format!("{index}.moriq")),
                    )
                })
                .collect(),
            ..Default::default()
        };
        save_to(dir.path(), &prefs).unwrap();
        assert_eq!(load_from(dir.path()).unwrap().recent, prefs.recent);
    }
    #[test]
    /// 验证模型来源默认镜像，保存往返不丢失历史。
    fn model_source_defaults_to_mirror_and_roundtrips_without_losing_history() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(
            folder.path().join("settings.toml"),
            "[ai]\non_demand = false\nretain_indexes = true\n",
        )
        .unwrap();
        let mut prefs = load_from(folder.path()).unwrap();
        assert!(prefs.ai.use_mirror);
        assert!(!prefs.ai.on_demand);
        prefs.ai.use_mirror = false;
        prefs
            .recent
            .push(("测试".into(), folder.path().join("test.moriq")));
        save_to(folder.path(), &prefs).unwrap();
        let loaded = load_from(folder.path()).unwrap();
        assert_eq!(loaded.ai, prefs.ai);
        assert_eq!(loaded.recent, prefs.recent);
    }
    #[test]
    /// 验证资源库打开期间卡片仍能读取实际封面。
    fn library_cards_read_real_covers_while_library_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([20, 80, 140]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "card").unwrap();
        assert!(card_cover(library.root()).unwrap().is_none());
        library.import(&source, None).unwrap();
        let cover = card_cover(library.root()).unwrap().unwrap();
        assert_eq!((cover.width, cover.height), (24, 16));
        library.set_cover(&source).unwrap();
        assert!(card_cover(library.root()).unwrap().is_some());
        assert_eq!(library.info().unwrap().count, 1);
        assert!(!dir.path().join("missing/catalog.db").exists());
        assert!(card_cover(&dir.path().join("missing")).is_err());
        assert!(!dir.path().join("missing/catalog.db").exists());
    }
    #[test]
    /// 验证最近路径往返一致，损坏设置按规则保留以便恢复。
    fn recent_paths_roundtrip_and_corrupt_settings_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = UserSettings {
            synco: Default::default(),
            category_template: None,
            ai: Default::default(),
            recent: vec![("摄影".into(), dir.path().join("中文 库.moriq"))],
            diagnostic_logging: true,
            reduced_motion: true,
            import: ImportPreferences {
                workers: 12,
                memory_mib: 2048,
            },
            language: Language::English,
            appearance: Appearance::Dark,
        };
        save_to(dir.path(), &prefs).unwrap();
        let loaded = load_from(dir.path()).unwrap();
        assert!(loaded.reduced_motion);
        assert_eq!(loaded.recent, prefs.recent);
        assert!(loaded.diagnostic_logging);
        assert_eq!(loaded.import, prefs.import);
        assert_eq!(loaded.language, prefs.language);
        assert_eq!(loaded.appearance, prefs.appearance);
        fs::write(dir.path().join("settings.toml"), "[broken").unwrap();
        assert!(load_from(dir.path()).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.toml")).unwrap(),
            "[broken"
        );
    }
    #[test]
    /// 验证忘记资源库仅移除目标记录，保留文件和其他偏好。
    fn forgetting_only_removes_the_selected_registry_entry_and_preserves_files_and_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.moriq");
        let b = dir.path().join("b.moriq");
        fs::create_dir(&a).unwrap();
        fs::write(a.join("keep"), b"original").unwrap();
        let prefs = UserSettings {
            recent: vec![("A".into(), a.clone()), ("B".into(), b.clone())],
            language: Language::English,
            ..Default::default()
        };
        save_to(dir.path(), &prefs).unwrap();
        let updated = forget_from(dir.path(), &a).unwrap();
        assert_eq!(updated.recent, vec![("B".into(), b)]);
        assert_eq!(load_from(dir.path()).unwrap().language, Language::English);
        assert_eq!(fs::read(a.join("keep")).unwrap(), b"original");
    }
    #[test]
    /// 验证分类模板保存与合并保留本地已有分类关联。
    fn category_template_roundtrips_and_can_merge_without_removing_local_associations() {
        let dir = tempfile::tempdir().unwrap();
        let mut profile = AiProfile::default();
        profile.categories.push(AiCategory {
            id: "theme_reusable".into(),
            kind: AiCategoryKind::Theme,
            label: "可复用主题".into(),
            definition: "主题说明".into(),
            enabled: true,
        });
        let prefs = UserSettings {
            category_template: Some(profile.clone()),
            ..Default::default()
        };
        save_to(dir.path(), &prefs).unwrap();
        let loaded = load_from(dir.path()).unwrap().category_template.unwrap();
        assert_eq!(profile, loaded);
        let mut existing = AiProfile {
            revision: 7,
            ..AiProfile::default()
        };
        existing.categories.push(AiCategory {
            id: "theme_local".into(),
            kind: AiCategoryKind::Theme,
            label: "原有主题".into(),
            definition: "原有说明".into(),
            enabled: true,
        });
        let merged = existing.with_template(&loaded).unwrap();
        assert_eq!(merged.revision, 7);
        assert!(
            merged
                .categories
                .iter()
                .any(|c| c.id == "theme_reusable" && c.enabled)
        );
        assert!(
            merged
                .categories
                .iter()
                .any(|c| c.id == "theme_local" && !c.enabled)
        );
    }
}
