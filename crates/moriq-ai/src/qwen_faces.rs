//! Detect faces and compare real portrait crops with Qwen's vision model.
use crate::Result;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// 构造人脸检测的结构化输出约束，限制坐标与结果格式。
fn detection_schema() -> Value {
    json!({"type":"object","properties":{"faces":{"type":"array","maxItems":32,"items":{
        "type":"object","properties":{
          "box":{"type":"array","items":{"type":"integer","minimum":0,"maximum":1000},"minItems":4,"maxItems":4},
          "confidence":{"type":"number","minimum":0,"maximum":1}},
        "required":["box","confidence"],"additionalProperties":false}}},"required":["faces"],"additionalProperties":false})
}
/// 校验并规范化模型返回的人脸框，剔除无效或不确定的检测。
fn boxes(value: &Value) -> Result<Vec<[f32; 4]>> {
    let faces = value["faces"].as_array().ok_or("Qwen 人脸回执缺少 faces")?;
    if faces.len() > 32 {
        return Err("Qwen 人脸数量超出预算".into());
    }
    let mut result = Vec::new();
    for face in faces {
        let coords = face["box"]
            .as_array()
            .filter(|v| v.len() == 4)
            .ok_or("Qwen 人脸坐标无效")?;
        let mut bounds = [0.0_f32; 4];
        for (index, value) in coords.iter().enumerate() {
            bounds[index] = value
                .as_u64()
                .filter(|n| *n <= 1000)
                .ok_or("Qwen 人脸坐标超出范围")? as f32
                / 1000.0;
        }
        let confidence = face["confidence"]
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .ok_or("Qwen 人脸置信度无效")?;
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
            return Err("Qwen 人脸边界无效".into());
        }
        if confidence < 0.9 {
            continue;
        }
        let candidate = [
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        ];
        if result.iter().any(|old: &[f32; 4]| {
            let intersection = ((old[0] + old[2]).min(candidate[0] + candidate[2])
                - old[0].max(candidate[0]))
            .max(0.0)
                * ((old[1] + old[3]).min(candidate[1] + candidate[3]) - old[1].max(candidate[1]))
                    .max(0.0);
            intersection
                / (old[2] * old[3] + candidate[2] * candidate[3] - intersection).max(f32::EPSILON)
                > 0.45
        }) {
            continue;
        }
        result.push(candidate);
    }
    Ok(result)
}
/// 按有效人脸框生成供身份比较使用的有界裁剪图。
fn crop(image: &image::RgbaImage, bounds: [f32; 4]) -> image::RgbaImage {
    let width = image.width() as f32;
    let height = image.height() as f32;
    let left = bounds[0] * width;
    let top = bounds[1] * height;
    let right = (bounds[0] + bounds[2]).min(1.0) * width;
    let bottom = (bounds[1] + bounds[3]).min(1.0) * height;
    let edge = (((right - left).max(bottom - top) * 1.22).ceil() as u32)
        .clamp(1, image.width().min(image.height()));
    let x = (((left + right - edge as f32) / 2.0).max(0.0) as u32).min(image.width() - edge);
    let y = (((top + bottom - edge as f32) / 2.0).max(0.0) as u32).min(image.height() - edge);
    let face = image::imageops::crop_imm(image, x, y, edge, edge).to_image();
    image::imageops::resize(&face, 256, 256, image::imageops::FilterType::Triangle)
}
/// 在原始预览上检查面积与清晰度，避免放大后把背景噪声当作细节。
fn usable_face(image: &image::RgbaImage, bounds: [f32; 4]) -> bool {
    let width = (bounds[2] * image.width() as f32) as u32;
    let height = (bounds[3] * image.height() as f32) as u32;
    if width.min(height) < 64 || bounds[2] * bounds[3] < 0.004 {
        return false;
    }
    let x = (bounds[0] * image.width() as f32) as u32;
    let y = (bounds[1] * image.height() as f32) as u32;
    let width = width.min(image.width().saturating_sub(x));
    let height = height.min(image.height().saturating_sub(y));
    if width == 0 || height == 0 {
        return false;
    }
    let region = image::imageops::crop_imm(image, x, y, width, height).to_image();
    let gray = image::DynamicImage::ImageRgba8(image::imageops::resize(
        &region,
        96,
        96,
        image::imageops::FilterType::Triangle,
    ))
    .to_luma8();
    let mut sum = 0.0;
    let mut square = 0.0;
    let mut count = 0.0;
    for y in 1..95 {
        for x in 1..95 {
            let lap = 4.0 * gray.get_pixel(x, y)[0] as f64
                - gray.get_pixel(x - 1, y)[0] as f64
                - gray.get_pixel(x + 1, y)[0] as f64
                - gray.get_pixel(x, y - 1)[0] as f64
                - gray.get_pixel(x, y + 1)[0] as f64;
            sum += lap;
            square += lap * lap;
            count += 1.0;
        }
    }
    square / count - (sum / count).powi(2) >= 35.0
}

/// 每批比较四个人物；跨批次有多个匹配时拒绝猜测身份。
fn match_person(
    target: &Path,
    references: &[(String, image::RgbaImage)],
    directory: &Path,
    cancel: &AtomicBool,
    request: &mut impl FnMut(&[&Path], &str, Value) -> Result<Value>,
) -> Result<(Option<String>, Vec<Value>)> {
    let mut matches = Vec::new();
    let mut comparisons = Vec::new();
    for chunk in references.chunks(4) {
        if cancel.load(Ordering::Acquire) {
            return Err("AI 任务已停止".into());
        }
        let mut sheet = image::RgbaImage::from_pixel(512, 512, image::Rgba([245, 245, 245, 255]));
        for (index, (_, portrait)) in chunk.iter().enumerate() {
            image::imageops::overlay(
                &mut sheet,
                portrait,
                (index % 2 * 256) as i64,
                (index / 2 * 256) as i64,
            );
        }
        let sheet_path = directory.join("candidates.png");
        sheet.save(&sheet_path).map_err(|e| e.to_string())?;
        let schema = json!({"type":"object","properties":{
            "candidate":{"type":"integer","minimum":0,"maximum":chunk.len()},
            "confidence":{"type":"number","minimum":0,"maximum":1}},
            "required":["candidate","confidence"],"additionalProperties":false});
        let prompt = format!(
            "第一张图为目标人脸，第二张图为参考人物网格：左上=1，右上=2，左下=3，右下=4；仅前{}格有效。比较眼鼻嘴和下颌等稳定面部结构，选择唯一确定的同一人物编号。无匹配、有歧义、模糊或遮挡时 candidate=0。不得根据衣服发型背景猜测。只返回编号和置信度。",
            chunk.len()
        );
        let result = request(&[target, &sheet_path], &prompt, schema)?;
        let candidate = result["candidate"]
            .as_u64()
            .filter(|v| *v <= chunk.len() as u64)
            .ok_or("Qwen 人脸匹配编号无效")?;
        let confidence = result["confidence"]
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or("Qwen 人脸匹配置信度无效")?;
        comparisons.push(
            json!({"candidates":chunk.iter().map(|p| &p.0).collect::<Vec<_>>(),"result":result}),
        );
        if candidate > 0 && confidence >= 0.95 {
            matches.push(chunk[candidate as usize - 1].0.clone());
        }
    }
    Ok((
        if matches.len() == 1 {
            matches.pop()
        } else {
            None
        },
        comparisons,
    ))
}
/// 先检测人脸，再与提供的人物参考比较；全程沿用取消控制。
pub fn infer(
    image_path: &Path,
    references: &[(String, PathBuf)],
    cancel: &AtomicBool,
) -> Result<Value> {
    let reader = image::ImageReader::open(image_path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut reader = reader;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1600);
    limits.max_image_height = Some(1600);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    crate::llama::vision_session(cancel, |request| {
        let detected = request(
            &[image_path],
            "仅找出前景主体中清晰、足够大的真实人类面部。忽略背景路人、小脸、失焦模糊脸、海报、屏幕或玩偶的脸。只返回面部，不包含头发、身体、动物或图案。box 使用归一化到0至1000的整数坐标，按 [左上x,左上y,右下x,右下y]。没有清晰人脸则 faces 为空，不猜测被遮挡或不在画面中的人脸。",
            detection_schema(),
        )?;
        let faces: Vec<_> = boxes(&detected)?
            .into_iter()
            .filter(|bounds| usable_face(&image, *bounds))
            .collect();
        let mut portraits = Vec::new();
        if !faces.is_empty() {
            for (person, path) in references {
                if cancel.load(Ordering::Acquire) {
                    return Err("AI 任务已停止".into());
                }
                let reference = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
                if !usable_face(&reference, [0.0, 0.0, 1.0, 1.0]) {
                    continue;
                }
                portraits.push((
                    person.clone(),
                    image::imageops::resize(
                        &reference,
                        256,
                        256,
                        image::imageops::FilterType::Triangle,
                    ),
                ));
            }
        }
        let mut output = Vec::new();
        let mut assigned = std::collections::HashSet::new();
        for bounds in faces {
            if cancel.load(Ordering::Acquire) {
                return Err("AI 任务已停止".into());
            }
            let target_path = directory.path().join("target.png");
            crop(&image, bounds)
                .save(&target_path)
                .map_err(|e| e.to_string())?;
            let (person, comparisons) = match_person(
                &target_path,
                &portraits,
                directory.path(),
                cancel,
                &mut |images, prompt, schema| request(images, prompt, schema),
            )?;
            let person = person.filter(|id| assigned.insert(id.clone()));
            output.push(json!({"box":bounds,"person":person,"comparisons":comparisons}));
        }
        Ok(json!({"faces":output}))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证人脸坐标规范化，以及无效或不确定检测的剔除。
    fn detector_coordinates_are_normalized_and_invalid_or_uncertain_faces_are_rejected() {
        let faces = boxes(&json!({"faces":[{"box":[100,200,400,600],"confidence":0.95}]})).unwrap();
        for (value, expected) in faces[0].iter().zip([0.1, 0.2, 0.3, 0.4]) {
            assert!((value - expected).abs() < 1e-5);
        }
        assert!(boxes(&json!({"faces":[{"box":[100,200,40,600],"confidence":1.0}]})).is_err());
        assert!(boxes(&json!({"faces":[{"box":[0,0,1001,1000],"confidence":1.0}]})).is_err());
        assert!(
            boxes(&json!({"faces":[{"box":[100,200,400,600],"confidence":0.5}]}))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn quality_gate_rejects_small_and_blurred_faces_before_matching() {
        let sharp = image::RgbaImage::from_fn(512, 512, |x, y| {
            let v = if (x / 8 + y / 8) % 2 == 0 { 30 } else { 220 };
            image::Rgba([v, v, v, 255])
        });
        assert!(usable_face(&sharp, [0.1, 0.1, 0.6, 0.6]));
        assert!(!usable_face(&sharp, [0.1, 0.1, 0.06, 0.06]));
        assert!(!usable_face(
            &image::imageops::blur(&sharp, 12.0),
            [0.1, 0.1, 0.6, 0.6]
        ));
        assert!(!usable_face(
            &image::RgbaImage::from_pixel(512, 512, image::Rgba([100, 100, 100, 255])),
            [0.1, 0.1, 0.6, 0.6]
        ));
        assert_eq!(boxes(&json!({"faces":[{"box":[100,100,400,400],"confidence":0.99},{"box":[105,105,405,405],"confidence":0.98}]})).unwrap().len(),1);
    }

    #[test]
    fn comparisons_are_batched_and_ambiguous_matches_are_not_assigned() {
        let dir = tempfile::tempdir().unwrap();
        let references: Vec<_> = (0..9)
            .map(|i| (format!("p{i}"), image::RgbaImage::new(256, 256)))
            .collect();
        let mut calls = 0;
        let result = match_person(
            Path::new("target.png"),
            &references,
            dir.path(),
            &AtomicBool::new(false),
            &mut |images, _, _| {
                assert_eq!(images.len(), 2);
                calls += 1;
                Ok(json!({"candidate":if calls == 1 {1} else {0},"confidence":0.99}))
            },
        )
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(result.0.as_deref(), Some("p0"));
        let ambiguous = match_person(
            Path::new("target.png"),
            &references,
            dir.path(),
            &AtomicBool::new(false),
            &mut |_, _, _| Ok(json!({"candidate":1,"confidence":0.99})),
        )
        .unwrap();
        assert!(ambiguous.0.is_none());
        assert!(
            match_person(
                Path::new("target.png"),
                &references,
                dir.path(),
                &AtomicBool::new(true),
                &mut |_, _, _| panic!("cancelled requests must not run")
            )
            .is_err()
        );
        assert!(
            match_person(
                Path::new("target.png"),
                &references,
                dir.path(),
                &AtomicBool::new(false),
                &mut |_, _, _| Ok(json!({"candidate":5,"confidence":0.99}))
            )
            .is_err()
        );
    }
}
