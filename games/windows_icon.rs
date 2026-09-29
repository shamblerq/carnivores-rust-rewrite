//! The build script of each game and the launcher: for Windows, the
//! program's icon (icon.ico beside Cargo.toml, made by icons/make_icons.py)
//! compiled into a resource by MinGW's windres and linked in as resource 1,
//! which Explorer shows and the window takes (carn_engine's window_icon).
//!
//! Without windres (another toolchain, or none installed) the program is
//! built all the same, only without its icon.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "gnu" {
        return;
    }
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = dir.join("icon.ico");
    let rc = out.join("icon.rc");
    let obj = out.join("icon.o");
    let ico_path = ico.display().to_string().replace('\\', "/");
    if std::fs::write(&rc, format!("1 ICON \"{ico_path}\"\n")).is_err() {
        return;
    }
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| {
        if cfg!(windows) {
            "windres".into()
        } else {
            "x86_64-w64-mingw32-windres".into()
        }
    });
    let ok = Command::new(&windres)
        .arg("-i")
        .arg(&rc)
        .arg("-o")
        .arg(&obj)
        .args(["-O", "coff"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    } else {
        println!("cargo:warning={windres} failed: building without the icon");
    }
}
