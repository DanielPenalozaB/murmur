use std::process::Command;

fn main() {
    link_clang_runtime();
    tauri_build::build()
}

/// whisper.cpp's Metal code checks the macOS version (`@available`) for features newer
/// than the 14.4 the app supports. Those checks call `__isPlatformVersionAtLeast` from
/// clang's runtime, which rustc doesn't link, so release builds failed to link.
fn link_clang_runtime() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(out) = Command::new("xcrun")
        .args(["clang", "--print-runtime-dir"])
        .output()
    else {
        return;
    };
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !dir.is_empty() {
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
}
