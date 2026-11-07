use std::{env, path::PathBuf};
/// 准备 AI 构建所需的运行组件清单和原生辅助程序，并登记重建依赖。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ai-runtime");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    prepare_faces(&runtime, &output)?;
    let manifest = runtime.join("checksums.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-env=MORIQ_DEV_AI_ROOT={}", runtime.display());
    if manifest.is_file() {
        std::fs::copy(&manifest, output.join("ai-checksums.json"))?;
    } else if env::var("PROFILE").as_deref() == Ok("release") {
        return Err("Run scripts/prepare-ai-runtime.ps1 before a release build".into());
    } else {
        std::fs::write(output.join("ai-checksums.json"), "{}")?;
    }
    let llama = runtime.join("llama");
    let llama_manifest = llama.join("checksums.json");
    println!("cargo:rerun-if-changed={}", llama_manifest.display());
    if llama_manifest.is_file() {
        std::fs::copy(&llama_manifest, output.join("llama-checksums.json"))?;
    } else if env::var("PROFILE").as_deref() == Ok("release") {
        return Err("Run scripts/prepare-ai-runtime.ps1 before a release build".into());
    } else {
        std::fs::write(output.join("llama-checksums.json"), "{}")?;
    }
    let llama_runtime =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ai-runtime/llama");
    println!(
        "cargo:rustc-env=MORIQ_DEV_LLAMA_ROOT={}",
        llama_runtime.display()
    );
    println!("cargo:rerun-if-changed=../moriq-library/native/codec-runner.cpp");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    let mut cmd = cc::Build::new().cpp(true).get_compiler().to_command();
    cmd.args([
        "/nologo",
        "/EHsc",
        "/std:c++17",
        "/O2",
        "/MT",
        "../moriq-library/native/codec-runner.cpp",
    ])
    .arg(format!("/Fo{}", output.join("ai-guard.obj").display()))
    .arg(format!("/Fe{}", output.join("ai-guard.exe").display()));
    if !cmd.status()?.success() {
        return Err("AI process guard compilation failed".into());
    }
    Ok(())
}

/// 编译独立 CPU 人脸进程，把运行组件摘要嵌入桌面程序。
fn prepare_faces(
    runtime: &std::path::Path,
    output: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let faces = runtime.join("faces");
    println!("cargo:rerun-if-changed=native/face-worker.cpp");
    println!(
        "cargo:rerun-if-changed={}",
        faces.join("provenance.json").display()
    );
    println!("cargo:rustc-env=MORIQ_DEV_FACE_ROOT={}", faces.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && faces.join("sface.onnx").is_file()
    {
        let sdk = root.join("opencv-sdk/opencv/build");
        let mut command = cc::Build::new().cpp(true).get_compiler().to_command();
        command
            .args([
                "/nologo",
                "/EHsc",
                "/std:c++17",
                "/O2",
                "/MD",
                "/utf-8",
                "/Brepro",
            ])
            .arg(format!("/I{}", sdk.join("include").display()))
            .arg("native/face-worker.cpp")
            .arg(format!("/Fo{}", output.join("face-worker.obj").display()))
            .arg(format!("/Fe{}", output.join("face-worker.exe").display()))
            .arg("/link")
            .arg("/Brepro")
            .arg(format!("/LIBPATH:{}", sdk.join("x64/vc16/lib").display()))
            .arg("opencv_world4120.lib");
        if !command.status()?.success() {
            return Err("CPU face worker compilation failed".into());
        }
        std::fs::copy(
            output.join("face-worker.exe"),
            faces.join("face-worker.exe"),
        )?;
        write_manifest(&faces)?;
        std::fs::copy(
            faces.join("checksums.json"),
            output.join("face-checksums.json"),
        )?;
        write_manifest(runtime)?;
    } else {
        if env::var("PROFILE").as_deref() == Ok("release")
            && env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        {
            return Err("Run scripts/prepare-face-runtime.ps1 before packaging".into());
        }
        std::fs::write(output.join("face-checksums.json"), "{}")?;
    }
    Ok(())
}
fn write_manifest(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    fn walk(
        root: &std::path::Path,
        folder: &std::path::Path,
        entries: &mut std::collections::BTreeMap<String, String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(folder)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(root, &path, entries)?;
            } else if path != root.join("checksums.json") {
                let relative = path
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .to_string_lossy()
                    .replace('\\', "/");
                entries.insert(
                    relative,
                    format!("{:x}", Sha256::digest(std::fs::read(path)?)),
                );
            }
        }
        Ok(())
    }
    let mut entries = std::collections::BTreeMap::new();
    walk(root, root, &mut entries)?;
    let bytes = serde_json::to_vec_pretty(&entries)?;
    let path = root.join("checksums.json");
    if std::fs::read(&path).ok().as_ref() != Some(&bytes) {
        std::fs::write(path, bytes)?;
    }
    Ok(())
}
