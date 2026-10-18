use super::*;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    recent: Vec<Recent>,
    english: bool,
    import: ImportPreferences,
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
pub fn remember(name: String, path: PathBuf) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.recent.retain(|(_, p)| *p != path);
    prefs.recent.insert(0, (name, path));
    prefs.recent.truncate(12);
    save_to(&folder, &prefs)?;
    Ok(prefs)
}
pub fn language(english: bool) -> Result<UserSettings> {
    let folder = location()?;
    let mut prefs = load_from(&folder)?;
    prefs.english = english;
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
    if fs::metadata(&path)?.len() > 128 * 1024 {
        return Err(Error::Invalid("设置文件过大，已停止读取".into()));
    }
    let stored: Stored = toml::from_str(&fs::read_to_string(path)?)
        .map_err(|e| Error::Invalid(format!("设置文件无法解析：{e}")))?;
    let recent = stored
        .recent
        .into_iter()
        .take(12)
        .map(|r| Ok((r.name, organize::decode_path(&r.platform, &r.path)?)))
        .collect::<Result<_>>()?;
    Ok(UserSettings {
        recent,
        english: stored.english,
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
        recent,
        english: prefs.english,
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
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn recent_paths_roundtrip_and_corrupt_settings_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = UserSettings {
            recent: vec![("摄影".into(), dir.path().join("中文 库.moriq"))],
            english: true,
            import: ImportPreferences {
                workers: 12,
                memory_mib: 2048,
            },
        };
        save_to(dir.path(), &prefs).unwrap();
        let loaded = load_from(dir.path()).unwrap();
        assert_eq!(loaded.recent, prefs.recent);
        assert!(loaded.english);
        assert_eq!(loaded.import, prefs.import);
        fs::write(dir.path().join("settings.toml"), "[broken").unwrap();
        assert!(load_from(dir.path()).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.toml")).unwrap(),
            "[broken"
        );
    }
}
