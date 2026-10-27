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
        if confidence < 0.8 {
            continue;
        }
        result.push([
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        ]);
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
            "找出图片中清晰可见的人类面部。只返回面部，不包含头发、身体、动物或图案。box 使用归一化到0至1000的整数坐标，按 [左上x,左上y,右下x,右下y]。没有清晰人脸则 faces 为空，不猜测被遮挡或不在画面中的人脸。",
            detection_schema(),
        )?;
        let faces = boxes(&detected)?;
        let mut output = Vec::new();
        let mut assigned = std::collections::HashSet::new();
        for bounds in faces {
            if cancel.load(Ordering::Acquire) {
                return Err("AI 任务已停止".into());
            }
            let target = crop(&image, bounds);
            let mut matches = std::collections::HashSet::new();
            let mut comparisons = Vec::new();
            for (person, path) in references {
                let mut sheet =
                    image::RgbaImage::from_pixel(544, 272, image::Rgba([245, 245, 245, 255]));
                let reference = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
                let reference = image::imageops::resize(
                    &reference,
                    256,
                    256,
                    image::imageops::FilterType::Triangle,
                );
                image::imageops::overlay(&mut sheet, &target, 8, 8);
                image::imageops::overlay(&mut sheet, &reference, 280, 8);
                let sheet_path = directory.path().join("references.png");
                sheet
                    .save_with_format(&sheet_path, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                let schema = json!({"type":"object","properties":{
                    "same_person":{"type":"boolean"},"confidence":{"type":"number","minimum":0,"maximum":1},
                    "evidence":{"type":"string","maxLength":160}},
                    "required":["evidence","same_person","confidence"],"additionalProperties":false});
                let prompt = "这是一张对比图，左、右各是一张独立的人脸照片。请比较左右面部的眼睛、鼻子、嘴、下颌等稳定结构，判断是否为同一个人。先在evidence简短说明一致或不一致的面部证据，再给出same_person和confidence。两个不同的人即使衣服、发型或背景相似也必须为false；只有面部身份高度确定一致才为true。图片太小、模糊、遮挡或无法确认时为false。不要猜测姓名。";
                let result = request(&[&sheet_path], prompt, schema)?;
                let same = result["same_person"]
                    .as_bool()
                    .ok_or("Qwen 人脸匹配结果无效")?;
                let confidence = result["confidence"]
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                    .ok_or("Qwen 人脸匹配置信度无效")?;
                comparisons.push(json!({"candidate":person,"result":result}));
                if same && confidence >= 0.95 {
                    matches.insert(person.clone());
                }
            }
            let person = if matches.len() == 1 {
                matches
                    .into_iter()
                    .next()
                    .filter(|id| assigned.insert(id.clone()))
            } else {
                None
            };
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
}
