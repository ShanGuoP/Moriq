use super::*;
pub const FIELDS: &[&str] = &[
    "make",
    "camera",
    "lens",
    "taken_at",
    "iso",
    "aperture_x100",
    "exposure_us",
    "focal_x100",
    "latitude_e6",
    "longitude_e6",
];
pub fn read(file: File) -> Result<serde_json::Value> {
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024).read_to_end(&mut bytes)?;
    let Ok(data) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) else {
        return Ok(serde_json::json!({}));
    };
    let mut output = serde_json::Map::new();
    for (tag, key) in [
        (exif::Tag::Make, "make"),
        (exif::Tag::Model, "camera"),
        (exif::Tag::LensModel, "lens"),
        (exif::Tag::DateTimeOriginal, "taken_at"),
    ] {
        if let Some(field) = data.get_field(tag, exif::In::PRIMARY) {
            let value = match &field.value {
                exif::Value::Ascii(parts) => parts
                    .iter()
                    .map(|s| {
                        String::from_utf8_lossy(s)
                            .trim_matches('\0')
                            .trim()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => field.display_value().with_unit(&data).to_string(),
            };
            output.insert(key.into(), value.into());
        }
    }
    for (tag, key, scale) in [
        (exif::Tag::PhotographicSensitivity, "iso", 1.0),
        (exif::Tag::FNumber, "aperture_x100", 100.0),
        (exif::Tag::ExposureTime, "exposure_us", 1_000_000.0),
        (exif::Tag::FocalLength, "focal_x100", 100.0),
    ] {
        if let Some(field) = data.get_field(tag, exif::In::PRIMARY) {
            let number = match &field.value {
                exif::Value::Rational(values) => values
                    .first()
                    .filter(|v| v.denom != 0)
                    .map(|v| v.num as f64 / v.denom as f64),
                value => value.get_uint(0).map(f64::from),
            };
            if let Some(number) = number.filter(|n| n.is_finite() && *n >= 0.) {
                output.insert(key.into(), ((number * scale).round() as i64).into());
            }
        }
    }
    if let Some(location) = crate::location::exif_location(&data) {
        output.insert(
            "latitude_e6".into(),
            ((location.latitude * 1_000_000.).round() as i64).into(),
        );
        output.insert(
            "longitude_e6".into(),
            ((location.longitude * 1_000_000.).round() as i64).into(),
        );
    }
    let tags = data
        .fields()
        .take(1024)
        .map(|f| {
            (
                f.tag.to_string(),
                serde_json::Value::String(
                    f.display_value()
                        .with_unit(&data)
                        .to_string()
                        .chars()
                        .take(4096)
                        .collect(),
                ),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    output.insert("tags".into(), tags.into());
    Ok(output.into())
}
pub(crate) fn store(db: &Connection, version: &str, data: &serde_json::Value) -> Result<()> {
    // The version may have been permanently deleted while analysis ran off the writer.
    // Keep the existence check in the same transaction as the result/checkpoint commit.
    db.execute(
        "INSERT INTO version_exif(version_id,data) SELECT id,?2 FROM asset_versions WHERE id=?1 ON \
                CONFLICT(version_id) DO UPDATE SET data=excluded.data",
        params![
            version,
            serde_json::to_string(data).map_err(|e| Error::Invalid(e.to_string()))?
        ],
    )?;

    Ok(())
}
pub(crate) fn compile(
    predicates: &[MetadataPredicate],
    values: &mut Vec<rusqlite::types::Value>,
) -> Result<String> {
    if predicates.len() > 32 {
        return Err(Error::Invalid("最多 32 个元数据条件".into()));
    }
    let mut conditions = Vec::new();
    for predicate in predicates {
        if !FIELDS.contains(&predicate.field.as_str())
            || predicate.contains.len() > 1024
            || predicate.min.zip(predicate.max).is_some_and(|(a, b)| a > b)
        {
            return Err(Error::Invalid("元数据字段、文字或范围无效".into()));
        }
        let field = format!("json_extract(em.data,'$.{}')", predicate.field);
        if !predicate.contains.is_empty() {
            values.push(predicate.contains.clone().into());
            conditions.push(format!("instr(lower({field}),lower(?{}))>0", values.len()));
        }
        let range_field = if predicate.field == "taken_at" {
            format!(
                "CAST(substr(replace(replace(replace(replace(replace({field},':',''),'-',''),'T',''),' ',''),'Z',''),1,14) AS INTEGER)"
            )
        } else {
            field.clone()
        };
        if let Some(min) = predicate.min {
            values.push(min.into());
            conditions.push(format!("{range_field}>=?{}", values.len()));
        }
        if let Some(max) = predicate.max {
            values.push(max.into());
            conditions.push(format!("{range_field}<=?{}", values.len()));
        }
    }
    if conditions.is_empty() {
        return Err(Error::Invalid("元数据检索条件为空".into()));
    }
    Ok(format!(
        "EXISTS(SELECT 1 FROM version_exif em WHERE em.version_id=v.id AND {})",
        conditions.join(" AND ")
    ))
}
