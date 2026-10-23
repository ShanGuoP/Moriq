use super::*;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
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
pub fn load() -> Result<UserSettings> {
    load_from(&location()?)
}
pub fn synco_preferences(value: SyncoPreferences) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.synco = value;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
pub fn ai_preferences(value: AiPreferences) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.ai = value;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
pub fn remember(name: String, path: PathBuf) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.recent.retain(|(_, p)| *p != path);
    prefs.recent.insert(0, (name, path));
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
pub fn reduced_motion(enabled: bool) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.reduced_motion = enabled;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
pub fn interface(language: Language, appearance: Appearance) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.language = language;
    prefs.appearance = appearance;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
pub fn diagnostic_logging(enabled: bool) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.diagnostic_logging = enabled;
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
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
    Ok(UserSettings {
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
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
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
    fn recent_paths_roundtrip_and_corrupt_settings_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = UserSettings {
            synco: Default::default(),
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
}
