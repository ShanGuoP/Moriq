use std::{env, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ai-runtime");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
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
