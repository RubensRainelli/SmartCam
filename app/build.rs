/*
 * Personal and Educational Free License (PEF License / PEFL)
 * Version 1.0 - 2026-05-26
 * Copyright (c) 2026 Rubens Rainelli
 *
 * Contact Details:
 *   Email: rubens@rainelli.it
 *   GitHub: https://github.com/RubensRainelli/
 *   Tel: (+39) 342 811 5882
 *
 * Personal and educational free use permitted.
 * Corporate, business or enterprise use prohibited without written authorization and fee.
 * The full license can be found in the LICENSE file.
 */

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_windres() -> Option<(PathBuf, Option<PathBuf>)> {
    if let Ok(output) = Command::new("windres").arg("--version").output() {
        if output.status.success() {
            return Some((PathBuf::from("windres"), None));
        }
    }

    let mut candidates = Vec::new();
    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        let winget_mingw = Path::new(&local_app_data)
            .join("Microsoft")
            .join("WinGet")
            .join("Packages")
            .join("BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe")
            .join("mingw64")
            .join("bin");
        candidates.push(winget_mingw);
    }
    candidates.push(PathBuf::from(r"C:\mingw64\bin"));
    candidates.push(PathBuf::from(r"C:\msys64\mingw64\bin"));
    candidates.push(PathBuf::from(r"C:\msys64\ucrt64\bin"));

    for bin_dir in candidates {
        let exe = bin_dir.join("windres.exe");
        if exe.exists() {
            return Some((exe, Some(bin_dir)));
        }
    }

    None
}

fn main() {
    println!("cargo:rerun-if-changed=smartcam.rc");
    println!("cargo:rerun-if-changed=../assets/icons/smartcam_cyber_lens.ico");
    println!("cargo:rerun-if-changed=../assets/icons/smartcam_emerald_cam.ico");
    println!("cargo:rerun-if-changed=../assets/icons/smartcam_neon_iris.ico");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    if target_env == "gnu" {
        if let Some((windres_bin, extra_bin_dir)) = find_windres() {
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            let rc_path = manifest_dir.join("smartcam.rc");
            let obj_path = out_dir.join("smartcam_res.o");

            let mut cmd = Command::new(&windres_bin);
            cmd.current_dir(&manifest_dir)
                .arg("-i")
                .arg(&rc_path)
                .arg("-O")
                .arg("coff")
                .arg("-o")
                .arg(&obj_path);

            if let Some(bin_dir) = extra_bin_dir {
                let current_path = env::var("PATH").unwrap_or_default();
                cmd.env("PATH", format!("{};{}", bin_dir.display(), current_path));
            }

            if let Ok(status) = cmd.status() {
                if status.success() {
                    println!("cargo:rustc-link-arg={}", obj_path.display());
                    return;
                }
            }
        }
    }

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("../assets/icons/smartcam_cyber_lens.ico", "101");
        res.set_icon_with_id("../assets/icons/smartcam_emerald_cam.ico", "102");
        res.set_icon_with_id("../assets/icons/smartcam_neon_iris.ico", "103");
        res.set("ProductName", "SmartCam");
        res.set(
            "FileDescription",
            "SmartCam v2.0.0 — Pure Rust Studio Virtual Camera",
        );
        res.set("CompanyName", "Rubens Rainelli");
        res.set(
            "LegalCopyright",
            "Copyright (c) 2026 Rubens Rainelli (PEFL v1.0)",
        );
        res.set("FileVersion", "2.0.0.0");
        res.set("ProductVersion", "2.0.0.0");
        let _ = res.compile();
    }
}
