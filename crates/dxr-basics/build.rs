use std::{fs, path};

fn main() {
    println!("cargo::rerun-if-changed=../../bin/");
    println!("cargo::rerun-if-changed=shaders/");

    copy_dll("dxcompiler.dll");
    copy_dll("dxil.dll");
    copy_dll("WinPixEventRuntime.dll");

    copy_shaders("rasterization.hlsl");
    copy_shaders("raytracing.hlsl");
    copy_shaders("fullscreen.hlsl");
    copy_shaders("scene.hlsl");
}

fn copy_dll(dll: &str) {
    let out_dir = std::env::var("OUT_DIR").unwrap() + "/../../../";

    let src = "../../bin/".to_string() + dll;
    let dst = out_dir.clone() + dll;
    println!("Copying {src} to {dst}");

    if let Err(e) = fs::copy(&src, &dst) {
        panic!("Failed to copy {src}: {e}");
    }
}

fn copy_shaders(shader: &str) {
    let out_dir = std::env::var("OUT_DIR").unwrap() + "/../../../";
    let out_dir = path::Path::new(&out_dir).join("shaders/dxr-basics/");
    create_dir(out_dir.parent().unwrap());
    create_dir(&out_dir);

    if !out_dir.exists() {
        if let Err(e) = fs::create_dir(&out_dir) {
            println!(
                "Failed to create {}: {e}",
                out_dir.as_os_str().to_str().unwrap()
            )
        }
    }

    let src_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap() + "/shaders/";
    let src = src_dir + shader;
    let dst = out_dir.join(shader);
    println!("Copying {src} to {}", dst.as_os_str().to_str().unwrap());

    if let Err(e) = fs::copy(&src, &dst) {
        panic!("Failed to copy {src}: {e}");
    }
}

fn create_dir(dir: &path::Path) {
    if !dir.exists() {
        if let Err(e) = fs::create_dir(dir) {
            println!(
                "Failed to create {}: {e}",
                dir.as_os_str().to_str().unwrap()
            )
        }
    }
}
