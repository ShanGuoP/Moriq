use std::{env, path::PathBuf};
/// 构建当前平台的媒体工作程序，登记原生源码与运行组件的重建依赖。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let codec_runtime =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/codecs/runtime");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    let checksum = codec_runtime.join("checksums.json");
    println!("cargo:rerun-if-changed={}", checksum.display());
    println!(
        "cargo:rustc-env=MORIQ_DEV_CODECS_ROOT={}",
        codec_runtime.display()
    );
    if checksum.is_file() {
        std::fs::copy(&checksum, output.join("codecs-checksums.json"))?;
    } else {
        std::fs::write(output.join("codecs-checksums.json"), "{}")?;
    }
    println!("cargo:rerun-if-changed=native/wic-worker.cpp");
    println!("cargo:rerun-if-changed=native/raw-worker.cpp");
    println!("cargo:rerun-if-changed=native/raw-budget.h");
    println!("cargo:rerun-if-changed=native/video-worker.cpp");
    println!("cargo:rerun-if-changed=native/codec-runner.cpp");
    println!("cargo:rerun-if-changed=native/raw-worker-portable.cpp");
    println!("cargo:rustc-check-cfg=cfg(bundled_video)");
    println!("cargo:rustc-check-cfg=cfg(bundled_libraw)");
    println!("cargo:rerun-if-env-changed=MORIQ_LIBRAW_DIR");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
            let source = env::var_os("MORIQ_LIBRAW_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../target/codecs/libraw-macos")
                });
            if source.join("lib/libraw.a").is_file() {
                let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?)
                    .join("raw-runtime");
                std::fs::create_dir_all(&output)?;
                let mut command = cc::Build::new().cpp(true).get_compiler().to_command();
                command
                    .args(["-std=c++17", "-O2", "-pthread"])
                    .arg(format!("-I{}", source.join("include").display()))
                    .arg("native/raw-worker-portable.cpp")
                    .arg(source.join("lib/libraw.a"))
                    .arg("-lz")
                    .arg("-o")
                    .arg(output.join("raw-worker"));
                if !command.status()?.success() {
                    return Err("Portable RAW worker compilation failed".into());
                }
                println!("cargo:rustc-cfg=bundled_libraw");
            } else {
                println!("cargo:warning=run scripts/prepare-raw-macos.sh to bundle LibRaw");
            }
        }
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
    let mut cmd = compiler.to_command();
    cmd.args(["/nologo", "/EHsc", "/std:c++17", "/O2", "/MT"])
        .arg("native/codec-runner.cpp")
        .arg(format!("/Fo{}", output.join("codec-runner.obj").display()))
        .arg(format!("/Fe{}", output.join("codec-runner.exe").display()));
    if !cmd.status()?.success() {
        return Err("Codec resource guard build failed".into());
    }
    let source = env::var_os("MORIQ_LIBRAW_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/codecs/libraw/LibRaw-0.22.2")
        });
    if source.join("lib/libraw.lib").is_file() {
        let runtime = output.join("raw-runtime");
        std::fs::create_dir_all(&runtime)?;
        let mut cmd = compiler.to_command();
        cmd.args([
            "/nologo",
            "/EHsc",
            "/std:c++17",
            "/O2",
            "/MT",
            "/DUNICODE",
            "/D_UNICODE",
        ])
        .arg(format!("/I{}", source.display()))
        .arg("native/raw-worker.cpp")
        .arg(format!("/Fo{}", output.join("raw-worker.obj").display()))
        .arg(format!("/Fe{}", runtime.join("raw-worker.exe").display()))
        .arg("/link")
        .arg(source.join("lib/libraw.lib"))
        .args(["windowscodecs.lib", "ole32.lib", "uuid.lib"]);
        if !cmd.status()?.success() {
            return Err("LibRaw worker build failed".into());
        }
        std::fs::copy(source.join("bin/libraw.dll"), runtime.join("libraw.dll"))?;
        println!("cargo:rustc-cfg=bundled_libraw");
    } else {
        if env::var("PROFILE").as_deref() == Ok("release") {
            return Err("Release requires bundled LibRaw; run scripts/prepare-codecs.ps1".into());
        }
        println!(
            "cargo:warning=LibRaw missing: run scripts/prepare-codecs.ps1 before a release build"
        );
    }
    let ffmpeg =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/codecs/ffmpeg-selected");
    if ffmpeg.join("lib/avcodec.lib").is_file() {
        let mut cmd = compiler.to_command();
        cmd.args([
            "/nologo",
            "/EHsc",
            "/std:c++17",
            "/utf-8",
            "/O2",
            "/MT",
            "/DUNICODE",
            "/D_UNICODE",
        ])
        .arg(format!("/I{}", ffmpeg.join("include").display()))
        .arg("native/video-worker.cpp")
        .arg(format!("/Fo{}", output.join("video-worker.obj").display()))
        .arg(format!("/Fe{}", output.join("video-worker.exe").display()))
        .arg("/link");
        for name in ["avcodec", "avformat", "avutil", "swscale", "swresample"] {
            cmd.arg(ffmpeg.join(format!("lib/{name}.lib")));
        }
        cmd.args(["user32.lib", "gdi32.lib", "comctl32.lib", "winmm.lib"]);
        if !cmd.status()?.success() {
            return Err("Video worker build failed".into());
        }
        println!("cargo:rustc-cfg=bundled_video");
    } else if env::var("PROFILE").as_deref() == Ok("release") {
        return Err("Release requires bundled video codecs; run scripts/prepare-codecs.ps1".into());
    }
    Ok(())
}
