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

#![cfg(not(target_os = "windows"))]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::{resolve_repo_root, SmartCamConfig};

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

const RTLD_NOW: c_int = 2;

type FnCreate = unsafe extern "C" fn(u32) -> *mut c_void;
type FnDestroy = unsafe extern "C" fn(*mut c_void);
type FnGetSimdBackend = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type FnCropScale = unsafe extern "C" fn(
    *const u8,
    u32,
    u32,
    u32,
    *mut u8,
    u32,
    u32,
    u32,
    u32,
    u32,
    i32,
) -> c_int;
type FnComputeAiMask = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    u32,
    u32,
    *mut u8,
    u32,
) -> c_int;
type FnGenerateBlurBg = unsafe extern "C" fn(*const u8, *mut u8, u32, u32, u32, u32) -> c_int;
type FnAlphaBlend = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    *const u8,
    *const u8,
    *mut u8,
    u32,
    u32,
    u32,
    u32,
) -> c_int;

type FnGetCamCount = unsafe extern "C" fn() -> u32;
type FnGetCamInfo = unsafe extern "C" fn(u32, *mut c_char, u32, *mut c_char, u32, *mut c_int) -> c_int;
type FnCamCreate = unsafe extern "C" fn() -> *mut c_void;
type FnCamOpen = unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> c_int;
type FnCamRead = unsafe extern "C" fn(*mut c_void, *mut u8, u32, *mut u32, *mut u32) -> c_int;
type FnCamClose = unsafe extern "C" fn(*mut c_void);
type FnCamDestroy = unsafe extern "C" fn(*mut c_void);

type FnSinkCreate = unsafe extern "C" fn() -> *mut c_void;
type FnSinkInit = unsafe extern "C" fn(*mut c_void, *const c_char, u32, u32, u32) -> c_int;
type FnSinkPush = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32) -> c_int;
type FnSinkDestroy = unsafe extern "C" fn(*mut c_void);

unsafe fn load_sym<T>(handle: *mut c_void, name: &str) -> Option<T> {
    let cname = CString::new(name).ok()?;
    let ptr = dlsym(handle, cname.as_ptr());
    if ptr.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&ptr))
    }
}

fn find_linux_engine_so(repo_root: &Path) -> Option<PathBuf> {
    if let Ok(env_lib) = std::env::var("SMARTCAM_ENGINE_LIB") {
        let p = PathBuf::from(env_lib);
        if p.exists() {
            return Some(p);
        }
    }
    let candidates = [
        repo_root.join("usr/lib/libsmartcam_engine_avx512.so"),
        repo_root.join("usr/lib/libsmartcam_engine_avx2.so"),
        repo_root.join("usr/lib/libsmartcam_engine.so"),
        repo_root.join("platforms/linux/build/out_avx512/libsmartcam_engine_avx512.so"),
        repo_root.join("platforms/linux/build/out_avx2/libsmartcam_engine_avx2.so"),
        repo_root.join("platforms/linux/build/out_avx2/libsmartcam_engine.so"),
        repo_root.join("bin/libsmartcam_engine.so"),
    ];
    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }
    None
}

pub fn run_linux_main() {
    let repo_root = resolve_repo_root();
    let cfg_path = repo_root.join("config").join("default_config.yaml");
    let cfg = SmartCamConfig::load_from_file(&cfg_path);

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut max_frames: u64 = 0;
    let mut status_only = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                println!("SmartCam Linux AppImage (PEFL v1.0) — Copyright (c) 2026 Rubens Rainelli");
                println!("Utilizzo: SmartCam [--status] [--frames N] [--headless] [--accept-pefl]");
                return;
            }
            "--status" => status_only = true,
            "--frames" if i + 1 < args.len() => {
                i += 1;
                max_frames = args[i].parse().unwrap_or(10);
            }
            _ => {}
        }
        i += 1;
    }

    let so_path = match find_linux_engine_so(&repo_root) {
        Some(p) => p,
        None => {
            eprintln!("[SmartCam Linux] Impossibile trovare libsmartcam_engine.so");
            return;
        }
    };

    unsafe {
        let c_so = CString::new(so_path.to_string_lossy().as_bytes()).unwrap_or_default();
        let hlib = dlopen(c_so.as_ptr(), RTLD_NOW);
        if hlib.is_null() {
            eprintln!("[SmartCam Linux] Errore dlopen su {}", so_path.display());
            return;
        }

        let fn_create: Option<FnCreate> = load_sym(hlib, "smartcam_engine_create");
        let fn_destroy: Option<FnDestroy> = load_sym(hlib, "smartcam_engine_destroy");
        let fn_simd: Option<FnGetSimdBackend> = load_sym(hlib, "smartcam_engine_get_simd_backend");
        let fn_crop: Option<FnCropScale> = load_sym(hlib, "smartcam_engine_crop_scale_bgra");
        let fn_ai_mask: Option<FnComputeAiMask> = load_sym(hlib, "smartcam_engine_compute_ai_mask");
        let fn_blur: Option<FnGenerateBlurBg> = load_sym(hlib, "smartcam_engine_generate_blurred_bg");
        let fn_blend: Option<FnAlphaBlend> = load_sym(hlib, "smartcam_engine_blend_bgra");

        let fn_cam_count: Option<FnGetCamCount> = load_sym(hlib, "smartcam_linux_get_camera_count");
        let fn_cam_info: Option<FnGetCamInfo> = load_sym(hlib, "smartcam_linux_get_camera_info");
        let fn_cam_create: Option<FnCamCreate> = load_sym(hlib, "smartcam_linux_camera_create");
        let fn_cam_open: Option<FnCamOpen> = load_sym(hlib, "smartcam_linux_camera_open");
        let fn_cam_read: Option<FnCamRead> = load_sym(hlib, "smartcam_linux_camera_read_frame");
        let fn_cam_close: Option<FnCamClose> = load_sym(hlib, "smartcam_linux_camera_close");
        let fn_cam_destroy: Option<FnCamDestroy> = load_sym(hlib, "smartcam_linux_camera_destroy");

        let fn_sink_create: Option<FnSinkCreate> = load_sym(hlib, "smartcam_linux_sink_create");
        let fn_sink_init: Option<FnSinkInit> = load_sym(hlib, "smartcam_linux_sink_init");
        let fn_sink_push: Option<FnSinkPush> = load_sym(hlib, "smartcam_linux_sink_push_frame");
        let fn_sink_destroy: Option<FnSinkDestroy> = load_sym(hlib, "smartcam_linux_sink_destroy");

        let eng = fn_create.map(|f| f(0)).unwrap_or(std::ptr::null_mut());
        let simd_name = if let (Some(f), false) = (fn_simd, eng.is_null()) {
            let ptr = f(eng);
            if !ptr.is_null() {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            } else {
                "AVX2".to_string()
            }
        } else {
            "AVX2".to_string()
        };

        let cam_count = fn_cam_count.map(|f| f()).unwrap_or(0);
        println!("============================================================");
        println!("  SmartCam Linux v1.0.0 (V4L2 + v4l2loopback + C++17 SIMD)");
        println!("  Libreria Motore : {}", so_path.display());
        println!("  Backend SIMD    : {}", simd_name);
        println!("  Fotocamere V4L2 : {} rilevate", cam_count);

        let mut selected_cam = 0u32;
        if let Some(f_info) = fn_cam_info {
            let mut found_rgb = false;
            for idx in 0..cam_count {
                let mut name_buf = [0i8; 256];
                let mut sym_buf = [0i8; 256];
                let mut is_ir: c_int = 0;
                if f_info(idx, name_buf.as_mut_ptr(), 256, sym_buf.as_mut_ptr(), 256, &mut is_ir) != 0 {
                    let cname = CStr::from_ptr(name_buf.as_ptr()).to_string_lossy();
                    let csym = CStr::from_ptr(sym_buf.as_ptr()).to_string_lossy();
                    println!(
                        "    [{}] {} ({}) {}",
                        idx,
                        cname,
                        csym,
                        if is_ir != 0 { "[IR]" } else { "[RGB]" }
                    );
                    if !found_rgb && is_ir == 0 {
                        selected_cam = idx;
                        found_rgb = true;
                    }
                }
            }
        }
        println!("============================================================");

        if status_only {
            if let Some(f) = fn_destroy {
                if !eng.is_null() {
                    f(eng);
                }
            }
            dlclose(hlib);
            return;
        }

        let out_w = cfg.framing_output.output_width.max(640);
        let out_h = cfg.framing_output.output_height.max(480);
        let fps = cfg.camera_input.fps.max(15);

        let cam_handle = fn_cam_create.map(|f| f()).unwrap_or(std::ptr::null_mut());
        if let Some(f_open) = fn_cam_open {
            if !cam_handle.is_null() {
                let _ = f_open(cam_handle, selected_cam, cfg.camera_input.width, cfg.camera_input.height, fps);
            }
        }

        let sink_handle = fn_sink_create.map(|f| f()).unwrap_or(std::ptr::null_mut());
        if let Some(f_sinit) = fn_sink_init {
            if !sink_handle.is_null() {
                let cname = CString::new("SmartCam").unwrap();
                let _ = f_sinit(sink_handle, cname.as_ptr(), out_w, out_h, fps);
            }
        }

        let mut raw_buf = vec![0u8; (1920 * 1080 * 4) as usize];
        let mut framed_buf = vec![0u8; (out_w * out_h * 4) as usize];
        let mut bg_buf = vec![0u8; (out_w * out_h * 4) as usize];
        let mut mask_buf = vec![255u8; (out_w * out_h) as usize];
        let mut out_buf = vec![0u8; (out_w * out_h * 4) as usize];

        let interval = Duration::from_millis((1000 / fps.max(1)) as u64);
        let mut frames = 0u64;

        println!("[SmartCam Linux] Streaming attivo su Virtual Camera 'SmartCam' ({}x{} @ {} FPS). Premi Ctrl+C per terminare.", out_w, out_h, fps);

        loop {
            let t0 = Instant::now();
            let mut rw = cfg.camera_input.width.max(640);
            let mut rh = cfg.camera_input.height.max(480);

            let mut got_cam = false;
            if let Some(f_read) = fn_cam_read {
                if !cam_handle.is_null() {
                    let mut aw = 0u32;
                    let mut ah = 0u32;
                    if f_read(cam_handle, raw_buf.as_mut_ptr(), raw_buf.len() as u32, &mut aw, &mut ah) != 0
                        && aw > 0
                        && ah > 0
                    {
                        rw = aw;
                        rh = ah;
                        got_cam = true;
                    }
                }
            }
            if !got_cam {
                rw = out_w;
                rh = out_h;
            }

            if let Some(f_c) = fn_crop {
                let _ = f_c(
                    raw_buf.as_ptr(),
                    rw,
                    rh,
                    rw * 4,
                    framed_buf.as_mut_ptr(),
                    out_w,
                    out_h,
                    out_w * 4,
                    4,
                    3,
                    0,
                );
            }
            if let (Some(f_m), Some(f_b), Some(f_bl)) = (fn_ai_mask, fn_blur, fn_blend) {
                if !eng.is_null() {
                    let _ = f_b(framed_buf.as_ptr(), bg_buf.as_mut_ptr(), out_w, out_h, out_w * 4, 14);
                    let _ = f_m(eng, framed_buf.as_ptr(), out_w, out_h, out_w * 4, mask_buf.as_mut_ptr(), out_w);
                    let _ = f_bl(
                        eng,
                        framed_buf.as_ptr(),
                        bg_buf.as_ptr(),
                        mask_buf.as_ptr(),
                        out_buf.as_mut_ptr(),
                        out_w,
                        out_h,
                        out_w * 4,
                        out_w,
                    );
                }
            } else {
                out_buf.copy_from_slice(&framed_buf);
            }

            if let Some(f_push) = fn_sink_push {
                if !sink_handle.is_null() {
                    let _ = f_push(sink_handle, out_buf.as_ptr(), out_w, out_h, out_w * 4);
                }
            }

            frames += 1;
            if max_frames > 0 && frames >= max_frames {
                break;
            }
            let el = t0.elapsed();
            if el < interval {
                std::thread::sleep(interval - el);
            }
        }

        if let Some(f) = fn_cam_close {
            if !cam_handle.is_null() {
                f(cam_handle);
            }
        }
        if let Some(f) = fn_cam_destroy {
            if !cam_handle.is_null() {
                f(cam_handle);
            }
        }
        if let Some(f) = fn_sink_destroy {
            if !sink_handle.is_null() {
                f(sink_handle);
            }
        }
        if let Some(f) = fn_destroy {
            if !eng.is_null() {
                f(eng);
            }
        }
        dlclose(hlib);
    }
}
