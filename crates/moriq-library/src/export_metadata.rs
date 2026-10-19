use super::*;
pub(crate) fn normalized_exif(path: &Path, width: u32, height: u32) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(32 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let data = match exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) {
        Ok(data) => data,
        Err(exif::Error::NotFound(_) | exif::Error::InvalidFormat(_)) => return Ok(None),
        Err(e) => return Err(Error::Invalid(format!("无法保留源 EXIF：{e}"))),
    };
    let mut fields = data
        .fields()
        .filter(|f| {
            f.ifd_num == exif::In::PRIMARY
                && !matches!(
                    f.tag,
                    exif::Tag::MakerNote
                        | exif::Tag::Orientation
                        | exif::Tag::ImageWidth
                        | exif::Tag::ImageLength
                        | exif::Tag::PixelXDimension
                        | exif::Tag::PixelYDimension
                        | exif::Tag::ColorSpace
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    for (tag, value) in [
        (exif::Tag::Orientation, exif::Value::Short(vec![1])),
        (exif::Tag::ColorSpace, exif::Value::Short(vec![1])),
        (exif::Tag::PixelXDimension, exif::Value::Long(vec![width])),
        (exif::Tag::PixelYDimension, exif::Value::Long(vec![height])),
    ] {
        fields.push(exif::Field {
            tag,
            ifd_num: exif::In::PRIMARY,
            value,
        });
    }
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut output = std::io::Cursor::new(Vec::new());
    writer
        .write(&mut output, false)
        .map_err(|e| Error::Invalid(format!("EXIF 重建失败：{e}")))?;
    let bytes = output.into_inner();
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::Invalid("EXIF 超出 4 MiB 导出预算".into()));
    }
    Ok(Some(bytes))
}
pub(crate) fn attach<E: image::ImageEncoder>(encoder: &mut E, exif: Option<&[u8]>) -> Result<()> {
    let profile = moxcms::ColorProfile::new_srgb()
        .encode()
        .map_err(|e| Error::Color(e.to_string()))?;
    encoder
        .set_icc_profile(profile)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    if let Some(exif) = exif {
        encoder
            .set_exif_metadata(exif.to_vec())
            .map_err(|e| Error::Invalid(e.to_string()))?;
    }
    Ok(())
}
fn component(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|{}".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let value = value.trim().trim_end_matches('.');
    let value = if value.is_empty() { "未分组" } else { value };
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&value.to_uppercase().as_str())
    {
        format!("{value}_")
    } else {
        value.into()
    }
}
pub(crate) fn directory(
    template: &str,
    root: &Path,
    format: &str,
    imported_at: u64,
    metadata: &serde_json::Value,
) -> Result<PathBuf> {
    if template.len() > 1024 {
        return Err(Error::Invalid("导出目录模板过长".into()));
    }
    let template = template.replace('\\', "/");
    let parts = template.split('/').collect::<Vec<_>>();
    if parts.is_empty()
        || parts.len() > 8
        || parts
            .iter()
            .any(|s| s.is_empty() || matches!(*s, "." | ".."))
    {
        return Err(Error::Invalid("目录模板需要 1～8 层相对目录".into()));
    }
    let date = chrono::DateTime::from_timestamp_millis(imported_at as i64)
        .ok_or_else(|| Error::Invalid("导入日期无效".into()))?;
    let replacements = [
        ("{date}", date.format("%Y-%m-%d").to_string()),
        ("{year}", date.format("%Y").to_string()),
        ("{month}", date.format("%m").to_string()),
        ("{day}", date.format("%d").to_string()),
        ("{format}", component(format)),
        (
            "{camera}",
            component(metadata["exif"]["camera"].as_str().unwrap_or("未知相机")),
        ),
        (
            "{project}",
            component(metadata["project"].as_str().unwrap_or("未分组")),
        ),
    ];
    let mut resolved = Vec::new();
    for part in parts {
        let mut value = part.to_string();
        for (token, replacement) in &replacements {
            value = value.replace(token, replacement);
        }
        if value.contains(['{', '}']) {
            return Err(Error::Invalid("目录模板含未知变量".into()));
        }
        resolved.push(component(&value));
    }
    safe_path(root, &resolved.join("/"))
}
