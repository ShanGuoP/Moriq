//! Exercise the desktop's verified model lease, guarded worker and request bridge.
/// 通过正式模型租约和推理桥接执行一次验证，并输出结果摘要。
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let model = args.next().ok_or("usage: infer_model MODEL [IMAGE]")?;
    let mut request = serde_json::json!({});
    if let Some(image) = args.next() {
        request["image"] = image.into();
    }
    let result = moriq_ai::infer(&model, request);
    moriq_ai::unload()?;
    let result = result?;
    println!(
        "{}",
        serde_json::json!({
            "model": model,
            "provider": result["provider"],
            "image_vector_length": result["image_vector"].as_array().map(Vec::len),
            "labels": result["labels"],
            "faces": result["faces"].as_array().map(Vec::len),
            "caption": result["caption"],
        })
    );
    Ok(())
}
