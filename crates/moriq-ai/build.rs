use std::{env, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
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
