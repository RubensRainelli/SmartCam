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

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{compute_center_crop_rect, resolve_aspect_ratio_pair, SmartCamConfig};
use crate::win32_ffi::*;

type FnEngineCreate = unsafe extern "C" fn(u32) -> *mut c_void;
type FnEngineDestroy = unsafe extern "C" fn(*mut c_void);
type FnGetSimdBackend = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type FnConfigureGreenscreen =
    unsafe extern "C" fn(*mut c_void, u8, u8, u8, u8, u8, u8, u8, u8, f32);
type FnAutoCalibrateGreenscreen = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    u32,
    u32,
    i32,
    i32,
    *mut u8,
    *mut u8,
    *mut u8,
    *mut u8,
) -> i32;
type FnCropScaleBgra =
    unsafe extern "C" fn(*const u8, u32, u32, u32, *mut u8, u32, u32, u32, u32, u32, u32) -> i32;
type FnComputeGreenscreenMask =
    unsafe extern "C" fn(*mut c_void, *mut u8, u32, u32, u32, *mut u8, u32) -> i32;
type FnComputeAiMask =
    unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32, *mut u8, u32) -> i32;
type FnFillSolidBg = unsafe extern "C" fn(*mut u8, u32, u32, u32, u8, u8, u8) -> i32;
type FnGenerateBlurredBg = unsafe extern "C" fn(*const u8, *mut u8, u32, u32, u32, u32) -> i32;
type FnBlendBgra = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    *const u8,
    *const u8,
    *mut u8,
    u32,
    u32,
    u32,
    u32,
) -> i32;
type FnSetReferenceBg = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32) -> i32;
type FnClearReferenceBg = unsafe extern "C" fn(*mut c_void);
type FnHasReferenceBg = unsafe extern "C" fn(*mut c_void) -> i32;
type FnSetAiModeAndTolerance = unsafe extern "C" fn(*mut c_void, i32, f32);
type FnGetSubjectCenter = unsafe extern "C" fn(*mut c_void, *mut f32, *mut f32) -> i32;
type FnSetHardwarePolicy = unsafe extern "C" fn(*mut c_void, u32, u32, i32) -> i32;
type FnGetActiveComputeBackend = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type FnGetActiveComputeDeviceName = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type FnIsAiEnabled = unsafe extern "C" fn(*mut c_void) -> i32;
type FnCalibrateFaceLandmarks = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    u32,
    u32,
    f32,
    f32, // left_eye_x, left_eye_y
    f32,
    f32, // right_eye_x, right_eye_y
    f32,
    f32, // nose_tip_x, nose_tip_y
    f32,
    f32, // lips_x, lips_y
    f32,
    f32, // chin_x, chin_y
    *mut f32, // out_skin_cb
    *mut f32, // out_skin_cr
    *mut f32, // out_head_rx
    *mut f32, // out_head_ry
) -> i32;
type FnSetFaceCalibrationProfile = unsafe extern "C" fn(
    *mut c_void,
    i32, // enabled (0 or 1)
    f32, // skin_cb
    f32, // skin_cr
    f32, // head_rx
    f32, // head_ry
) -> i32;
type FnSetFaceCalibrationLandmarks = unsafe extern "C" fn(*mut c_void, *const f32) -> i32;
type FnSetSilhouetteMode = unsafe extern "C" fn(*mut c_void, i32) -> i32; // 0 = Volto + Busto, 1 = Solo Volto
type FnGetSilhouetteMode = unsafe extern "C" fn(*mut c_void) -> i32;
type FnSetMultiPersonMode = unsafe extern "C" fn(*mut c_void, i32) -> i32; // 0 = Singolo, 1 = Multi-Persona

// Windows Camera & Sink C API exports (from win_bridge_c_api)
type FnWinGetCameraCount = unsafe extern "C" fn() -> u32;
type FnWinGetCameraInfo =
    unsafe extern "C" fn(u32, *mut c_char, u32, *mut c_char, u32, *mut i32) -> i32;
type FnWinCameraCreate = unsafe extern "C" fn() -> *mut c_void;
type FnWinCameraOpen = unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> i32;
type FnWinCameraReadFrame =
    unsafe extern "C" fn(*mut c_void, *mut u8, u32, *mut u32, *mut u32) -> i32;
type FnWinCameraClose = unsafe extern "C" fn(*mut c_void);
type FnWinCameraDestroy = unsafe extern "C" fn(*mut c_void);

type FnWinSinkCreate = unsafe extern "C" fn() -> *mut c_void;
type FnWinSinkInit = unsafe extern "C" fn(*mut c_void, *const c_char, u32, u32, u32) -> i32;
type FnWinSinkPushFrame = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32) -> i32;
type FnWinSinkIsConsumerActive = unsafe extern "C" fn(*mut c_void) -> i32;
type FnWinSinkDestroy = unsafe extern "C" fn(*mut c_void);

#[derive(Clone, Debug)]
pub struct CameraDeviceEntry {
    pub index: u32,
    pub name: String,
    pub symbolic_link: String,
    pub is_infrared: bool,
}

impl CameraDeviceEntry {
    pub fn display_label(&self) -> String {
        if self.is_infrared {
            if self.name.contains("[Sensore IR]") {
                format!("[{}] {}", self.index, self.name)
            } else {
                format!("[{}] {} [Sensore IR]", self.index, self.name)
            }
        } else {
            format!("[{}] {}", self.index, self.name)
        }
    }
}

pub fn is_ir_camera_name(name: &str, symlink: &str) -> bool {
    let n_up = name.to_ascii_uppercase();
    let s_up = symlink.to_ascii_uppercase();
    if n_up.contains("INFRARED")
        || n_up.contains("INFRAROSSI")
        || n_up.contains("WINDOWS HELLO")
        || s_up.contains("INFRARED")
        || s_up.contains("WINDOWS HELLO")
        || s_up.contains("SENSOR_CAMERA")
    {
        return true;
    }
    has_ir_word(&n_up) || has_ir_word(&s_up)
}

fn has_ir_word(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    for i in 0..=(bytes.len() - 2) {
        if bytes[i] == b'I' && bytes[i + 1] == b'R' {
            let left_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            let right_ok = (i + 2 >= bytes.len()) || !bytes[i + 2].is_ascii_alphabetic();
            if left_ok && right_ok {
                return true;
            }
        }
    }
    false
}

unsafe fn load_proc<T: Copy>(hmod: HMODULE, name: &[u8]) -> Option<T> {
    if hmod.is_null() {
        return None;
    }
    let ptr = GetProcAddress(hmod, name.as_ptr());
    if ptr.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&ptr))
    }
}

pub fn cpu_supports_avx512() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

pub struct NativeEngineLibrary {
    pub hmodule: HMODULE,
    pub loaded_dll_path: PathBuf,
    pub variant_name: String,
    pub fn_create: Option<FnEngineCreate>,
    pub fn_destroy: Option<FnEngineDestroy>,
    pub fn_get_simd: Option<FnGetSimdBackend>,
    pub fn_cfg_gs: Option<FnConfigureGreenscreen>,
    pub fn_autocal_gs: Option<FnAutoCalibrateGreenscreen>,
    pub fn_crop_scale: Option<FnCropScaleBgra>,
    pub fn_gs_mask: Option<FnComputeGreenscreenMask>,
    pub fn_ai_mask: Option<FnComputeAiMask>,
    pub fn_fill_solid: Option<FnFillSolidBg>,
    pub fn_blur_bg: Option<FnGenerateBlurredBg>,
    pub fn_blend: Option<FnBlendBgra>,
    pub fn_set_ref_bg: Option<FnSetReferenceBg>,
    pub fn_clear_ref_bg: Option<FnClearReferenceBg>,
    pub fn_has_ref_bg: Option<FnHasReferenceBg>,
    pub fn_set_ai_mode_tol: Option<FnSetAiModeAndTolerance>,
    pub fn_get_subj_center: Option<FnGetSubjectCenter>,
    pub fn_set_hardware_policy: Option<FnSetHardwarePolicy>,
    pub fn_get_compute_backend: Option<FnGetActiveComputeBackend>,
    pub fn_get_compute_device_name: Option<FnGetActiveComputeDeviceName>,
    pub fn_is_ai_enabled: Option<FnIsAiEnabled>,
    pub fn_calibrate_face_landmarks: Option<FnCalibrateFaceLandmarks>,
    pub fn_set_face_calibration_profile: Option<FnSetFaceCalibrationProfile>,
    pub fn_set_face_calibration_landmarks: Option<FnSetFaceCalibrationLandmarks>,
    pub fn_set_silhouette_mode: Option<FnSetSilhouetteMode>,
    pub fn_get_silhouette_mode: Option<FnGetSilhouetteMode>,
    pub fn_set_multi_person_mode: Option<FnSetMultiPersonMode>,
    pub fn_cam_count: Option<FnWinGetCameraCount>,
    pub fn_cam_info: Option<FnWinGetCameraInfo>,
    pub fn_cam_create: Option<FnWinCameraCreate>,
    pub fn_cam_open: Option<FnWinCameraOpen>,
    pub fn_cam_read: Option<FnWinCameraReadFrame>,
    pub fn_cam_close: Option<FnWinCameraClose>,
    pub fn_cam_destroy: Option<FnWinCameraDestroy>,
    pub fn_sink_create: Option<FnWinSinkCreate>,
    pub fn_sink_init: Option<FnWinSinkInit>,
    pub fn_sink_push: Option<FnWinSinkPushFrame>,
    pub fn_sink_active: Option<FnWinSinkIsConsumerActive>,
    pub fn_sink_destroy: Option<FnWinSinkDestroy>,
}

unsafe impl Send for NativeEngineLibrary {}
unsafe impl Sync for NativeEngineLibrary {}

impl NativeEngineLibrary {
    pub fn load_best(repo_root: &Path, preferred_variant: &str, simd_pref: &str) -> Option<Self> {
        let build_root = repo_root.join("platforms").join("windows").join("build");
        let avx512_ok = cpu_supports_avx512();

        let want_avx512 = preferred_variant == "out_avx512"
            || simd_pref == "avx512"
            || (preferred_variant != "out_avx2" && simd_pref == "auto" && avx512_ok);

        let bin_dir = repo_root.join("bin");
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| repo_root.to_path_buf());

        let mut candidates: Vec<(String, PathBuf)> = Vec::new();
        let avx512_paths = [
            build_root.join("out_avx512").join("smartcam_engine.dll"),
            build_root.join("out_avx512").join("common_engine").join("smartcam_engine.dll"),
            build_root.join("out_avx512").join("smartcam_win_bridge.dll"),
            bin_dir.join("smartcam_engine_avx512.dll"),
            exe_dir.join("smartcam_engine_avx512.dll"),
        ];
        let avx2_paths = [
            build_root.join("out_avx2").join("smartcam_engine.dll"),
            build_root.join("out_avx2").join("common_engine").join("smartcam_engine.dll"),
            build_root.join("out_avx2").join("smartcam_win_bridge.dll"),
            bin_dir.join("smartcam_engine_avx2.dll"),
            bin_dir.join("smartcam_engine.dll"),
            bin_dir.join("smartcam_win_bridge.dll"),
            exe_dir.join("smartcam_engine_avx2.dll"),
            exe_dir.join("smartcam_engine.dll"),
            exe_dir.join("smartcam_win_bridge.dll"),
        ];

        if want_avx512 && avx512_ok {
            for p in &avx512_paths {
                candidates.push(("out_avx512".to_string(), p.clone()));
            }
            for p in &avx2_paths {
                candidates.push(("out_avx2".to_string(), p.clone()));
            }
        } else {
            for p in &avx2_paths {
                candidates.push(("out_avx2".to_string(), p.clone()));
            }
            if avx512_ok {
                for p in &avx512_paths {
                    candidates.push(("out_avx512".to_string(), p.clone()));
                }
            }
        }

        for (variant, path) in candidates {
            if !path.is_file() {
                continue;
            }
            unsafe {
                let wpath = to_wstring(&path.to_string_lossy());
                let hmod = LoadLibraryW(wpath.as_ptr());
                if hmod.is_null() {
                    continue;
                }
                let fn_create = load_proc::<FnEngineCreate>(hmod, b"smartcam_engine_create\0");
                let fn_destroy = load_proc::<FnEngineDestroy>(hmod, b"smartcam_engine_destroy\0");
                if fn_create.is_none() || fn_destroy.is_none() {
                    FreeLibrary(hmod);
                    continue;
                }

                return Some(Self {
                    hmodule: hmod,
                    loaded_dll_path: path,
                    variant_name: variant,
                    fn_create,
                    fn_destroy,
                    fn_get_simd: load_proc(hmod, b"smartcam_engine_get_simd_backend\0"),
                    fn_cfg_gs: load_proc(hmod, b"smartcam_engine_configure_greenscreen\0"),
                    fn_autocal_gs: load_proc(hmod, b"smartcam_engine_autocalibrate_greenscreen\0"),
                    fn_crop_scale: load_proc(hmod, b"smartcam_engine_crop_scale_bgra\0"),
                    fn_gs_mask: load_proc(hmod, b"smartcam_engine_compute_greenscreen_mask\0"),
                    fn_ai_mask: load_proc(hmod, b"smartcam_engine_compute_ai_mask\0"),
                    fn_fill_solid: load_proc(hmod, b"smartcam_engine_fill_solid_bg\0"),
                    fn_blur_bg: load_proc(hmod, b"smartcam_engine_generate_blurred_bg\0"),
                    fn_blend: load_proc(hmod, b"smartcam_engine_blend_bgra\0"),
                    fn_set_ref_bg: load_proc(hmod, b"smartcam_engine_set_reference_background\0"),
                    fn_clear_ref_bg: load_proc(hmod, b"smartcam_engine_clear_reference_background\0"),
                    fn_has_ref_bg: load_proc(hmod, b"smartcam_engine_has_reference_background\0"),
                    fn_set_ai_mode_tol: load_proc(hmod, b"smartcam_engine_set_ai_mode_and_tolerance\0"),
                    fn_get_subj_center: load_proc(hmod, b"smartcam_engine_get_subject_center\0"),
                    fn_set_hardware_policy: load_proc(hmod, b"smartcam_engine_set_hardware_policy\0"),
                    fn_get_compute_backend: load_proc(hmod, b"smartcam_engine_get_active_compute_backend\0"),
                    fn_get_compute_device_name: load_proc(hmod, b"smartcam_engine_get_active_compute_device_name\0"),
                    fn_is_ai_enabled: load_proc(hmod, b"smartcam_engine_is_ai_enabled\0"),
                    fn_calibrate_face_landmarks: load_proc(hmod, b"smartcam_engine_calibrate_face_landmarks\0"),
                    fn_set_face_calibration_profile: load_proc(hmod, b"smartcam_engine_set_face_calibration_profile\0"),
                    fn_set_face_calibration_landmarks: load_proc(hmod, b"smartcam_engine_set_face_calibration_landmarks\0"),
                    fn_set_silhouette_mode: load_proc(hmod, b"smartcam_engine_set_silhouette_mode\0"),
                    fn_get_silhouette_mode: load_proc(hmod, b"smartcam_engine_get_silhouette_mode\0"),
                    fn_set_multi_person_mode: load_proc(hmod, b"smartcam_engine_set_multi_person_mode\0"),
                    fn_cam_count: load_proc(hmod, b"smartcam_win_get_camera_count\0"),
                    fn_cam_info: load_proc(hmod, b"smartcam_win_get_camera_info\0"),
                    fn_cam_create: load_proc(hmod, b"smartcam_win_camera_create\0"),
                    fn_cam_open: load_proc(hmod, b"smartcam_win_camera_open\0"),
                    fn_cam_read: load_proc(hmod, b"smartcam_win_camera_read_frame\0"),
                    fn_cam_close: load_proc(hmod, b"smartcam_win_camera_close\0"),
                    fn_cam_destroy: load_proc(hmod, b"smartcam_win_camera_destroy\0"),
                    fn_sink_create: load_proc(hmod, b"smartcam_win_sink_create\0"),
                    fn_sink_init: load_proc(hmod, b"smartcam_win_sink_init\0"),
                    fn_sink_push: load_proc(hmod, b"smartcam_win_sink_push_frame\0"),
                    fn_sink_active: load_proc(hmod, b"smartcam_win_sink_is_consumer_active\0"),
                    fn_sink_destroy: load_proc(hmod, b"smartcam_win_sink_destroy\0"),
                });
            }
        }
        None
    }

    pub fn enumerate_cameras(&self) -> Vec<CameraDeviceEntry> {
        let mut list = Vec::new();
        unsafe {
            if let (Some(fn_count), Some(fn_info)) = (self.fn_cam_count, self.fn_cam_info) {
                let count = fn_count();
                for idx in 0..count {
                    let mut name_buf = [0u8; 256];
                    let mut sym_buf = [0u8; 512];
                    let mut is_ir: i32 = 0;
                    if fn_info(
                        idx,
                        name_buf.as_mut_ptr() as *mut c_char,
                        name_buf.len() as u32,
                        sym_buf.as_mut_ptr() as *mut c_char,
                        sym_buf.len() as u32,
                        &mut is_ir,
                    ) == 1
                    {
                        let name = CStr::from_ptr(name_buf.as_ptr() as *const c_char)
                            .to_string_lossy()
                            .to_string();
                        let symlink = CStr::from_ptr(sym_buf.as_ptr() as *const c_char)
                            .to_string_lossy()
                            .to_string();
                        // Skip our own virtual camera if enumerated
                        if name.eq_ignore_ascii_case("SmartCam")
                            || name.contains("SmartCam Virtual Camera")
                        {
                            continue;
                        }
                        let ir = is_ir != 0 || is_ir_camera_name(&name, &symlink);
                        list.push(CameraDeviceEntry {
                            index: idx,
                            name,
                            symbolic_link: symlink,
                            is_infrared: ir,
                        });
                    }
                }
            }
        }
        list
    }
}

pub struct StudioPresetThumb {
    pub id: u8,
    pub label: String,
    pub rel_path: String,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct SmartCamPipelineInstance {
    pub repo_root: PathBuf,
    pub lib: Option<NativeEngineLibrary>,
    pub engine_handle: *mut c_void,
    pub cam_handle: *mut c_void,
    pub sink_handle: *mut c_void,
    pub simd_backend_label: String,
    pub compute_backend_label: String,
    pub compute_device_label: String,
    pub active_ai_enabled: bool,
    pub camera_open: bool,
    pub camera_busy_error: bool,
    pub fallback_shm: Option<FallbackSharedMemoryWriter>,
    pub framed_buf: Vec<u8>,
    pub bg_buf: Vec<u8>,
    pub mask_buf: Vec<u8>,
    pub out_buf: Vec<u8>,
    pub prev_ai_prob: Vec<f32>,
    // Reference Background & Subject Tracking state
    pub ref_bg_buf: Vec<u8>,
    pub ref_bg_w: u32,
    pub ref_bg_h: u32,
    pub has_ref_bg_fallback: bool,
    pub tracked_subject_center: (f32, f32),
    pub multi_person_mode_active: bool,
    // Custom / Studio Background Image & Social Crop/Zoom state
    pub bg_source_image: Vec<u8>,
    pub bg_source_w: u32,
    pub bg_source_h: u32,
    pub bg_cropped_cache: Vec<u8>,
    pub bg_cropped_w: u32,
    pub bg_cropped_h: u32,
    pub bg_cropped_zoom: f32,
    pub bg_cropped_pan_x: f32,
    pub bg_cropped_pan_y: f32,
    pub bg_cropped_mirror_h: bool,
    pub bg_cropped_mirror_v: bool,
    pub bg_loaded_path: String,
    pub studio_preset_thumbs: Vec<StudioPresetThumb>,
}

unsafe impl Send for SmartCamPipelineInstance {}

impl SmartCamPipelineInstance {
    pub fn new(repo_root: &Path, cfg: &SmartCamConfig, preferred_variant: &str) -> Self {
        let lib = NativeEngineLibrary::load_best(
            repo_root,
            preferred_variant,
            &cfg.compositor.simd_acceleration,
        );

        let mut engine_handle = std::ptr::null_mut();
        let mut simd_backend_label = "Rust Vectorized Fallback".to_string();

        if let Some(ref l) = lib {
            unsafe {
                let tier = match cfg.compositor.simd_acceleration.as_str() {
                    "scalar" => 1,
                    "avx2" => 2,
                    "avx512" => 3,
                    _ => 0,
                };
                if let Some(f_create) = l.fn_create {
                    engine_handle = f_create(tier);
                }
                if !engine_handle.is_null() {
                    if let Some(f_simd) = l.fn_get_simd {
                        let ptr = f_simd(engine_handle);
                        if !ptr.is_null() {
                            let s = CStr::from_ptr(ptr).to_string_lossy();
                            simd_backend_label = format!("C++ {} ({})", l.variant_name, s);
                        }
                    }
                }
            }
        }

        let preset_defs = [
            (1u8, "🏢 Studio Blu", "assets/backgrounds/preset_1_studio_blue.jpg"),
            (2u8, "🪵 Ufficio Caldo", "assets/backgrounds/preset_2_warm_office.jpg"),
            (3u8, "☀️ Loft Chiaro", "assets/backgrounds/preset_3_minimal_loft.jpg"),
            (4u8, "🏛️ Executive Dark", "assets/backgrounds/preset_4_dark_executive.jpg"),
        ];
        let mut studio_preset_thumbs = Vec::with_capacity(4);
        for (pid, plbl, ppath) in preset_defs {
            let (px, tw, th) = generate_studio_preset_bgra(pid, 160, 90);
            studio_preset_thumbs.push(StudioPresetThumb {
                id: pid,
                label: plbl.to_string(),
                rel_path: ppath.to_string(),
                pixels: px,
                width: tw,
                height: th,
            });
        }

        let mut inst = Self {
            repo_root: repo_root.to_path_buf(),
            lib,
            engine_handle,
            cam_handle: std::ptr::null_mut(),
            sink_handle: std::ptr::null_mut(),
            simd_backend_label,
            compute_backend_label: "CPU".to_string(),
            compute_device_label: String::new(),
            active_ai_enabled: cfg.ai_enabled,
            camera_open: false,
            camera_busy_error: false,
            fallback_shm: None,
            framed_buf: Vec::new(),
            bg_buf: Vec::new(),
            mask_buf: Vec::new(),
            out_buf: Vec::new(),
            prev_ai_prob: Vec::new(),
            ref_bg_buf: Vec::new(),
            ref_bg_w: 0,
            ref_bg_h: 0,
            has_ref_bg_fallback: false,
            tracked_subject_center: (0.50, 0.52),
            multi_person_mode_active: cfg.is_plugin_enabled("multi_person_tracker"),
            bg_source_image: Vec::new(),
            bg_source_w: 0,
            bg_source_h: 0,
            bg_cropped_cache: Vec::new(),
            bg_cropped_w: 0,
            bg_cropped_h: 0,
            bg_cropped_zoom: 1.0,
            bg_cropped_pan_x: 0.5,
            bg_cropped_pan_y: 0.5,
            bg_cropped_mirror_h: cfg.compositor.background_mirror_h,
            bg_cropped_mirror_v: cfg.compositor.background_mirror_v,
            bg_loaded_path: String::new(),
            studio_preset_thumbs,
        };
        inst.update_greenscreen_config(cfg);
        inst.sync_face_and_silhouette_settings(cfg, inst.multi_person_mode_active);
        inst.try_restore_face_calibration_snapshot(cfg);
        let _ = inst.load_or_generate_background_image(&cfg.compositor.background_image_path);
        inst.init_sink(cfg);
        inst
    }

    pub fn try_restore_face_calibration_snapshot(&mut self, cfg: &SmartCamConfig) {
        if !cfg.segmentation.ai_selfie.face_calibrated {
            return;
        }
        let snap_path = self.repo_root.join("config").join("face_calib_snapshot.bin");
        if let Ok(bytes) = std::fs::read(&snap_path) {
            if bytes.len() >= 8 {
                let sw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let sh = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                let expected = 8usize + (sw as usize) * (sh as usize) * 4;
                if sw >= 64 && sh >= 36 && sw <= 3840 && sh <= 2160 && bytes.len() >= expected {
                    let pts = cfg.segmentation.ai_selfie.calib_landmarks;
                    if let Some(ref l) = self.lib {
                        if !self.engine_handle.is_null() {
                            if let Some(f_cal) = l.fn_calibrate_face_landmarks {
                                let mut out_cb = cfg.segmentation.ai_selfie.calib_skin_cb;
                                let mut out_cr = cfg.segmentation.ai_selfie.calib_skin_cr;
                                let mut out_rx = cfg.segmentation.ai_selfie.calib_head_rx;
                                let mut out_ry = cfg.segmentation.ai_selfie.calib_head_ry;
                                unsafe {
                                    let _ = f_cal(
                                        self.engine_handle,
                                        bytes[8..expected].as_ptr(),
                                        sw,
                                        sh,
                                        sw * 4,
                                        pts[0].0,
                                        pts[0].1,
                                        pts[1].0,
                                        pts[1].1,
                                        pts[2].0,
                                        pts[2].1,
                                        pts[3].0,
                                        pts[3].1,
                                        pts[4].0,
                                        pts[4].1,
                                        &mut out_cb,
                                        &mut out_cr,
                                        &mut out_rx,
                                        &mut out_ry,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn apply_hardware_policy(&mut self, cfg: &SmartCamConfig) {
        self.active_ai_enabled = cfg.ai_enabled;
        let backend_id = crate::benchmark::compute_backend_str_to_id(&cfg.compute_backend);
        let simd_id = crate::benchmark::simd_mode_str_to_policy_id(&cfg.compositor.simd_acceleration);

        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                unsafe {
                    if let Some(f_policy) = l.fn_set_hardware_policy {
                        let _ = f_policy(
                            self.engine_handle,
                            backend_id,
                            simd_id,
                            if cfg.ai_enabled { 1 } else { 0 },
                        );
                    }
                    let mut simd_s = "AVX2".to_string();
                    if let Some(f_simd) = l.fn_get_simd {
                        let ptr = f_simd(self.engine_handle);
                        if !ptr.is_null() {
                            simd_s = CStr::from_ptr(ptr).to_string_lossy().to_string();
                        }
                    }
                    let mut backend_s = crate::benchmark::compute_backend_id_to_display(backend_id).to_string();
                    if let Some(f_back) = l.fn_get_compute_backend {
                        let ptr = f_back(self.engine_handle);
                        if !ptr.is_null() {
                            backend_s = CStr::from_ptr(ptr).to_string_lossy().to_string();
                        }
                    }
                    let mut dev_s = String::new();
                    if let Some(f_dev) = l.fn_get_compute_device_name {
                        let ptr = f_dev(self.engine_handle);
                        if !ptr.is_null() {
                            dev_s = CStr::from_ptr(ptr).to_string_lossy().to_string();
                        }
                    }
                    if let Some(f_ai) = l.fn_is_ai_enabled {
                        self.active_ai_enabled = f_ai(self.engine_handle) != 0;
                    }
                    self.compute_backend_label = backend_s.clone();
                    self.compute_device_label = dev_s.clone();
                    if backend_s.eq_ignore_ascii_case("CPU") || backend_s.eq_ignore_ascii_case("Auto") {
                        self.simd_backend_label = format!("CPU {} ({})", simd_s, l.variant_name);
                    } else if !dev_s.is_empty() {
                        self.simd_backend_label = format!("GPU {} — {} (+{})", backend_s, dev_s, simd_s);
                    } else {
                        self.simd_backend_label = format!("GPU {} (+{})", backend_s, simd_s);
                    }
                    return;
                }
            }
        }
        let simd_disp = crate::benchmark::simd_policy_id_to_display(simd_id);
        let back_disp = crate::benchmark::compute_backend_id_to_display(backend_id);
        self.compute_backend_label = back_disp.to_string();
        self.simd_backend_label = format!("Rust {} ({})", back_disp, simd_disp);
    }

    pub fn update_greenscreen_config(&mut self, cfg: &SmartCamConfig) {
        self.apply_hardware_policy(cfg);
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_cfg) = l.fn_cfg_gs {
                    let gs = &cfg.segmentation.green_screen;
                    unsafe {
                        f_cfg(
                            self.engine_handle,
                            gs.hue_min,
                            gs.hue_max,
                            gs.sat_min,
                            gs.sat_max,
                            gs.val_min,
                            gs.val_max,
                            gs.morph_kernel_size,
                            gs.blur_radius,
                            gs.spill_suppression,
                        );
                    }
                }
                if let Some(f_ai_tol) = l.fn_set_ai_mode_tol {
                    unsafe {
                        f_ai_tol(
                            self.engine_handle,
                            cfg.segmentation.ai_selfie.engine_mode,
                            cfg.segmentation.ai_selfie.bg_diff_tolerance,
                        );
                    }
                }
            }
        }
        let multi_active = self.multi_person_mode_active || cfg.is_plugin_enabled("multi_person_tracker");
        self.sync_face_and_silhouette_settings(cfg, multi_active);
    }

    pub fn sync_face_and_silhouette_settings(&mut self, cfg: &SmartCamConfig, multi_person_active: bool) {
        self.multi_person_mode_active = multi_person_active;
        let ai = &cfg.segmentation.ai_selfie;
        if ai.face_calibrated {
            let pts = ai.calib_landmarks;
            let eye_mid_x = 0.5 * (pts[0].0 + pts[1].0);
            let eye_mid_y = 0.5 * (pts[0].1 + pts[1].1);
            let eye_to_chin = (pts[4].1 - eye_mid_y).abs().max(0.055);
            let head_cx = (0.30 * eye_mid_x + 0.35 * pts[2].0 + 0.20 * pts[3].0 + 0.15 * pts[4].0).clamp(0.12, 0.88);
            let head_cy = (eye_mid_y + 0.105 * eye_to_chin).clamp(0.14, 0.82);
            self.tracked_subject_center = (head_cx, head_cy);
        }
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                unsafe {
                    if let Some(f_cal_lm) = l.fn_set_face_calibration_landmarks {
                        let flat_lm: [f32; 10] = [
                            ai.calib_landmarks[0].0,
                            ai.calib_landmarks[0].1,
                            ai.calib_landmarks[1].0,
                            ai.calib_landmarks[1].1,
                            ai.calib_landmarks[2].0,
                            ai.calib_landmarks[2].1,
                            ai.calib_landmarks[3].0,
                            ai.calib_landmarks[3].1,
                            ai.calib_landmarks[4].0,
                            ai.calib_landmarks[4].1,
                        ];
                        let _ = f_cal_lm(self.engine_handle, flat_lm.as_ptr());
                    }
                    if let Some(f_cal_prof) = l.fn_set_face_calibration_profile {
                        let _ = f_cal_prof(
                            self.engine_handle,
                            if ai.face_calibrated { 1 } else { 0 },
                            ai.calib_skin_cb,
                            ai.calib_skin_cr,
                            ai.calib_head_rx,
                            ai.calib_head_ry,
                        );
                    }
                    if let Some(f_sil) = l.fn_set_silhouette_mode {
                        let _ = f_sil(self.engine_handle, ai.silhouette_mode.clamp(0, 1));
                    }
                    if let Some(f_mp) = l.fn_set_multi_person_mode {
                        let _ = f_mp(self.engine_handle, if multi_person_active { 1 } else { 0 });
                    }
                }
            }
        }
    }

    pub fn calibrate_face_from_landmarks(
        &mut self,
        snapshot_bgra: &[u8],
        w: u32,
        h: u32,
        pts: [(f32, f32); 5],
        cfg: &mut SmartCamConfig,
    ) -> bool {
        if w == 0 || h == 0 || snapshot_bgra.len() < (w * h * 4) as usize {
            return false;
        }

        // Persist a compact 256x144 BGRA calibration snapshot so the 5-point visual template survives app restarts
        let snap_w = 256u32;
        let snap_h = 144u32;
        let mut snap_file_bytes = Vec::with_capacity(8 + (snap_w * snap_h * 4) as usize);
        snap_file_bytes.extend_from_slice(&snap_w.to_le_bytes());
        snap_file_bytes.extend_from_slice(&snap_h.to_le_bytes());
        for gy in 0..snap_h {
            let sy = ((gy * h) / snap_h).min(h - 1);
            for gx in 0..snap_w {
                let sx = ((gx * w) / snap_w).min(w - 1);
                let off = ((sy * w + sx) * 4) as usize;
                snap_file_bytes.extend_from_slice(&snapshot_bgra[off..off + 4]);
            }
        }
        let snap_path = self.repo_root.join("config").join("face_calib_snapshot.bin");
        let _ = std::fs::write(&snap_path, &snap_file_bytes);

        // 1. Pure-Rust anatomical & YCbCr skin chrominance computation (fallback & baseline)
        let (mut skin_cb, mut skin_cr, mut head_rx, mut head_ry) =
            rust_compute_face_calibration_from_landmarks(snapshot_bgra, w, h, pts);

        // 2. Call native C++ engine calibration if symbol is available in DLL
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_cal) = l.fn_calibrate_face_landmarks {
                    let mut out_cb = skin_cb;
                    let mut out_cr = skin_cr;
                    let mut out_rx = head_rx;
                    let mut out_ry = head_ry;
                    unsafe {
                        if f_cal(
                            self.engine_handle,
                            snapshot_bgra.as_ptr(),
                            w,
                            h,
                            w * 4,
                            pts[0].0,
                            pts[0].1,
                            pts[1].0,
                            pts[1].1,
                            pts[2].0,
                            pts[2].1,
                            pts[3].0,
                            pts[3].1,
                            pts[4].0,
                            pts[4].1,
                            &mut out_cb,
                            &mut out_cr,
                            &mut out_rx,
                            &mut out_ry,
                        ) != 0
                        {
                            skin_cb = out_cb;
                            skin_cr = out_cr;
                            head_rx = out_rx;
                            head_ry = out_ry;
                        }
                    }
                }
            }
        }

        cfg.segmentation.ai_selfie.face_calibrated = true;
        cfg.segmentation.ai_selfie.calib_skin_cb = skin_cb.clamp(60.0, 175.0);
        cfg.segmentation.ai_selfie.calib_skin_cr = skin_cr.clamp(115.0, 210.0);
        cfg.segmentation.ai_selfie.calib_head_rx = head_rx.clamp(0.04, 0.40);
        cfg.segmentation.ai_selfie.calib_head_ry = head_ry.clamp(0.06, 0.48);
        cfg.segmentation.ai_selfie.calib_landmarks = pts;

        let eye_mid_x = 0.5 * (pts[0].0 + pts[1].0);
        let eye_mid_y = 0.5 * (pts[0].1 + pts[1].1);
        let eye_to_chin = (pts[4].1 - eye_mid_y).abs().max(0.055);
        let face_cx = (0.30 * eye_mid_x + 0.35 * pts[2].0 + 0.20 * pts[3].0 + 0.15 * pts[4].0).clamp(0.12, 0.88);
        let face_cy = (eye_mid_y + 0.105 * eye_to_chin).clamp(0.14, 0.82);
        self.tracked_subject_center = (face_cx, face_cy);

        let multi_active = self.multi_person_mode_active || cfg.is_plugin_enabled("multi_person_tracker");
        self.sync_face_and_silhouette_settings(cfg, multi_active);
        true
    }

    pub fn init_sink(&mut self, cfg: &SmartCamConfig) {
        let out_w = cfg.framing_output.output_width;
        let out_h = cfg.framing_output.output_height;
        let fps = cfg.camera_input.fps;

        if let Some(ref l) = self.lib {
            unsafe {
                if let (Some(f_create), Some(f_init)) = (l.fn_sink_create, l.fn_sink_init) {
                    let h = f_create();
                    if !h.is_null() {
                        let cname = CString::new(cfg.virtual_camera.friendly_name.as_str())
                            .unwrap_or_else(|_| CString::new("SmartCam").unwrap());
                        if f_init(h, cname.as_ptr(), out_w, out_h, fps) == 1 {
                            self.sink_handle = h;
                            return;
                        }
                    }
                }
            }
        }
        self.fallback_shm = FallbackSharedMemoryWriter::open(&cfg.virtual_camera.shared_memory_name);
    }

    pub fn open_hardware_camera(&mut self, index: u32, width: u32, height: u32, fps: u32) -> bool {
        self.close_hardware_camera();
        self.camera_busy_error = false;
        if let Some(ref l) = self.lib {
            unsafe {
                if let (Some(f_create), Some(f_open)) = (l.fn_cam_create, l.fn_cam_open) {
                    let h = f_create();
                    if !h.is_null() {
                        if f_open(h, index, width, height, fps) == 1 {
                            self.cam_handle = h;
                            self.camera_open = true;
                            return true;
                        } else {
                            if let Some(f_destroy) = l.fn_cam_destroy {
                                f_destroy(h);
                            }
                            self.camera_busy_error = true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn close_hardware_camera(&mut self) {
        if !self.cam_handle.is_null() {
            if let Some(ref l) = self.lib {
                unsafe {
                    if let Some(f_close) = l.fn_cam_close {
                        f_close(self.cam_handle);
                    }
                    if let Some(f_destroy) = l.fn_cam_destroy {
                        f_destroy(self.cam_handle);
                    }
                }
            }
            self.cam_handle = std::ptr::null_mut();
        }
        self.camera_open = false;
    }

    pub fn read_camera_frame(&mut self, dst: &mut Vec<u8>, target_w: u32, target_h: u32) -> Option<(u32, u32)> {
        if !self.camera_open || self.cam_handle.is_null() {
            return None;
        }
        let max_bytes = (target_w.max(1920) * target_h.max(1080) * 4) as usize;
        if dst.len() < max_bytes {
            dst.resize(max_bytes, 0);
        }
        if let Some(ref l) = self.lib {
            if let Some(f_read) = l.fn_cam_read {
                let mut actual_w: u32 = 0;
                let mut actual_h: u32 = 0;
                unsafe {
                    if f_read(
                        self.cam_handle,
                        dst.as_mut_ptr(),
                        dst.len() as u32,
                        &mut actual_w,
                        &mut actual_h,
                    ) == 1
                        && actual_w > 0
                        && actual_h > 0
                    {
                        return Some((actual_w, actual_h));
                    }
                }
            }
        }
        None
    }

    pub fn is_consumer_active(&self) -> bool {
        if !self.sink_handle.is_null() {
            if let Some(ref l) = self.lib {
                if let Some(f_active) = l.fn_sink_active {
                    unsafe {
                        return f_active(self.sink_handle) != 0;
                    }
                }
            }
        }
        false
    }

    pub fn push_frame_to_sink(&mut self, bgra: &[u8], width: u32, height: u32, fps: u32) {
        if !self.sink_handle.is_null() {
            if let Some(ref l) = self.lib {
                if let Some(f_push) = l.fn_sink_push {
                    unsafe {
                        let _ = f_push(self.sink_handle, bgra.as_ptr(), width, height, width * 4);
                        return;
                    }
                }
            }
        }
        if let Some(ref mut shm) = self.fallback_shm {
            shm.write_frame(bgra, width, height, fps);
        }
    }

    pub fn auto_calibrate_greenscreen(
        &mut self,
        frame_bgra: &[u8],
        width: u32,
        height: u32,
        sample_x: i32,
        sample_y: i32,
        cfg: &mut SmartCamConfig,
    ) -> (u8, u8, u8, u8) {
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_auto) = l.fn_autocal_gs {
                    let mut hmin = cfg.segmentation.green_screen.hue_min;
                    let mut hmax = cfg.segmentation.green_screen.hue_max;
                    let mut smin = cfg.segmentation.green_screen.sat_min;
                    let mut vmin = cfg.segmentation.green_screen.val_min;
                    unsafe {
                        if f_auto(
                            self.engine_handle,
                            frame_bgra.as_ptr(),
                            width,
                            height,
                            width * 4,
                            sample_x,
                            sample_y,
                            &mut hmin,
                            &mut hmax,
                            &mut smin,
                            &mut vmin,
                        ) == 1
                        {
                            cfg.segmentation.green_screen.hue_min = hmin;
                            cfg.segmentation.green_screen.hue_max = hmax;
                            cfg.segmentation.green_screen.sat_min = smin;
                            cfg.segmentation.green_screen.val_min = vmin;
                            return (hmin, hmax, smin, vmin);
                        }
                    }
                }
            }
        }

        // Pure Rust HSV auto-calibration fallback (samples patch or upper corners)
        let (h, s, v) = sample_hsv_from_frame(frame_bgra, width, height, sample_x, sample_y);
        let hmin = h.saturating_sub(18).min(170);
        let hmax = (h + 18).min(180).max(hmin + 6);
        let smin = s.saturating_sub(45).clamp(25, 200);
        let vmin = v.saturating_sub(55).clamp(25, 200);
        cfg.segmentation.green_screen.hue_min = hmin;
        cfg.segmentation.green_screen.hue_max = hmax;
        cfg.segmentation.green_screen.sat_min = smin;
        cfg.segmentation.green_screen.val_min = vmin;
        self.update_greenscreen_config(cfg);
        (hmin, hmax, smin, vmin)
    }

    pub fn set_reference_background(&mut self, bg_bgra: &[u8], width: u32, height: u32) -> bool {
        let needed = (width * height * 4) as usize;
        if width == 0 || height == 0 || bg_bgra.len() < needed {
            return false;
        }
        self.ref_bg_buf.clear();
        self.ref_bg_buf.extend_from_slice(&bg_bgra[..needed]);
        self.ref_bg_w = width;
        self.ref_bg_h = height;
        self.has_ref_bg_fallback = true;

        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_set) = l.fn_set_ref_bg {
                    unsafe {
                        let _ = f_set(
                            self.engine_handle,
                            bg_bgra.as_ptr(),
                            width,
                            height,
                            width * 4,
                        );
                    }
                }
            }
        }
        true
    }

    pub fn clear_reference_background(&mut self) {
        self.ref_bg_buf.clear();
        self.ref_bg_w = 0;
        self.ref_bg_h = 0;
        self.has_ref_bg_fallback = false;

        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_clear) = l.fn_clear_ref_bg {
                    unsafe {
                        f_clear(self.engine_handle);
                    }
                }
            }
        }
    }

    pub fn has_reference_background(&self) -> bool {
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_has) = l.fn_has_ref_bg {
                    unsafe {
                        if f_has(self.engine_handle) != 0 {
                            return true;
                        }
                    }
                }
            }
        }
        self.has_ref_bg_fallback
    }

    pub fn get_subject_center(&mut self) -> (f32, f32) {
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_center) = l.fn_get_subj_center {
                    let mut cx: f32 = self.tracked_subject_center.0;
                    let mut cy: f32 = self.tracked_subject_center.1;
                    unsafe {
                        if f_center(self.engine_handle, &mut cx, &mut cy) != 0 {
                            self.tracked_subject_center = (cx.clamp(0.1, 0.9), cy.clamp(0.1, 0.9));
                            return self.tracked_subject_center;
                        }
                    }
                }
            }
        }
        self.tracked_subject_center
    }

    pub fn load_or_generate_background_image(&mut self, path_str: &str) -> bool {
        let effective = if path_str.trim().is_empty() {
            "assets/backgrounds/preset_1_studio_blue.jpg"
        } else {
            path_str.trim()
        };

        let candidate_path = {
            let p = Path::new(effective);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.repo_root.join(p)
            }
        };

        if let Some((pixels, w, h)) = decode_image_file_gdiplus(&candidate_path) {
            self.bg_source_image = pixels;
            self.bg_source_w = w;
            self.bg_source_h = h;
            self.bg_loaded_path = effective.to_string();
            self.bg_cropped_w = 0; // force crop cache rebuild
            return true;
        }

        // Fallback: generate a high-definition studio photography preset in memory
        let preset_id = if effective.contains("preset_2") || effective.contains("warm") {
            2
        } else if effective.contains("preset_3") || effective.contains("minimal") || effective.contains("loft") {
            3
        } else if effective.contains("preset_4") || effective.contains("dark") || effective.contains("executive") {
            4
        } else {
            1
        };
        let (pixels, w, h) = generate_studio_preset_bgra(preset_id, 1920, 1080);
        self.bg_source_image = pixels;
        self.bg_source_w = w;
        self.bg_source_h = h;
        self.bg_loaded_path = effective.to_string();
        self.bg_cropped_w = 0;
        true
    }

    pub fn rebuild_cropped_background(
        &mut self,
        out_w: u32,
        out_h: u32,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
        mirror_h: bool,
        mirror_v: bool,
    ) {
        if self.bg_source_image.is_empty() || self.bg_source_w == 0 || self.bg_source_h == 0 {
            let _ = self.load_or_generate_background_image("assets/backgrounds/preset_1_studio_blue.jpg");
        }
        let needed = (out_w * out_h * 4) as usize;
        if self.bg_cropped_cache.len() != needed {
            self.bg_cropped_cache.resize(needed, 255);
        }
        crop_zoom_image_social(
            &self.bg_source_image,
            self.bg_source_w,
            self.bg_source_h,
            &mut self.bg_cropped_cache,
            out_w,
            out_h,
            zoom,
            pan_x,
            pan_y,
            mirror_h,
            mirror_v,
        );
        self.bg_cropped_w = out_w;
        self.bg_cropped_h = out_h;
        self.bg_cropped_zoom = zoom;
        self.bg_cropped_pan_x = pan_x;
        self.bg_cropped_pan_y = pan_y;
        self.bg_cropped_mirror_h = mirror_h;
        self.bg_cropped_mirror_v = mirror_v;
    }

    pub fn process_frame(
        &mut self,
        raw_bgra: &[u8],
        src_w: u32,
        src_h: u32,
        cfg: &SmartCamConfig,
    ) -> (u32, u32) {
        let phone_active = cfg.is_plugin_enabled("phone_camera_source");
        let (out_w, out_h, aw, ah) = if phone_active && src_w >= 64 && src_h >= 64 {
            let pw = (src_w & !1).max(64);
            let ph = (src_h & !1).max(64);
            (pw, ph, pw, ph)
        } else {
            let ow = cfg.framing_output.output_width.max(160);
            let oh = cfg.framing_output.output_height.max(120);
            let (a_w, a_h) = resolve_aspect_ratio_pair(&cfg.framing_output.aspect_ratio, src_w, src_h);
            (ow, oh, a_w, a_h)
        };
        let total_px = (out_w * out_h) as usize;
        let total_bytes = total_px * 4;

        if self.framed_buf.len() != total_bytes {
            self.framed_buf.resize(total_bytes, 255);
        }
        if self.bg_buf.len() != total_bytes {
            self.bg_buf.resize(total_bytes, 255);
        }
        if self.mask_buf.len() != total_px {
            self.mask_buf.resize(total_px, 255);
        }
        if self.out_buf.len() != total_bytes {
            self.out_buf.resize(total_bytes, 255);
        }

        let strategy_id = if cfg.framing_output.crop_strategy == "letterbox" { 1 } else { 0 };

        // 1. Crop & Scale
        let mut crop_ok = false;
        if let Some(ref l) = self.lib {
            if let Some(f_crop) = l.fn_crop_scale {
                unsafe {
                    crop_ok = f_crop(
                        raw_bgra.as_ptr(),
                        src_w,
                        src_h,
                        src_w * 4,
                        self.framed_buf.as_mut_ptr(),
                        out_w,
                        out_h,
                        out_w * 4,
                        aw,
                        ah,
                        strategy_id,
                    ) == 1;
                }
            }
        }
        if !crop_ok {
            rust_crop_and_scale(raw_bgra, src_w, src_h, &mut self.framed_buf, out_w, out_h, aw, ah);
        }

        // Passthrough shortcut
        if cfg.segmentation.mode == "passthrough" {
            self.out_buf.copy_from_slice(&self.framed_buf);
            return (out_w, out_h);
        }

        // 2. Prepare Background Buffer (Image vs Blur vs Solid Color)
        let mut bg_ok = false;
        if cfg.compositor.background_mode == "image" {
            let want_path = if cfg.compositor.background_image_path.is_empty() {
                "assets/backgrounds/preset_1_studio_blue.jpg"
            } else {
                cfg.compositor.background_image_path.as_str()
            };
            if self.bg_loaded_path != want_path || self.bg_source_image.is_empty() {
                let _ = self.load_or_generate_background_image(want_path);
            }
            let z = cfg.compositor.background_zoom.clamp(1.0, 4.0);
            let px = cfg.compositor.background_pan_x.clamp(0.0, 1.0);
            let py = cfg.compositor.background_pan_y.clamp(0.0, 1.0);
            let mh = cfg.compositor.background_mirror_h;
            let mv = cfg.compositor.background_mirror_v;
            if self.bg_cropped_w != out_w
                || self.bg_cropped_h != out_h
                || (self.bg_cropped_zoom - z).abs() > 0.001
                || (self.bg_cropped_pan_x - px).abs() > 0.001
                || (self.bg_cropped_pan_y - py).abs() > 0.001
                || self.bg_cropped_mirror_h != mh
                || self.bg_cropped_mirror_v != mv
                || self.bg_cropped_cache.len() != total_bytes
            {
                self.rebuild_cropped_background(out_w, out_h, z, px, py, mh, mv);
            }
            if self.bg_cropped_cache.len() == total_bytes {
                self.bg_buf.copy_from_slice(&self.bg_cropped_cache);
                bg_ok = true;
            }
        } else if cfg.compositor.background_mode == "blur" {
            let blur_r = cfg.compositor.background_blur_radius.clamp(2, 40);
            if let Some(ref l) = self.lib {
                if let Some(f_blur) = l.fn_blur_bg {
                    unsafe {
                        bg_ok = f_blur(
                            self.framed_buf.as_ptr(),
                            self.bg_buf.as_mut_ptr(),
                            out_w,
                            out_h,
                            out_w * 4,
                            blur_r,
                        ) == 1;
                    }
                }
            }
            if !bg_ok {
                rust_fast_box_blur(&self.framed_buf, &mut self.bg_buf, out_w, out_h);
                bg_ok = true;
            }
        }
        if !bg_ok {
            let [r, g, b] = cfg.compositor.background_color_rgb;
            let mut solid_ok = false;
            if let Some(ref l) = self.lib {
                if let Some(f_fill) = l.fn_fill_solid {
                    unsafe {
                        solid_ok = f_fill(self.bg_buf.as_mut_ptr(), out_w, out_h, out_w * 4, r, g, b) == 1;
                    }
                }
            }
            if !solid_ok {
                for px in self.bg_buf.chunks_exact_mut(4) {
                    px[0] = b;
                    px[1] = g;
                    px[2] = r;
                    px[3] = 255;
                }
            }
        }

        // 3. Compute Foreground Alpha Mask
        let mut mask_ok = false;
        if cfg.segmentation.mode == "green_screen" {
            if let Some(ref l) = self.lib {
                if !self.engine_handle.is_null() {
                    if let Some(f_gs) = l.fn_gs_mask {
                        unsafe {
                            mask_ok = f_gs(
                                self.engine_handle,
                                self.framed_buf.as_mut_ptr(),
                                out_w,
                                out_h,
                                out_w * 4,
                                self.mask_buf.as_mut_ptr(),
                                out_w,
                            ) == 1;
                        }
                    }
                }
            }
            if !mask_ok {
                rust_greenscreen_mask(&mut self.framed_buf, &mut self.mask_buf, out_w, out_h, cfg);
            }
        } else {
            // Mode 2: AI Selfie Segmentation (Dynamic Face/Torso Tracking or Reference Background Subtraction)
            let multi_active = self.multi_person_mode_active || cfg.is_plugin_enabled("multi_person_tracker");
            self.sync_face_and_silhouette_settings(cfg, multi_active);
            if let Some(ref l) = self.lib {
                if !self.engine_handle.is_null() {
                    if let Some(f_ai_tol) = l.fn_set_ai_mode_tol {
                        unsafe {
                            f_ai_tol(
                                self.engine_handle,
                                cfg.segmentation.ai_selfie.engine_mode,
                                cfg.segmentation.ai_selfie.bg_diff_tolerance,
                            );
                        }
                    }
                    if let Some(f_ai) = l.fn_ai_mask {
                        unsafe {
                            mask_ok = f_ai(
                                self.engine_handle,
                                self.framed_buf.as_ptr(),
                                out_w,
                                out_h,
                                out_w * 4,
                                self.mask_buf.as_mut_ptr(),
                                out_w,
                            ) == 1;
                        }
                    }
                }
            }
            if !mask_ok {
                let ref_slice = if cfg.segmentation.ai_selfie.engine_mode != 1
                    && self.has_ref_bg_fallback
                    && self.ref_bg_w > 0
                    && self.ref_bg_h > 0
                    && self.ref_bg_buf.len() >= (self.ref_bg_w * self.ref_bg_h * 4) as usize
                {
                    Some((self.ref_bg_buf.as_slice(), self.ref_bg_w, self.ref_bg_h))
                } else {
                    None
                };
                self.tracked_subject_center = rust_ai_portrait_mask(
                    &self.framed_buf,
                    ref_slice,
                    self.tracked_subject_center,
                    &mut self.mask_buf,
                    out_w,
                    out_h,
                    cfg,
                );
            } else {
                let _ = self.get_subject_center();
            }
        }

        // 4. SIMD Alpha Blending
        let mut blend_ok = false;
        if let Some(ref l) = self.lib {
            if !self.engine_handle.is_null() {
                if let Some(f_blend) = l.fn_blend {
                    unsafe {
                        blend_ok = f_blend(
                            self.engine_handle,
                            self.framed_buf.as_ptr(),
                            self.bg_buf.as_ptr(),
                            self.mask_buf.as_ptr(),
                            self.out_buf.as_mut_ptr(),
                            out_w,
                            out_h,
                            out_w * 4,
                            out_w,
                        ) == 1;
                    }
                }
            }
        }
        if !blend_ok {
            for i in 0..total_px {
                let a = self.mask_buf[i] as u32;
                let alpha = a + (a >> 7);
                let inv = 256 - alpha;
                let base = i * 4;
                self.out_buf[base] =
                    (((self.framed_buf[base] as u32) * alpha + (self.bg_buf[base] as u32) * inv) >> 8) as u8;
                self.out_buf[base + 1] = (((self.framed_buf[base + 1] as u32) * alpha
                    + (self.bg_buf[base + 1] as u32) * inv)
                    >> 8) as u8;
                self.out_buf[base + 2] = (((self.framed_buf[base + 2] as u32) * alpha
                    + (self.bg_buf[base + 2] as u32) * inv)
                    >> 8) as u8;
                self.out_buf[base + 3] = 255;
            }
        }

        (out_w, out_h)
    }
}

impl Drop for SmartCamPipelineInstance {
    fn drop(&mut self) {
        self.close_hardware_camera();
        if let Some(ref l) = self.lib {
            unsafe {
                if !self.sink_handle.is_null() {
                    if let Some(f_sdestroy) = l.fn_sink_destroy {
                        f_sdestroy(self.sink_handle);
                    }
                    self.sink_handle = std::ptr::null_mut();
                }
                if !self.engine_handle.is_null() {
                    if let Some(f_edestroy) = l.fn_destroy {
                        f_edestroy(self.engine_handle);
                    }
                    self.engine_handle = std::ptr::null_mut();
                }
                if !l.hmodule.is_null() {
                    FreeLibrary(l.hmodule);
                }
            }
        }
    }
}

// ============================================================================
// Pure-Rust Fallback Helpers, Studio Presets & Social Crop/Zoom Resampler
// ============================================================================

/// Computes the exact source crop rectangle `(crop_x, crop_y, crop_w, crop_h)` inside `(src_w, src_h)`
/// matching target aspect ratio `(aspect_w : aspect_h)`, zoom factor `[1.0..=4.0]`, and pan `[0.0..=1.0]`.
pub fn compute_social_crop_rect(
    src_w: u32,
    src_h: u32,
    aspect_w: u32,
    aspect_h: u32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> (u32, u32, u32, u32) {
    if src_w == 0 || src_h == 0 || aspect_w == 0 || aspect_h == 0 {
        return (0, 0, src_w.max(1), src_h.max(1));
    }
    let (_, _, base_w, base_h) = compute_center_crop_rect(src_w, src_h, aspect_w, aspect_h);
    let z = zoom.clamp(1.0, 4.0);
    let cw = ((base_w as f32) / z).round().max(16.0).min(src_w as f32) as u32;
    let ch = ((base_h as f32) / z).round().max(16.0).min(src_h as f32) as u32;

    let max_x = src_w.saturating_sub(cw);
    let max_y = src_h.saturating_sub(ch);
    let cx = ((max_x as f32) * pan_x.clamp(0.0, 1.0)).round() as u32;
    let cy = ((max_y as f32) * pan_y.clamp(0.0, 1.0)).round() as u32;
    (cx.min(max_x), cy.min(max_y), cw, ch)
}

/// Crops, optionally mirrors horizontally/vertically, and bilinearly scales `src` (`src_w x src_h`)
/// into `dst` (`dst_w x dst_h`) using Social Crop/Zoom parameters.
#[allow(clippy::too_many_arguments)]
pub fn crop_zoom_image_social(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    mirror_h: bool,
    mirror_v: bool,
) -> (u32, u32, u32, u32) {
    let (cx, cy, cw, ch) = compute_social_crop_rect(src_w, src_h, dst_w, dst_h, zoom, pan_x, pan_y);
    if src.len() < (src_w * src_h * 4) as usize || dst.len() < (dst_w * dst_h * 4) as usize {
        return (cx, cy, cw, ch);
    }

    let max_sx = src_w.saturating_sub(1);
    let max_sy = src_h.saturating_sub(1);

    for y in 0..dst_h {
        let fy = (cy as f32) + ((y as f32) + 0.5) * (ch as f32) / (dst_h.max(1) as f32) - 0.5;
        let raw_y0 = (fy.floor().max(0.0) as u32).min(max_sy);
        let raw_y1 = (raw_y0 + 1).min(max_sy);
        let wy = ((fy - (raw_y0 as f32)).clamp(0.0, 1.0) * 256.0) as u32;
        let inv_wy = 256 - wy;
        let y0 = if mirror_v { max_sy - raw_y0 } else { raw_y0 };
        let y1 = if mirror_v { max_sy - raw_y1 } else { raw_y1 };

        for x in 0..dst_w {
            let fx = (cx as f32) + ((x as f32) + 0.5) * (cw as f32) / (dst_w.max(1) as f32) - 0.5;
            let raw_x0 = (fx.floor().max(0.0) as u32).min(max_sx);
            let raw_x1 = (raw_x0 + 1).min(max_sx);
            let wx = ((fx - (raw_x0 as f32)).clamp(0.0, 1.0) * 256.0) as u32;
            let inv_wx = 256 - wx;
            let x0 = if mirror_h { max_sx - raw_x0 } else { raw_x0 };
            let x1 = if mirror_h { max_sx - raw_x1 } else { raw_x1 };

            let w00 = inv_wx * inv_wy;
            let w10 = wx * inv_wy;
            let w01 = inv_wx * wy;
            let w11 = wx * wy;

            let i00 = ((y0 * src_w + x0) * 4) as usize;
            let i10 = ((y0 * src_w + x1) * 4) as usize;
            let i01 = ((y1 * src_w + x0) * 4) as usize;
            let i11 = ((y1 * src_w + x1) * 4) as usize;
            let d_idx = ((y * dst_w + x) * 4) as usize;

            for c in 0..3 {
                let val = (src[i00 + c] as u32) * w00
                    + (src[i10 + c] as u32) * w10
                    + (src[i01 + c] as u32) * w01
                    + (src[i11 + c] as u32) * w11;
                dst[d_idx + c] = (val >> 16) as u8;
            }
            dst[d_idx + 3] = 255;
        }
    }
    (cx, cy, cw, ch)
}

/// Generates a high-resolution architectural/studio photography preset in `BGRA32` when disk assets are not found.
pub fn generate_studio_preset_bgra(preset_id: u8, width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let mut buf = vec![255u8; (width * height * 4) as usize];
    for y in 0..height {
        let ny = (y as f32) / (height.max(1) as f32);
        for x in 0..width {
            let nx = (x as f32) / (width.max(1) as f32);
            let dx = nx - 0.52;
            let dy = ny - 0.42;
            let vignette = (1.0 - (dx * dx * 0.95 + dy * dy * 1.15)).clamp(0.35, 1.0);

            // Soft bokeh architectural elements (window blinds, bookshelf & warm key light)
            let window_panel = if nx > 0.62 && nx < 0.90 && ny > 0.12 && ny < 0.72 {
                let blind = ((ny * 28.0).sin() * 0.5 + 0.5).powf(2.0);
                0.18 * blind
            } else {
                0.0
            };
            let shelf_accent = if nx > 0.08 && nx < 0.36 && ny > 0.20 && ny < 0.80 {
                let row = ((ny * 14.0).cos() * 0.5 + 0.5).powf(3.0);
                0.10 * row
            } else {
                0.0
            };

            let (r_base, g_base, b_base) = match preset_id {
                2 => {
                    // Preset 2: Warm Modern Office (warm amber/wood tones + daylight window)
                    let r = (76.0 + 95.0 * (1.0 - ny * 0.5) + 110.0 * window_panel + 40.0 * shelf_accent) * vignette;
                    let g = (58.0 + 72.0 * (1.0 - ny * 0.5) + 92.0 * window_panel + 25.0 * shelf_accent) * vignette;
                    let b = (44.0 + 50.0 * (1.0 - ny * 0.4) + 75.0 * window_panel + 12.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
                3 => {
                    // Preset 3: Minimal Daylight Loft (bright airy architectural concrete & soft cyan daylight)
                    let r = (135.0 + 75.0 * (1.0 - ny * 0.45) + 45.0 * window_panel - 20.0 * shelf_accent) * vignette;
                    let g = (144.0 + 78.0 * (1.0 - ny * 0.45) + 48.0 * window_panel - 15.0 * shelf_accent) * vignette;
                    let b = (154.0 + 82.0 * (1.0 - ny * 0.40) + 52.0 * window_panel - 10.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
                4 => {
                    // Preset 4: Dark Executive Suite (deep charcoal slate + subtle gold/teal rim lighting)
                    let r = (24.0 + 36.0 * (1.0 - ny * 0.6) + 64.0 * window_panel + 45.0 * shelf_accent) * vignette;
                    let g = (30.0 + 44.0 * (1.0 - ny * 0.6) + 58.0 * window_panel + 32.0 * shelf_accent) * vignette;
                    let b = (42.0 + 58.0 * (1.0 - ny * 0.5) + 52.0 * window_panel + 20.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
                _ => {
                    // Preset 1: Studio Blue Bokeh (professional broadcast studio blue-teal depth)
                    let r = (26.0 + 48.0 * (1.0 - ny * 0.5) + 65.0 * window_panel + 22.0 * shelf_accent) * vignette;
                    let g = (54.0 + 86.0 * (1.0 - ny * 0.5) + 95.0 * window_panel + 38.0 * shelf_accent) * vignette;
                    let b = (92.0 + 122.0 * (1.0 - ny * 0.4) + 115.0 * window_panel + 55.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
            };

            let idx = ((y * width + x) * 4) as usize;
            buf[idx] = b_base.clamp(0.0, 255.0) as u8;
            buf[idx + 1] = g_base.clamp(0.0, 255.0) as u8;
            buf[idx + 2] = r_base.clamp(0.0, 255.0) as u8;
            buf[idx + 3] = 255;
        }
    }
    (buf, width, height)
}

pub fn bgr_to_hsv(b: u8, g: u8, r: u8) -> (u8, u8, u8) {
    let rf = r as i32;
    let gf = g as i32;
    let bf = b as i32;
    let cmax = rf.max(gf).max(bf);
    let cmin = rf.min(gf).min(bf);
    let delta = cmax - cmin;
    let val = cmax as u8;
    let sat = if cmax == 0 {
        0u8
    } else {
        ((delta * 255) / cmax).clamp(0, 255) as u8
    };
    if delta == 0 {
        return (0, sat, val);
    }
    let mut hue = if cmax == rf {
        (30 * (gf - bf)) / delta
    } else if cmax == gf {
        60 + (30 * (bf - rf)) / delta
    } else {
        120 + (30 * (rf - gf)) / delta
    };
    if hue < 0 {
        hue += 180;
    }
    (hue.clamp(0, 180) as u8, sat, val)
}

fn sample_hsv_from_frame(
    frame_bgra: &[u8],
    width: u32,
    height: u32,
    sample_x: i32,
    sample_y: i32,
) -> (u8, u8, u8) {
    if width == 0 || height == 0 || frame_bgra.len() < (width * height * 4) as usize {
        return (60, 180, 180);
    }
    let (cx, cy) = if sample_x >= 0 && sample_y >= 0 {
        (
            (sample_x as u32).min(width - 1),
            (sample_y as u32).min(height - 1),
        )
    } else {
        (width / 8, height / 8)
    };
    let mut sum_h = 0u32;
    let mut sum_s = 0u32;
    let mut sum_v = 0u32;
    let mut count = 0u32;

    let y0 = cy.saturating_sub(4);
    let y1 = (cy + 4).min(height - 1);
    let x0 = cx.saturating_sub(4);
    let x1 = (cx + 4).min(width - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let idx = ((y * width + x) * 4) as usize;
            let (h, s, v) = bgr_to_hsv(frame_bgra[idx], frame_bgra[idx + 1], frame_bgra[idx + 2]);
            sum_h += h as u32;
            sum_s += s as u32;
            sum_v += v as u32;
            count += 1;
        }
    }
    if count == 0 {
        return (60, 180, 180);
    }
    (
        (sum_h / count) as u8,
        (sum_s / count) as u8,
        (sum_v / count) as u8,
    )
}

fn rust_crop_and_scale(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    aspect_w: u32,
    aspect_h: u32,
) {
    let (cx, cy, cw, ch) = compute_center_crop_rect(src_w, src_h, aspect_w, aspect_h);
    for y in 0..dst_h {
        let sy = cy + (y * ch) / dst_h.max(1);
        let sy = sy.min(src_h.saturating_sub(1));
        for x in 0..dst_w {
            let sx = cx + (x * cw) / dst_w.max(1);
            let sx = sx.min(src_w.saturating_sub(1));
            let s_idx = ((sy * src_w + sx) * 4) as usize;
            let d_idx = ((y * dst_w + x) * 4) as usize;
            if s_idx + 4 <= src.len() && d_idx + 4 <= dst.len() {
                dst[d_idx..d_idx + 4].copy_from_slice(&src[s_idx..s_idx + 4]);
            }
        }
    }
}

fn rust_greenscreen_mask(
    fg_bgra: &mut [u8],
    mask: &mut [u8],
    width: u32,
    height: u32,
    cfg: &SmartCamConfig,
) {
    let gs = &cfg.segmentation.green_screen;
    let total = (width * height) as usize;
    for i in 0..total {
        let base = i * 4;
        let b = fg_bgra[base];
        let g = fg_bgra[base + 1];
        let r = fg_bgra[base + 2];
        let (h, s, v) = bgr_to_hsv(b, g, r);
        if h >= gs.hue_min
            && h <= gs.hue_max
            && s >= gs.sat_min
            && s <= gs.sat_max
            && v >= gs.val_min
            && v <= gs.val_max
        {
            mask[i] = 0;
        } else {
            mask[i] = 255;
            if gs.spill_suppression > 0.01 {
                let max_rb = r.max(b);
                if g > max_rb {
                    let excess = ((g - max_rb) as f32 * gs.spill_suppression) as u8;
                    fg_bgra[base + 1] = g.saturating_sub(excess);
                }
            }
        }
    }
}

fn rust_compute_face_calibration_from_landmarks(
    snapshot_bgra: &[u8],
    width: u32,
    height: u32,
    pts: [(f32, f32); 5],
) -> (f32, f32, f32, f32) {
    // pts: [0]=Left Eye, [1]=Right Eye, [2]=Nose Tip, [3]=Lips, [4]=Chin
    let (lex, ley) = (pts[0].0.clamp(0.02, 0.98), pts[0].1.clamp(0.02, 0.98));
    let (rex, rey) = (pts[1].0.clamp(0.02, 0.98), pts[1].1.clamp(0.02, 0.98));
    let (nx, ny) = (pts[2].0.clamp(0.02, 0.98), pts[2].1.clamp(0.02, 0.98));
    let (lx, ly) = (pts[3].0.clamp(0.02, 0.98), pts[3].1.clamp(0.02, 0.98));
    let (cx, cy) = (pts[4].0.clamp(0.02, 0.98), pts[4].1.clamp(0.02, 0.98));

    let eye_dist = ((rex - lex).hypot(rey - ley)).max(0.035);
    let eye_mid_x = 0.5 * (lex + rex);
    let eye_mid_y = 0.5 * (ley + rey);
    let eye_to_chin = (cy - eye_mid_y).abs().max(0.055);

    let head_rx = (eye_dist * 1.22).clamp(0.065, 0.28);
    let head_ry = (eye_to_chin * 0.96).clamp(0.110, 0.36);

    // Sample skin patches at nose bridge, left/right cheeks, forehead, and chin
    let forehead_x = eye_mid_x;
    let forehead_y = (eye_mid_y - eye_to_chin * 0.36).clamp(0.03, 0.95);
    let left_cheek_x = (0.55 * lex + 0.45 * nx).clamp(0.03, 0.97);
    let left_cheek_y = (0.45 * ley + 0.55 * ny).clamp(0.03, 0.97);
    let right_cheek_x = (0.55 * rex + 0.45 * nx).clamp(0.03, 0.97);
    let right_cheek_y = (0.45 * rey + 0.55 * ny).clamp(0.03, 0.97);

    let sample_centers = [
        (nx, ny),
        (left_cheek_x, left_cheek_y),
        (right_cheek_x, right_cheek_y),
        (forehead_x, forehead_y),
        (0.5 * (lx + cx), 0.5 * (ly + cy)),
    ];

    let mut sum_cb = 0.0f32;
    let mut sum_cr = 0.0f32;
    let mut count = 0.0f32;

    for (sx_norm, sy_norm) in sample_centers {
        let px_c = ((sx_norm * (width as f32)).round() as i32).clamp(2, (width as i32).saturating_sub(3));
        let py_c = ((sy_norm * (height as f32)).round() as i32).clamp(2, (height as i32).saturating_sub(3));
        for dy in -2..=2 {
            for dx in -2..=2 {
                let x = (px_c + dx) as u32;
                let y = (py_c + dy) as u32;
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < snapshot_bgra.len() {
                    let b = snapshot_bgra[idx] as f32;
                    let g = snapshot_bgra[idx + 1] as f32;
                    let r = snapshot_bgra[idx + 2] as f32;
                    let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.500000 * b;
                    let cr = 128.0 + 0.500000 * r - 0.418688 * g - 0.081312 * b;
                    sum_cb += cb;
                    sum_cr += cr;
                    count += 1.0;
                }
            }
        }
    }

    let (skin_cb, skin_cr) = if count > 0.0 {
        (
            (sum_cb / count).clamp(65.0, 170.0),
            (sum_cr / count).clamp(118.0, 205.0),
        )
    } else {
        (113.0, 148.0)
    };

    (skin_cb, skin_cr, head_rx, head_ry)
}

fn rust_ai_portrait_mask(
    fg_bgra: &[u8],
    ref_bg_info: Option<(&[u8], u32, u32)>,
    prev_center: (f32, f32),
    mask: &mut [u8],
    width: u32,
    height: u32,
    cfg: &SmartCamConfig,
) -> (f32, f32) {
    let ai = &cfg.segmentation.ai_selfie;
    let thresh = ai.confidence_threshold;
    let tol = ai.bg_diff_tolerance.clamp(0.10, 0.95);
    let face_only = ai.silhouette_mode == 1;
    let cal_active = ai.face_calibrated;
    let target_cb = ai.calib_skin_cb;
    let target_cr = ai.calib_skin_cr;
    let head_rx = if cal_active {
        ai.calib_head_rx.clamp(0.08, 0.28)
    } else {
        0.135f32
    };
    let head_ry = if cal_active {
        ai.calib_head_ry.clamp(0.12, 0.36)
    } else {
        0.22f32
    };
    let body_rx = (head_rx * 2.15).clamp(0.22, 0.46);
    let body_ry = (head_ry * 1.45).clamp(0.24, 0.46);

    // 1. Dynamic Subject Tracking: estimate true facial skin centroid in upper-middle frame (rejecting wood/yellow hoodies)
    let mut sum_wx = 0.0f32;
    let mut sum_wy = 0.0f32;
    let mut sum_w = 0.0f32;
    let step = 8u32;
    let max_face_y = (height * 64) / 100;
    for y in (step..max_face_y).step_by(step as usize) {
        let ny = (y as f32) / (height.max(1) as f32);
        let row_base_l = ((y * width + 4) * 4) as usize;
        let row_base_r = ((y * width + width.saturating_sub(5)) * 4) as usize;
        let (bl, gl, rl) = (
            fg_bgra[row_base_l] as i32,
            fg_bgra[row_base_l + 1] as i32,
            fg_bgra[row_base_l + 2] as i32,
        );
        let (br, gr, rr) = (
            fg_bgra[row_base_r] as i32,
            fg_bgra[row_base_r + 1] as i32,
            fg_bgra[row_base_r + 2] as i32,
        );

        for x in (step..width.saturating_sub(step)).step_by(step as usize) {
            let nx = (x as f32) / (width.max(1) as f32);
            let base = ((y * width + x) * 4) as usize;
            let b = fg_bgra[base] as i32;
            let g = fg_bgra[base + 1] as i32;
            let r = fg_bgra[base + 2] as i32;

            let is_green = g > 135 && g > r + 40 && g > b + 40;
            if is_green {
                continue;
            }
            let diff_l = (r - rl).abs() + (g - gl).abs() + (b - bl).abs();
            let diff_r = (r - rr).abs() + (g - gr).abs() + (b - br).abs();
            if diff_l < 38 || diff_r < 38 {
                continue;
            }

            if cal_active {
                let rf = r as f32;
                let gf = g as f32;
                let bf = b as f32;
                let cb = 128.0 - 0.168736 * rf - 0.331264 * gf + 0.500000 * bf;
                let cr = 128.0 + 0.500000 * rf - 0.418688 * gf - 0.081312 * bf;
                let dist_c = (cb - target_cb).hypot(cr - target_cr);
                if dist_c < 22.0 && r > 55 {
                    let w = (24.0 - dist_c).max(1.0) * 0.12;
                    sum_wx += nx * w;
                    sum_wy += ny * w;
                    sum_w += w;
                }
            } else {
                let rb = r - b;
                let rg = r - g;
                let true_skin = r > 85
                    && g > 45
                    && b > 25
                    && rb >= 18
                    && rg >= (rb * 24) / 100
                    && rg <= (rb * 85) / 100
                    && rb * 2 <= r;
                if true_skin {
                    let w = (rb as f32) * 0.025;
                    sum_wx += nx * w;
                    sum_wy += ny * w;
                    sum_w += w;
                }
            }
        }
    }

    let (target_cx, target_cy) = if sum_w > 1.2 {
        (
            (sum_wx / sum_w).clamp(0.15, 0.85),
            (sum_wy / sum_w).clamp(0.18, 0.62),
        )
    } else {
        prev_center
    };
    let jump = ((target_cx - prev_center.0).powi(2) + (target_cy - prev_center.1).powi(2)).sqrt();
    let alpha = if jump > 0.04 { 0.85 } else { 0.35 };
    let cx = prev_center.0 * (1.0 - alpha) + target_cx * alpha;
    let cy = prev_center.1 * (1.0 - alpha) + target_cy * alpha;

    // Fast Non-AI path (< 0.2 ms) when AI is disabled and no reference background is set
    if !cfg.ai_enabled && ref_bg_info.is_none() {
        let block = 8u32;
        for by in (0..height).step_by(block as usize) {
            let y_end = (by + block).min(height);
            let ny = (by as f32) / (height.max(1) as f32);
            for bx in (0..width).step_by(block as usize) {
                let x_end = (bx + block).min(width);
                let nx = (bx as f32) / (width.max(1) as f32);
                let base = ((by * width + bx) * 4) as usize;
                let b = fg_bgra[base];
                let g = fg_bgra[base + 1];
                let r = fg_bgra[base + 2];
                let is_bright_green = g > 140 && g > r.saturating_add(45) && g > b.saturating_add(45);
                let alpha = if is_bright_green {
                    0u8
                } else {
                    let head_dx = (nx - cx) / head_rx;
                    let head_dy = (ny - (cy - 0.03)) / head_ry;
                    let head_d2 = head_dx * head_dx + head_dy * head_dy;
                    let min_d2 = if face_only {
                        head_d2
                    } else {
                        let body_dx = (nx - cx) / body_rx;
                        let body_dy = (ny - (cy + 0.36)) / body_ry;
                        head_d2.min(body_dx * body_dx + body_dy * body_dy)
                    };
                    if min_d2 <= 1.05 {
                        255u8
                    } else if min_d2 <= 1.35 {
                        128u8
                    } else {
                        0u8
                    }
                };
                for y in by..y_end {
                    let row = (y * width) as usize;
                    for x in bx..x_end {
                        mask[row + (x as usize)] = alpha;
                    }
                }
            }
        }
        return (cx, cy);
    }

    // 2. Compute per-pixel foreground probability (Reference Background Subtraction OR Dynamic Head+Torso Silhouette)
    for y in 0..height {
        let ny = (y as f32) / (height.max(1) as f32);
        for x in 0..width {
            let nx = (x as f32) / (width.max(1) as f32);
            let i = (y * width + x) as usize;
            let base = i * 4;
            let b = fg_bgra[base];
            let g = fg_bgra[base + 1];
            let r = fg_bgra[base + 2];

            let is_bright_green = g > 140 && g > r.saturating_add(45) && g > b.saturating_add(45);
            if is_bright_green {
                mask[i] = 0;
                continue;
            }

            if let Some((ref_bg, rw, rh)) = ref_bg_info {
                let rx = ((x * rw) / width.max(1)).min(rw.saturating_sub(1));
                let ry = ((y * rh) / height.max(1)).min(rh.saturating_sub(1));
                let rbase = ((ry * rw + rx) * 4) as usize;

                // Normalized chroma + luma difference resilient to auto-exposure / LED flicker
                let y_cur = (r as i32 * 77 + g as i32 * 150 + b as i32 * 29) >> 8;
                let y_ref = (ref_bg[rbase + 2] as i32 * 77
                    + ref_bg[rbase + 1] as i32 * 150
                    + ref_bg[rbase] as i32 * 29)
                    >> 8;
                let cb_cur = b as i32 - y_cur;
                let cr_cur = r as i32 - y_cur;
                let cb_ref = ref_bg[rbase] as i32 - y_ref;
                let cr_ref = ref_bg[rbase + 2] as i32 - y_ref;

                let d_chroma = ((cb_cur - cb_ref).abs() + (cr_cur - cr_ref).abs()) as f32 / 255.0;
                let d_luma = (y_cur - y_ref).abs() as f32 / 255.0;
                let luma_gate = (0.04 + tol * 0.14).clamp(0.04, 0.20);
                let eff_luma = (d_luma - luma_gate).max(0.0);

                let diff = d_chroma * 1.45 + eff_luma * 0.85;
                let gain = (5.2 - tol * 3.0).clamp(1.8, 5.0);
                let mut prob = (diff * gain).clamp(0.0, 1.0);
                if face_only {
                    let head_dx = (nx - cx) / (head_rx * 1.15);
                    let head_dy = (ny - (cy - 0.03)) / (head_ry * 1.15);
                    let head_d2 = head_dx * head_dx + head_dy * head_dy;
                    let head_gate = (1.35 - head_d2).clamp(0.0, 1.0);
                    prob *= head_gate;
                }
                let cutoff = (thresh * 0.30 + tol * 0.22).clamp(0.10, 0.65);
                let t = ((prob - cutoff) / 0.22).clamp(0.0, 1.0);
                mask[i] = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
            } else {
                // Dynamic Head (+ optional Shoulders) Silhouette centered on tracked `(cx, cy)`
                let head_dx = (nx - cx) / head_rx;
                let head_dy = (ny - (cy - 0.03)) / head_ry;
                let head_d2 = head_dx * head_dx + head_dy * head_dy;

                let min_d2 = if face_only {
                    head_d2
                } else {
                    let body_dx = (nx - cx) / body_rx;
                    let body_dy = (ny - (cy + 0.36)) / body_ry;
                    let body_d2 = body_dx * body_dx + body_dy * body_dy;
                    head_d2.min(body_d2)
                };
                let prob = (1.32 - min_d2).clamp(0.0, 1.0);
                let t = ((prob - (thresh - 0.15)) / 0.30).clamp(0.0, 1.0);
                mask[i] = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
            }
        }
    }
    (cx, cy)
}

fn rust_fast_box_blur(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    // Downsampled 8x8 block average for ultra-fast fallback bokeh
    let step = 8u32;
    for by in (0..height).step_by(step as usize) {
        let y_end = (by + step).min(height);
        for bx in (0..width).step_by(step as usize) {
            let x_end = (bx + step).min(width);
            let sy = (by + y_end) / 2;
            let sx = (bx + x_end) / 2;
            let s_idx = ((sy * width + sx) * 4) as usize;
            let b = src[s_idx];
            let g = src[s_idx + 1];
            let r = src[s_idx + 2];
            for y in by..y_end {
                for x in bx..x_end {
                    let d_idx = ((y * width + x) * 4) as usize;
                    dst[d_idx] = b;
                    dst[d_idx + 1] = g;
                    dst[d_idx + 2] = r;
                    dst[d_idx + 3] = 255;
                }
            }
        }
    }
}

pub fn generate_synthetic_bgra_frame(buf: &mut Vec<u8>, width: u32, height: u32, frame_idx: u64) {
    let total_bytes = (width * height * 4) as usize;
    if buf.len() != total_bytes {
        buf.resize(total_bytes, 255);
    }

    let phase = (frame_idx as f32) * 0.08;
    let cx = (width as i32) / 2 + (phase.sin() * 26.0) as i32;
    let head_y = (height as i32) * 38 / 100;
    let head_rx = (width as i32) / 11;
    let head_ry = (height as i32) / 7;
    let body_y = (height as i32) * 82 / 100;
    let body_rx = (width as i32) / 5;
    let body_ry = (height as i32) / 3;

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let idx = ((y as u32 * width + x as u32) * 4) as usize;

            let hdx = ((x - cx) as f32) / (head_rx.max(1) as f32);
            let hdy = ((y - head_y) as f32) / (head_ry.max(1) as f32);
            let in_head = (hdx * hdx + hdy * hdy) <= 1.0;

            let bdx = ((x - cx) as f32) / (body_rx.max(1) as f32);
            let bdy = ((y - body_y) as f32) / (body_ry.max(1) as f32);
            let in_shoulders = (bdx * bdx + bdy * bdy) <= 1.0;

            if in_head {
                // Warm natural portrait tone
                buf[idx] = 142;     // B
                buf[idx + 1] = 172; // G
                buf[idx + 2] = 222; // R
                buf[idx + 3] = 255;
            } else if in_shoulders {
                // Smart blue-grey blazer/shirt
                buf[idx] = 115;     // B
                buf[idx + 1] = 78;  // G
                buf[idx + 2] = 52;  // R
                buf[idx + 3] = 255;
            } else {
                // Chroma green screen background (B=20, G=215, R=20)
                buf[idx] = 20;
                buf[idx + 1] = 215;
                buf[idx + 2] = 20;
                buf[idx + 3] = 255;
            }
        }
    }
}

// ============================================================================
// Direct Win32 Shared Memory Writer Fallback (SharedFrameHeader v1)
// ============================================================================

#[repr(C, packed(8))]
struct SharedFrameHeaderRaw {
    magic: u32,
    version: u32,
    sequence_counter: AtomicI64,
    width: u32,
    height: u32,
    stride: u32,
    fps_num: u32,
    fps_den: u32,
    pixel_format: u32,
    timestamp_100ns: u64,
    frame_index: u64,
    reserved: [u8; 64],
}

pub struct FallbackSharedMemoryWriter {
    mapping_handle: HANDLE,
    event_handle: HANDLE,
    mapped_view: *mut u8,
    frame_index: u64,
}

unsafe impl Send for FallbackSharedMemoryWriter {}

impl FallbackSharedMemoryWriter {
    const MAX_FRAME_BYTES: usize = 3840 * 2160 * 4;
    const TOTAL_BYTES: usize = 128 + Self::MAX_FRAME_BYTES;

    pub fn open(mapping_name: &str) -> Option<Self> {
        unsafe {
            let wname = to_wstring(mapping_name);
            let size_high = ((Self::TOTAL_BYTES as u64) >> 32) as DWORD;
            let size_low = (Self::TOTAL_BYTES as u64 & 0xFFFF_FFFF) as DWORD;
            let hmap = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                NULL,
                PAGE_READWRITE,
                size_high,
                size_low,
                wname.as_ptr(),
            );
            if hmap.is_null() {
                return None;
            }
            let view = MapViewOfFile(hmap, FILE_MAP_ALL_ACCESS, 0, 0, Self::TOTAL_BYTES) as *mut u8;
            if view.is_null() {
                CloseHandle(hmap);
                return None;
            }
            let ev_name = to_wstring("Local\\SmartCam_MF_FrameEvent_v1");
            let hev = CreateEventW(NULL, FALSE, FALSE, ev_name.as_ptr());

            let hdr = &mut *(view as *mut SharedFrameHeaderRaw);
            if hdr.magic != 0x534D434D {
                std::ptr::write_bytes(view, 0, 128);
                hdr.magic = 0x534D434D;
                hdr.version = 1;
                hdr.pixel_format = 4;
            }
            Some(Self {
                mapping_handle: hmap,
                event_handle: hev,
                mapped_view: view,
                frame_index: 0,
            })
        }
    }

    pub fn write_frame(&mut self, bgra: &[u8], width: u32, height: u32, fps: u32) {
        let bytes = (width * height * 4) as usize;
        if self.mapped_view.is_null() || bytes == 0 || bytes > Self::MAX_FRAME_BYTES || bgra.len() < bytes {
            return;
        }
        unsafe {
            let hdr = &mut *(self.mapped_view as *mut SharedFrameHeaderRaw);
            hdr.sequence_counter.fetch_add(1, Ordering::SeqCst);
            hdr.width = width;
            hdr.height = height;
            hdr.stride = width * 4;
            hdr.fps_num = fps;
            hdr.fps_den = 1;
            hdr.pixel_format = 4;
            hdr.timestamp_100ns = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| (d.as_nanos() / 100) as u64)
                .unwrap_or(0);
            self.frame_index += 1;
            hdr.frame_index = self.frame_index;

            let payload = self.mapped_view.add(128);
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), payload, bytes);

            hdr.sequence_counter.fetch_add(1, Ordering::SeqCst);
            if !self.event_handle.is_null() {
                SetEvent(self.event_handle);
            }
        }
    }
}

impl Drop for FallbackSharedMemoryWriter {
    fn drop(&mut self) {
        unsafe {
            if !self.mapped_view.is_null() {
                UnmapViewOfFile(self.mapped_view as *const c_void);
            }
            if !self.mapping_handle.is_null() {
                CloseHandle(self.mapping_handle);
            }
            if !self.event_handle.is_null() {
                CloseHandle(self.event_handle);
            }
        }
    }
}
