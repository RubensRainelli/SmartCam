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

#![allow(dead_code)]

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::SmartCamConfig;
use crate::engine_bridge::{is_ir_camera_name, CameraDeviceEntry, NativeEngineLibrary};
use crate::win32_ffi::*;

pub const SMARTCAM_CLSID: &str = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct BuildVariantStatus {
    pub name: String,
    pub label: String,
    pub mf_source_dll: PathBuf,
    pub engine_dll: PathBuf,
    pub has_mf_source: bool,
    pub has_engine: bool,
}

#[derive(Clone, Debug)]
pub struct SystemDiagnostics {
    pub clsid: String,
    pub clsid_registered: bool,
    pub registered_friendly_name: String,
    pub registered_inproc_dll: String,
    pub registered_dll_exists: bool,
    pub frameserver_enabled: Option<bool>,
    pub is_admin: bool,
    pub build_variants: Vec<BuildVariantStatus>,
    pub active_build_variant: String,
    pub physical_cameras: Vec<CameraDeviceEntry>,
    pub ipc_active: bool,
    pub ipc_width: u32,
    pub ipc_height: u32,
    pub ipc_fps: u32,
    pub ipc_frame_index: u64,
    pub autostart_enabled: bool,
}

pub fn detect_build_variants(repo_root: &Path) -> Vec<BuildVariantStatus> {
    let build_root = repo_root.join("platforms").join("windows").join("build");
    let bin_dir = repo_root.join("bin");
    let specs = [
        (
            "out_avx2",
            "AVX2 Baseline (Universale x86_64 — 256-bit YMM)",
            build_root.join("out_avx2"),
            "smartcam_engine_avx2.dll",
        ),
        (
            "out_avx512",
            "AVX-512 High-Throughput (512-bit ZMM Registers)",
            build_root.join("out_avx512"),
            "smartcam_engine_avx512.dll",
        ),
    ];

    let mut out = Vec::new();
    for (name, label, dir, bin_dll_name) in specs {
        let mf_primary = dir.join("smartcam_mf_source.dll");
        let mf_bin = bin_dir.join("smartcam_mf_source.dll");
        let mf_dll = if mf_primary.is_file() {
            mf_primary
        } else if mf_bin.is_file() {
            mf_bin
        } else {
            mf_primary
        };

        let eng_primary = dir.join("smartcam_engine.dll");
        let eng_sub = dir.join("common_engine").join("smartcam_engine.dll");
        let eng_bin = bin_dir.join(bin_dll_name);
        let eng_dll = if eng_primary.is_file() {
            eng_primary
        } else if eng_sub.is_file() {
            eng_sub
        } else if eng_bin.is_file() {
            eng_bin
        } else {
            eng_primary
        };

        out.push(BuildVariantStatus {
            name: name.to_string(),
            label: label.to_string(),
            has_mf_source: mf_dll.is_file(),
            has_engine: eng_dll.is_file(),
            mf_source_dll: mf_dll,
            engine_dll: eng_dll,
        });
    }
    out
}

fn reg_query_string(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    unsafe {
        let wsub = to_wstring(subkey);
        let mut hkey: HKEY = NULL;
        if RegOpenKeyExW(
            root,
            wsub.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        ) != 0
        {
            return None;
        }
        let wval = if value_name.is_empty() {
            vec![0u16]
        } else {
            to_wstring(value_name)
        };
        let mut buf = [0u16; 512];
        let mut cb = (buf.len() * 2) as DWORD;
        let mut dtype: DWORD = 0;
        let status = RegQueryValueExW(
            hkey,
            if value_name.is_empty() {
                std::ptr::null()
            } else {
                wval.as_ptr()
            },
            std::ptr::null_mut(),
            &mut dtype,
            buf.as_mut_ptr() as *mut BYTE,
            &mut cb,
        );
        RegCloseKey(hkey);
        if status == 0 {
            Some(from_wstring_slice(&buf))
        } else {
            None
        }
    }
}

fn reg_query_dword(root: HKEY, subkey: &str, value_name: &str) -> Option<u32> {
    unsafe {
        let wsub = to_wstring(subkey);
        let mut hkey: HKEY = NULL;
        if RegOpenKeyExW(
            root,
            wsub.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        ) != 0
        {
            return None;
        }
        let wval = to_wstring(value_name);
        let mut data: u32 = 0;
        let mut cb = std::mem::size_of::<u32>() as DWORD;
        let mut dtype: DWORD = 0;
        let status = RegQueryValueExW(
            hkey,
            wval.as_ptr(),
            std::ptr::null_mut(),
            &mut dtype,
            &mut data as *mut u32 as *mut BYTE,
            &mut cb,
        );
        RegCloseKey(hkey);
        if status == 0 {
            Some(data)
        } else {
            None
        }
    }
}

pub fn is_autostart_enabled() -> bool {
    reg_query_string(
        HKEY_CURRENT_USER,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "SmartCam",
    )
    .is_some()
}

pub fn set_autostart_enabled(enable: bool) -> bool {
    unsafe {
        let wsub = to_wstring("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let mut hkey: HKEY = NULL;
        let mut disp: DWORD = 0;
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wsub.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            &mut disp,
        ) != 0
        {
            return false;
        }
        let wval = to_wstring("SmartCam");
        let ok = if enable {
            if let Ok(exe) = std::env::current_exe() {
                let cmd = format!("\"{}\" --minimized", exe.display());
                let wcmd = to_wstring(&cmd);
                RegSetValueExW(
                    hkey,
                    wval.as_ptr(),
                    0,
                    REG_SZ,
                    wcmd.as_ptr() as *const BYTE,
                    (wcmd.len() * 2) as DWORD,
                ) == 0
            } else {
                false
            }
        } else {
            let _ = RegDeleteValueW(hkey, wval.as_ptr());
            true
        };
        RegCloseKey(hkey);
        ok
    }
}

pub fn probe_shared_memory() -> (bool, u32, u32, u32, u64) {
    unsafe {
        let wname = to_wstring("Local\\SmartCam_MF_SharedFrame_v1");
        let hmap = OpenFileMappingW(FILE_MAP_READ, FALSE, wname.as_ptr());
        if hmap.is_null() {
            return (false, 0, 0, 0, 0);
        }
        let view = MapViewOfFile(hmap, FILE_MAP_READ, 0, 0, 128) as *const u8;
        if view.is_null() {
            CloseHandle(hmap);
            return (false, 0, 0, 0, 0);
        }
        let magic = std::ptr::read_unaligned(view as *const u32);
        let version = std::ptr::read_unaligned(view.add(4) as *const u32);
        let width = std::ptr::read_unaligned(view.add(16) as *const u32);
        let height = std::ptr::read_unaligned(view.add(20) as *const u32);
        let fps = std::ptr::read_unaligned(view.add(28) as *const u32);
        let frame_idx = std::ptr::read_unaligned(view.add(48) as *const u64);

        UnmapViewOfFile(view as *const _);
        CloseHandle(hmap);

        if magic == 0x534D434D && version == 1 && width > 0 && height > 0 {
            (true, width, height, fps, frame_idx)
        } else {
            (false, 0, 0, 0, 0)
        }
    }
}

pub fn enumerate_cameras_smart(repo_root: &Path) -> Vec<CameraDeviceEntry> {
    let mut list = Vec::new();
    if let Some(lib) = NativeEngineLibrary::load_best(repo_root, "out_avx2", "auto") {
        list = lib.enumerate_cameras();
        unsafe {
            if !lib.hmodule.is_null() {
                FreeLibrary(lib.hmodule);
            }
        }
    }

    // Also query WMI PnP Camera devices (which includes Windows Hello IR Sensor cameras
    // registered under KSCATEGORY_SENSOR_CAMERA so the user can see both RGB and IR cameras)
    let ps_cmd = "Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPClass -in @('Camera','Image') -and $_.Status -eq 'OK' } | ForEach-Object { \"$($_.Name)|$($_.DeviceID)\" }";
    if let Ok(out) = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", ps_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (name, dev_id) = trimmed.split_once('|').unwrap_or((trimmed, ""));
            if name.eq_ignore_ascii_case("SmartCam") || name.contains("SmartCam Virtual Camera") {
                continue;
            }
            let already_present = list.iter().any(|c| c.name.eq_ignore_ascii_case(name));
            if !already_present {
                let next_idx = list.len() as u32;
                let is_ir = is_ir_camera_name(name, dev_id);
                list.push(CameraDeviceEntry {
                    index: next_idx,
                    name: name.to_string(),
                    symbolic_link: dev_id.to_string(),
                    is_infrared: is_ir,
                });
            }
        }
    }

    if list.is_empty() {
        list.push(CameraDeviceEntry {
            index: 0,
            name: "Webcam Principale / Flusso Sintetico 1080p".to_string(),
            symbolic_link: String::new(),
            is_infrared: false,
        });
    }
    list
}

pub fn select_best_default_camera_index(cameras: &[CameraDeviceEntry], current_idx: u32) -> u32 {
    if cameras.is_empty() {
        return 0;
    }
    // If current_idx points to a valid non-IR camera, keep it
    if let Some(cam) = cameras.iter().find(|c| c.index == current_idx) {
        if !cam.is_infrared {
            return current_idx;
        }
    }
    // Otherwise pick the first non-IR camera automatically!
    if let Some(non_ir) = cameras.iter().find(|c| !c.is_infrared) {
        return non_ir.index;
    }
    cameras[0].index
}

pub const DSHOW_VIDEO_INPUT_CATEGORY: &str = "{860BB310-5D01-11d0-BD3B-00A0C911CE86}";

const SMARTCAM_FILTER_DATA: [u8; 88] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x70, 0x69, 0x33, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x74, 0x79, 0x33, 0x00, 0x00, 0x00, 0x00,
    0x38, 0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00,
    0x76, 0x69, 0x64, 0x73, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

fn reg_set_string(root: HKEY, subkey: &str, value_name: &str, data: &str) -> bool {
    unsafe {
        let wsub = to_wstring(subkey);
        let mut hkey: HKEY = NULL;
        let mut disp: DWORD = 0;
        if RegCreateKeyExW(
            root,
            wsub.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE | KEY_WOW64_64KEY,
            std::ptr::null(),
            &mut hkey,
            &mut disp,
        ) != 0
        {
            return false;
        }
        let wval = if value_name.is_empty() {
            Vec::new()
        } else {
            to_wstring(value_name)
        };
        let wdata = to_wstring(data);
        let status = RegSetValueExW(
            hkey,
            if value_name.is_empty() {
                std::ptr::null()
            } else {
                wval.as_ptr()
            },
            0,
            REG_SZ,
            wdata.as_ptr() as *const BYTE,
            (wdata.len() * 2) as DWORD,
        );
        RegCloseKey(hkey);
        status == 0
    }
}

fn reg_set_binary(root: HKEY, subkey: &str, value_name: &str, bytes: &[u8]) -> bool {
    unsafe {
        let wsub = to_wstring(subkey);
        let mut hkey: HKEY = NULL;
        let mut disp: DWORD = 0;
        if RegCreateKeyExW(
            root,
            wsub.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE | KEY_WOW64_64KEY,
            std::ptr::null(),
            &mut hkey,
            &mut disp,
        ) != 0
        {
            return false;
        }
        let wval = to_wstring(value_name);
        let status = RegSetValueExW(
            hkey,
            wval.as_ptr(),
            0,
            REG_BINARY,
            bytes.as_ptr(),
            bytes.len() as DWORD,
        );
        RegCloseKey(hkey);
        status == 0
    }
}

fn register_virtual_camera_in_hive(root: HKEY, dll_path: &str) -> bool {
    let clsid_subkey = format!("Software\\Classes\\CLSID\\{}", SMARTCAM_CLSID);
    let inproc_subkey = format!("{}\\InprocServer32", clsid_subkey);
    let dshow_subkey = format!(
        "Software\\Classes\\CLSID\\{}\\Instance\\{}",
        DSHOW_VIDEO_INPUT_CATEGORY, SMARTCAM_CLSID
    );

    let ok1 = reg_set_string(root, &clsid_subkey, "", "SmartCam");
    let ok2 = reg_set_string(root, &inproc_subkey, "", dll_path);
    let ok3 = reg_set_string(root, &inproc_subkey, "ThreadingModel", "Both");
    let ok4 = reg_set_string(root, &dshow_subkey, "FriendlyName", "SmartCam");
    let ok5 = reg_set_string(root, &dshow_subkey, "CLSID", SMARTCAM_CLSID);
    let ok6 = reg_set_string(root, &dshow_subkey, "DevicePath", "smartcam://virtual-camera-0");
    let ok7 = reg_set_binary(root, &dshow_subkey, "FilterData", &SMARTCAM_FILTER_DATA);

    ok1 && ok2 && ok3 && ok4 && ok5 && ok6 && ok7
}

pub fn ensure_virtual_camera_registered(repo_root: &Path, preferred_variant: &str) -> Option<PathBuf> {
    let variants = detect_build_variants(repo_root);
    let mut chosen_dll: Option<PathBuf> = None;

    if let Some(v) = variants.iter().find(|v| v.name == preferred_variant && v.has_mf_source) {
        chosen_dll = Some(v.mf_source_dll.clone());
    }
    if chosen_dll.is_none() {
        if let Some(v) = variants.iter().find(|v| v.has_mf_source) {
            chosen_dll = Some(v.mf_source_dll.clone());
        }
    }
    if chosen_dll.is_none() {
        let bin_cand = repo_root.join("bin").join("smartcam_mf_source.dll");
        if bin_cand.is_file() {
            chosen_dll = Some(bin_cand);
        }
    }

    let dll_path = chosen_dll?;
    let dll_str = dll_path.to_string_lossy().to_string();

    // Register in HKCU (works immediately without UAC for Chromium, Edge, Firefox, OBS, Zoom, Teams)
    let ok_hkcu = register_virtual_camera_in_hive(HKEY_CURRENT_USER, &dll_str);
    // Also register in HKLM if elevated
    let _ = register_virtual_camera_in_hive(HKEY_LOCAL_MACHINE, &dll_str);

    if ok_hkcu {
        Some(dll_path)
    } else {
        None
    }
}

pub fn collect_diagnostics(repo_root: &Path, include_cameras: bool) -> SystemDiagnostics {
    let clsid_subkey = format!("SOFTWARE\\Classes\\CLSID\\{}", SMARTCAM_CLSID);
    let inproc_subkey = format!("{}\\InprocServer32", clsid_subkey);
    let dshow_subkey = format!(
        "SOFTWARE\\Classes\\CLSID\\{}\\Instance\\{}",
        DSHOW_VIDEO_INPUT_CATEGORY, SMARTCAM_CLSID
    );

    let mut friendly = reg_query_string(HKEY_LOCAL_MACHINE, &clsid_subkey, "")
        .or_else(|| reg_query_string(HKEY_CURRENT_USER, &clsid_subkey, ""));
    let mut inproc = reg_query_string(HKEY_LOCAL_MACHINE, &inproc_subkey, "")
        .or_else(|| reg_query_string(HKEY_CURRENT_USER, &inproc_subkey, ""))
        .unwrap_or_default();
    let dshow_friendly = reg_query_string(HKEY_LOCAL_MACHINE, &dshow_subkey, "FriendlyName")
        .or_else(|| reg_query_string(HKEY_CURRENT_USER, &dshow_subkey, "FriendlyName"));

    // Auto-heal / auto-register if missing, pointing to a non-existent DLL, or missing DirectShow category
    if inproc.is_empty() || !Path::new(&inproc).is_file() || dshow_friendly.is_none() {
        if let Some(healed_path) = ensure_virtual_camera_registered(repo_root, "out_avx2") {
            inproc = healed_path.to_string_lossy().to_string();
            friendly = Some("SmartCam".to_string());
        }
    }

    let clsid_registered = friendly.is_some() || !inproc.is_empty();
    let registered_dll_exists = !inproc.is_empty() && Path::new(&inproc).is_file();

    let frameserver_enabled = reg_query_dword(
        HKEY_LOCAL_MACHINE,
        "SOFTWARE\\Microsoft\\Windows Media Foundation\\Platform",
        "EnableFrameServerMode",
    )
    .map(|v| v == 1);

    let variants = detect_build_variants(repo_root);
    let active_build_variant = if inproc.to_ascii_lowercase().contains("out_avx512") {
        "out_avx512".to_string()
    } else {
        "out_avx2".to_string()
    };

    let (ipc_active, ipc_width, ipc_height, ipc_fps, ipc_frame_index) = probe_shared_memory();
    let physical_cameras = if include_cameras {
        enumerate_cameras_smart(repo_root)
    } else {
        Vec::new()
    };

    let is_admin = unsafe { IsUserAnAdmin() != 0 };

    SystemDiagnostics {
        clsid: SMARTCAM_CLSID.to_string(),
        clsid_registered,
        registered_friendly_name: friendly.unwrap_or_else(|| "SmartCam".to_string()),
        registered_inproc_dll: inproc,
        registered_dll_exists,
        frameserver_enabled,
        is_admin,
        build_variants: variants,
        active_build_variant,
        physical_cameras,
        ipc_active,
        ipc_width,
        ipc_height,
        ipc_fps,
        ipc_frame_index,
        autostart_enabled: is_autostart_enabled(),
    }
}

pub fn invoke_driver_install(repo_root: &Path, variant: &str) -> (bool, String) {
    // 1. Register immediately via native Win32 Registry APIs (HKCU + HKLM if admin)
    let native_reg_dll = ensure_virtual_camera_registered(repo_root, variant);

    let script = repo_root
        .join("platforms")
        .join("windows")
        .join("scripts")
        .join("install_driver.ps1");
    if !script.is_file() {
        if let Some(dll) = native_reg_dll {
            return (
                true,
                format!(
                    "Driver Virtual Camera 'SmartCam' registrato con successo ({})",
                    dll.display()
                ),
            );
        }
        return (false, format!("Script non trovato: {}", script.display()));
    }

    let variants = detect_build_variants(repo_root);
    let dll_path = variants
        .iter()
        .find(|v| v.name == variant)
        .map(|v| v.mf_source_dll.clone())
        .unwrap_or_else(|| {
            repo_root
                .join("platforms")
                .join("windows")
                .join("build")
                .join(variant)
                .join("smartcam_mf_source.dll")
        });

    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &script.to_string_lossy(),
        "-NonInteractiveAcceptPefl",
        "-DllPath",
        &dll_path.to_string_lossy(),
    ]);
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout.trim(), stderr.trim()).trim().to_string();
            if out.status.success() || native_reg_dll.is_some() {
                (
                    true,
                    if combined.is_empty() {
                        format!("Driver SmartCam ({}) registrato con successo!", variant)
                    } else {
                        combined
                    },
                )
            } else {
                (false, combined)
            }
        }
        Err(e) => {
            if let Some(dll) = native_reg_dll {
                (
                    true,
                    format!(
                        "Driver SmartCam registrato nativamente ({})",
                        dll.display()
                    ),
                )
            } else {
                (false, format!("Errore esecuzione install_driver.ps1: {}", e))
            }
        }
    }
}

pub fn invoke_driver_uninstall(repo_root: &Path) -> (bool, String) {
    let script = repo_root
        .join("platforms")
        .join("windows")
        .join("scripts")
        .join("uninstall_driver.ps1");
    if !script.is_file() {
        return (false, format!("Script non trovato: {}", script.display()));
    }

    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &script.to_string_lossy(),
        "-NonInteractiveAcceptPefl",
    ]);
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout.trim(), stderr.trim()).trim().to_string();
            (out.status.success(), combined)
        }
        Err(e) => (false, format!("Errore esecuzione uninstall_driver.ps1: {}", e)),
    }
}

pub fn open_windows11_camera_privacy_settings() {
    unsafe {
        let op = to_wstring("open");
        let uri = to_wstring("ms-settings:privacy-webcam");
        ShellExecuteW(NULL, op.as_ptr(), uri.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
}

pub fn open_web_testbench(repo_root: &Path) -> String {
    let html_path = repo_root.join("app").join("web_preview").join("index.html");
    let url = if html_path.is_file() {
        format!(
            "file:///{}",
            html_path.to_string_lossy().replace('\\', "/")
        )
    } else {
        "http://localhost:8080".to_string()
    };
    unsafe {
        let op = to_wstring("open");
        let wurl = to_wstring(&url);
        ShellExecuteW(NULL, op.as_ptr(), wurl.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
    url
}

pub fn print_cli_status(diag: &SystemDiagnostics, cfg: &SmartCamConfig, as_json: bool) {
    if as_json {
        let cams_json: Vec<String> = diag
            .physical_cameras
            .iter()
            .map(|c| {
                format!(
                    "{{\"index\": {}, \"name\": \"{}\", \"label\": \"{}\", \"is_infrared\": {}}}",
                    c.index,
                    c.name.replace('"', "\\\""),
                    c.display_label().replace('"', "\\\""),
                    c.is_infrared
                )
            })
            .collect();
        let plugins_json: Vec<String> = cfg
            .enabled_plugins
            .iter()
            .map(|p| format!("\"{}\"", p.replace('"', "\\\"")))
            .collect();
        println!(
            "{{\n  \"clsid\": \"{}\",\n  \"clsid_registered\": {},\n  \"architecture\": \"x86_64 (64-bit Native)\",\n  \"source_bit_depth\": {},\n  \"registered_inproc_dll\": \"{}\",\n  \"registered_dll_exists\": {},\n  \"frameserver_enabled\": {},\n  \"active_build_variant\": \"{}\",\n  \"ipc_active\": {},\n  \"autostart_enabled\": {},\n  \"physical_cameras\": [{}],\n  \"aspect_ratio\": \"{}\",\n  \"segmentation_mode\": \"{}\",\n  \"background_mode\": \"{}\",\n  \"background_mirror_h\": {},\n  \"background_mirror_v\": {},\n  \"compute_backend\": \"{}\",\n  \"simd_acceleration\": \"{}\",\n  \"ai_enabled\": {},\n  \"enabled_plugins\": [{}]\n}}",
            diag.clsid,
            diag.clsid_registered,
            cfg.camera_input.bit_depth,
            diag.registered_inproc_dll.replace('\\', "\\\\"),
            diag.registered_dll_exists,
            diag.frameserver_enabled
                .map(|b| b.to_string())
                .unwrap_or_else(|| "null".to_string()),
            diag.active_build_variant,
            diag.ipc_active,
            diag.autostart_enabled,
            cams_json.join(", "),
            cfg.framing_output.aspect_ratio,
            cfg.segmentation.mode,
            cfg.compositor.background_mode,
            cfg.compositor.background_mirror_h,
            cfg.compositor.background_mirror_v,
            cfg.compute_backend,
            cfg.compositor.simd_acceleration,
            cfg.ai_enabled,
            plugins_json.join(", ")
        );
        return;
    }

    println!("========================================================================");
    println!("  SmartCam Native Rust Application (SmartCam.exe) — Stato del Sistema");
    println!("========================================================================");
    println!("  * Architettura Binari    : 64-bit Nativo (x86_64 PE32+)");
    println!(
        "  * Profondità Sorgente    : {}-bit ({})",
        cfg.camera_input.bit_depth,
        if cfg.camera_input.bit_depth == 64 {
            "BGRA64 Deep Color 16-bit/canale + BGRA32 8-bit/canale"
        } else {
            "BGRA32 Standard 8-bit/canale"
        }
    );
    println!("  * Media Foundation CLSID : {}", diag.clsid);
    println!(
        "  * Stato Registro Driver  : {}",
        if diag.clsid_registered { "REGISTRATO (OK — 64-bit x86_64)" } else { "NON REGISTRATO" }
    );
    if !diag.registered_inproc_dll.is_empty() {
        println!(
            "  * Driver COM 64-bit DLL  : {} (Presente={})",
            diag.registered_inproc_dll, diag.registered_dll_exists
        );
    }
    println!("  * EnableFrameServerMode  : {:?}", diag.frameserver_enabled);
    println!(
        "  * Shared Memory IPC      : {}",
        if diag.ipc_active {
            format!(
                "ATTIVO ({}x{} @ {} FPS, Frame #{})",
                diag.ipc_width, diag.ipc_height, diag.ipc_fps, diag.ipc_frame_index
            )
        } else {
            "IN ATTESA (Standby)".to_string()
        }
    );
    println!("------------------------------------------------------------------------");
    println!("  Varianti Motore C++ SIMD Compilate:");
    for b in &diag.build_variants {
        let mark = if b.name == diag.active_build_variant { "*" } else { " " };
        println!(
            "   [{}] {:10} : MF_Source={} | Engine_SIMD={} ({})",
            mark, b.name, b.has_mf_source, b.has_engine, b.label
        );
    }
    println!("------------------------------------------------------------------------");
    println!("  Fotocamere Rilevate (con Filtro Intelligente Sensore IR):");
    for cam in &diag.physical_cameras {
        let default_tag = if cam.index == cfg.camera_input.device_index {
            " <-- [ATTIVA DI DEFAULT]"
        } else {
            ""
        };
        println!("    - {}{}", cam.display_label(), default_tag);
    }
    println!("------------------------------------------------------------------------");
    println!("  Configurazione Attiva (`config/default_config.yaml`):");
    println!(
        "    - Input Fotocamera : Dispositivo #{} ({}x{} @ {} FPS, Sorgente {}-bit)",
        cfg.camera_input.device_index,
        cfg.camera_input.width,
        cfg.camera_input.height,
        cfg.camera_input.fps,
        cfg.camera_input.bit_depth
    );
    println!(
        "    - Formato Output   : {} ({}x{}, {})",
        cfg.framing_output.aspect_ratio,
        cfg.framing_output.output_width,
        cfg.framing_output.output_height,
        cfg.framing_output.crop_strategy
    );
    println!(
        "    - Effetto Sfondo   : mode={}, bg={}, mirror_h={}, mirror_v={}, backend={}, simd={}, ai_enabled={}",
        cfg.segmentation.mode,
        cfg.compositor.background_mode,
        cfg.compositor.background_mirror_h,
        cfg.compositor.background_mirror_v,
        cfg.compute_backend,
        cfg.compositor.simd_acceleration,
        cfg.ai_enabled
    );
    println!(
        "    - Plugin Attivi    : {:?}",
        cfg.enabled_plugins
    );
    println!("========================================================================");
}
