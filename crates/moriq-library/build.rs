use std::{env, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native/wic-worker.cpp");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    let compiler = cc::Build::new().cpp(true).get_compiler();
    let mut cmd = compiler.to_command();
    cmd.arg("/nologo")
        .arg("/EHsc")
        .arg("/std:c++17")
        .arg("/O2")
        .arg("/MT")
        .arg("/DUNICODE")
        .arg("/D_UNICODE")
        .arg("native/wic-worker.cpp")
        .arg(format!("/Fo{}", output.join("wic-worker.obj").display()))
        .arg(format!("/Fe{}", output.join("wic-worker.exe").display()))
        .arg("/link")
        .arg("windowscodecs.lib")
        .arg("ole32.lib")
        .arg("oleaut32.lib")
        .arg("uuid.lib");
    if !cmd.status()?.success() {
        return Err("WIC worker build failed".into());
    }
    Ok(())
}
