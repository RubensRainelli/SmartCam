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

use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use smartcam_engine::{CropStrategy, GreenScreenConfig, SimdTier, SmartCamEngine};

use crate::config::{resolve_aspect_ratio_pair, SmartCamConfig};
use crate::win32_ffi::*;

pub type FnEngineCreate = unsafe extern "C" fn(u32) -> *mut c_void;
pub type FnEngineDestroy = unsafe extern "C" fn(*mut c_void);
pub type FnGetSimdBackend = unsafe extern "C" fn(*mut c_void) -> *const c_char;
pub type FnConfigureGreenscreen =
    unsafe extern "C" fn(*mut c_void, u8, u8, u8, u8, u8, u8, u8, u8, f32);
pub type FnAutoCalibrateGreenscreen = unsafe extern "C" fn(
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
pub type FnCropScaleBgra =
    unsafe extern "C" fn(*const u8, u32, u32, u32, *mut u8, u32, u32, u32, u32, u32, u32) -> i32;
pub type FnComputeGreenscreenMask =
    unsafe extern "C" fn(*mut c_void, *mut u8, u32, u32, u32, *mut u8, u32) -> i32;
pub type FnComputeAiMask =
    unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32, *mut u8, u32) -> i32;
pub type FnFillSolidBg = unsafe extern "C" fn(*mut u8, u32, u32, u32, u8, u8, u8) -> i32;
pub type FnGenerateBlurredBg = unsafe extern "C" fn(*const u8, *mut u8, u32, u32, u32, u32) -> i32;
pub type FnBlendBgra = unsafe extern "C" fn(
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
pub type FnSetReferenceBg = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32) -> i32;
pub type FnClearReferenceBg = unsafe extern "C" fn(*mut c_void);
pub type FnHasReferenceBg = unsafe extern "C" fn(*mut c_void) -> i32;
pub type FnSetAiModeAndTolerance = unsafe extern "C" fn(*mut c_void, i32, f32);
pub type FnGetSubjectCenter = unsafe extern "C" fn(*mut c_void, *mut f32, *mut f32) -> i32;
pub type FnSetHardwarePolicy = unsafe extern "C" fn(*mut c_void, u32, u32, i32) -> i32;
pub type FnGetActiveComputeBackend = unsafe extern "C" fn(*mut c_void) -> *const c_char;
pub type FnGetActiveComputeDeviceName = unsafe extern "C" fn(*mut c_void) -> *const c_char;
pub type FnIsAiEnabled = unsafe extern "C" fn(*mut c_void) -> i32;
pub type FnCalibrateFaceLandmarks = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    u32,
    u32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    *mut f32,
    *mut f32,
    *mut f32,
    *mut f32,
) -> i32;
pub type FnSetFaceCalibrationProfile =
    unsafe extern "C" fn(*mut c_void, i32, f32, f32, f32, f32) -> i32;
pub type FnSetFaceCalibrationLandmarks = unsafe extern "C" fn(*mut c_void, *const f32) -> i32;
pub type FnSetSilhouetteMode = unsafe extern "C" fn(*mut c_void, i32) -> i32;
pub type FnGetSilhouetteMode = unsafe extern "C" fn(*mut c_void) -> i32;
pub type FnSetMultiPersonMode = unsafe extern "C" fn(*mut c_void, i32) -> i32;

pub type FnWinGetCameraCount = unsafe extern "C" fn() -> u32;
pub type FnWinGetCameraInfo =
    unsafe extern "C" fn(u32, *mut c_char, u32, *mut c_char, u32, *mut i32) -> i32;
pub type FnWinCameraCreate = unsafe extern "C" fn() -> *mut c_void;
pub type FnWinCameraOpen = unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> i32;
pub type FnWinCameraReadFrame =
    unsafe extern "C" fn(*mut c_void, *mut u8, u32, *mut u32, *mut u32) -> i32;
pub type FnWinCameraClose = unsafe extern "C" fn(*mut c_void);
pub type FnWinCameraDestroy = unsafe extern "C" fn(*mut c_void);

pub type FnWinSinkCreate = unsafe extern "C" fn() -> *mut c_void;
pub type FnWinSinkInit = unsafe extern "C" fn(*mut c_void, *const c_char, u32, u32, u32) -> i32;
pub type FnWinSinkPushFrame = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u32) -> i32;
pub type FnWinSinkIsConsumerActive = unsafe extern "C" fn(*mut c_void) -> i32;
pub type FnWinSinkDestroy = unsafe extern "C" fn(*mut c_void);

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

pub fn cpu_supports_avx512() -> bool {
    smartcam_engine::detect_best_simd_tier() == SimdTier::Avx512
}

// ============================================================================
// Pure-Rust Functions Matching C ABI For Backward Compatibility
// ============================================================================

unsafe extern "C" fn pure_engine_create(tier_u32: u32) -> *mut c_void {
    let tier = match tier_u32 {
        1 => SimdTier::Scalar,
        2 => SimdTier::Avx2,
        3 => SimdTier::Avx512,
        4 => SimdTier::Neon,
        _ => SimdTier::Auto,
    };
    Box::into_raw(Box::new(SmartCamEngine::new(tier))) as *mut c_void
}

unsafe extern "C" fn pure_engine_destroy(handle: *mut c_void) {
    if !handle.is_null() {
        drop(Box::from_raw(handle as *mut SmartCamEngine));
    }
}

unsafe extern "C" fn pure_get_simd(handle: *mut c_void) -> *const c_char {
    if !handle.is_null() {
        let engine = &*(handle as *const SmartCamEngine);
        match engine.get_simd_tier() {
            SimdTier::Avx512 => b"AVX-512\0".as_ptr() as *const c_char,
            SimdTier::Avx2 => b"AVX2\0".as_ptr() as *const c_char,
            SimdTier::Neon => b"NEON\0".as_ptr() as *const c_char,
            _ => b"Scalar\0".as_ptr() as *const c_char,
        }
    } else {
        b"Auto\0".as_ptr() as *const c_char
    }
}

unsafe extern "C" fn pure_crop_scale(
    src: *const u8,
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    dst: *mut u8,
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    aspect_w: u32,
    aspect_h: u32,
    strategy: u32,
) -> i32 {
    if src.is_null() || dst.is_null() || src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return 0;
    }
    let src_slice = std::slice::from_raw_parts(src, (src_h * stride_src) as usize);
    let dst_slice = std::slice::from_raw_parts_mut(dst, (dst_h * stride_dst) as usize);
    let strat = if strategy == 1 {
        CropStrategy::Letterbox
    } else {
        CropStrategy::CenterCrop
    };
    if smartcam_engine::crop_scale_bgra(
        src_slice,
        src_w,
        src_h,
        stride_src,
        dst_slice,
        dst_w,
        dst_h,
        stride_dst,
        aspect_w,
        aspect_h,
        strat,
        SimdTier::Auto,
    ) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_blend(
    handle: *mut c_void,
    fg: *const u8,
    bg: *const u8,
    mask: *const u8,
    out: *mut u8,
    w: u32,
    h: u32,
    stride_rgba: u32,
    stride_mask: u32,
) -> i32 {
    if fg.is_null() || bg.is_null() || mask.is_null() || out.is_null() || w == 0 || h == 0 {
        return 0;
    }
    let fg_slice = std::slice::from_raw_parts(fg, (h * stride_rgba) as usize);
    let bg_slice = std::slice::from_raw_parts(bg, (h * stride_rgba) as usize);
    let mask_slice = std::slice::from_raw_parts(mask, (h * stride_mask) as usize);
    let out_slice = std::slice::from_raw_parts_mut(out, (h * stride_rgba) as usize);
    let tier = if !handle.is_null() {
        let engine = &*(handle as *const SmartCamEngine);
        engine.get_simd_tier()
    } else {
        SimdTier::Auto
    };
    if smartcam_engine::blend_bgra(
        fg_slice,
        bg_slice,
        mask_slice,
        out_slice,
        w,
        h,
        stride_rgba,
        stride_rgba,
        stride_mask,
        stride_rgba,
        tier,
    ) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_fill_solid(
    dst: *mut u8,
    width: u32,
    height: u32,
    stride: u32,
    r: u8,
    g: u8,
    b: u8,
) -> i32 {
    if dst.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let dst_slice = std::slice::from_raw_parts_mut(dst, (height * stride) as usize);
    if smartcam_engine::fill_solid_bg(dst_slice, width, height, stride, r, g, b, true) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_blur_bg(
    src: *const u8,
    dst: *mut u8,
    width: u32,
    height: u32,
    stride: u32,
    radius: u32,
) -> i32 {
    if src.is_null() || dst.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let src_slice = std::slice::from_raw_parts(src, (height * stride) as usize);
    let dst_slice = std::slice::from_raw_parts_mut(dst, (height * stride) as usize);
    if smartcam_engine::generate_blurred_bg(
        src_slice,
        dst_slice,
        width,
        height,
        stride,
        radius,
        SimdTier::Auto,
    ) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_gs_mask(
    handle: *mut c_void,
    fg: *mut u8,
    w: u32,
    h: u32,
    stride_fg: u32,
    mask: *mut u8,
    stride_mask: u32,
) -> i32 {
    if fg.is_null() || mask.is_null() || w == 0 || h == 0 {
        return 0;
    }
    let fg_slice = std::slice::from_raw_parts_mut(fg, (h * stride_fg) as usize);
    let mask_slice = std::slice::from_raw_parts_mut(mask, (h * stride_mask) as usize);
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        if engine.compute_greenscreen_mask(fg_slice, w, h, stride_fg, mask_slice, stride_mask, true) {
            engine.suppress_green_spill(fg_slice, mask_slice, w, h, stride_fg, stride_mask, true);
            1
        } else {
            0
        }
    } else {
        let mut gs = smartcam_engine::GreenScreenSegmenter::new(GreenScreenConfig::default());
        if gs.compute_mask(fg_slice, w, h, stride_fg, mask_slice, stride_mask, true, SimdTier::Auto) {
            gs.suppress_green_spill(fg_slice, mask_slice, w, h, stride_fg, stride_mask, true);
            1
        } else {
            0
        }
    }
}

unsafe extern "C" fn pure_ai_mask(
    handle: *mut c_void,
    fg: *const u8,
    w: u32,
    h: u32,
    stride_fg: u32,
    mask: *mut u8,
    stride_mask: u32,
) -> i32 {
    if fg.is_null() || mask.is_null() || w == 0 || h == 0 {
        return 0;
    }
    let fg_slice = std::slice::from_raw_parts(fg, (h * stride_fg) as usize);
    let mask_slice = std::slice::from_raw_parts_mut(mask, (h * stride_mask) as usize);
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        if engine.compute_ai_mask(fg_slice, w, h, stride_fg, mask_slice, stride_mask, true, true) {
            1
        } else {
            0
        }
    } else {
        let mut ai = smartcam_engine::AiOnnxSegmenter::new(smartcam_engine::AiSegmenterConfig::default());
        if ai.compute_mask(fg_slice, w, h, stride_fg, mask_slice, stride_mask, true, true) {
            1
        } else {
            0
        }
    }
}

unsafe extern "C" fn pure_cfg_gs(
    handle: *mut c_void,
    h_min: u8,
    h_max: u8,
    s_min: u8,
    s_max: u8,
    v_min: u8,
    v_max: u8,
    morph: u8,
    blur: u8,
    spill: f32,
) {
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.configure_greenscreen(GreenScreenConfig {
            hue_min: h_min,
            hue_max: h_max,
            sat_min: s_min,
            sat_max: s_max,
            val_min: v_min,
            val_max: v_max,
            morph_kernel_size: morph,
            blur_radius: blur,
            spill_suppression: spill,
        });
    }
}

unsafe extern "C" fn pure_autocal_gs(
    handle: *mut c_void,
    frame: *const u8,
    w: u32,
    h: u32,
    stride: u32,
    sample_x: i32,
    sample_y: i32,
    out_hmin: *mut u8,
    out_hmax: *mut u8,
    out_smin: *mut u8,
    out_vmin: *mut u8,
) -> i32 {
    if frame.is_null() || w == 0 || h == 0 {
        return 0;
    }
    let frame_slice = std::slice::from_raw_parts(frame, (h * stride) as usize);
    let (hmin, hmax, smin, vmin) = if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.autocalibrate_greenscreen(frame_slice, w, h, stride, true, sample_x, sample_y)
    } else {
        smartcam_engine::autocalibrate_greenscreen_direct(
            frame_slice, w, h, stride, true, sample_x, sample_y,
        )
    };
    if !out_hmin.is_null() {
        *out_hmin = hmin;
    }
    if !out_hmax.is_null() {
        *out_hmax = hmax;
    }
    if !out_smin.is_null() {
        *out_smin = smin;
    }
    if !out_vmin.is_null() {
        *out_vmin = vmin;
    }
    1
}

unsafe extern "C" fn pure_set_hardware_policy(
    handle: *mut c_void,
    backend: u32,
    simd: u32,
    ai: i32,
) -> i32 {
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        let b = match backend {
            1 => smartcam_engine::ComputeBackend::Cpu,
            2 => smartcam_engine::ComputeBackend::Vulkan,
            3 => smartcam_engine::ComputeBackend::OpenCl,
            4 => smartcam_engine::ComputeBackend::Cuda,
            5 => smartcam_engine::ComputeBackend::Rocm,
            _ => smartcam_engine::ComputeBackend::Auto,
        };
        let s = match simd {
            1 => SimdTier::Avx512,
            2 => SimdTier::Avx2,
            3 => SimdTier::Scalar,
            4 => SimdTier::Neon,
            _ => SimdTier::Auto,
        };
        engine.set_hardware_policy(b, s, ai != 0);
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_get_active_compute_backend(handle: *mut c_void) -> *const c_char {
    if !handle.is_null() {
        let engine = &*(handle as *const SmartCamEngine);
        match engine.get_active_compute_backend() {
            smartcam_engine::ComputeBackend::Cpu => b"CPU\0".as_ptr() as *const c_char,
            smartcam_engine::ComputeBackend::Vulkan => b"Vulkan\0".as_ptr() as *const c_char,
            smartcam_engine::ComputeBackend::OpenCl => b"OpenCL\0".as_ptr() as *const c_char,
            smartcam_engine::ComputeBackend::Cuda => b"CUDA\0".as_ptr() as *const c_char,
            smartcam_engine::ComputeBackend::Rocm => b"ROCm\0".as_ptr() as *const c_char,
            smartcam_engine::ComputeBackend::Auto => b"Auto\0".as_ptr() as *const c_char,
        }
    } else {
        b"CPU\0".as_ptr() as *const c_char
    }
}

unsafe extern "C" fn pure_get_active_compute_device_name(_handle: *mut c_void) -> *const c_char {
    static DEV_NAME: &[u8] = b"Host CPU Vector Unit\0";
    DEV_NAME.as_ptr() as *const c_char
}

unsafe extern "C" fn pure_is_ai_enabled(_handle: *mut c_void) -> i32 {
    1
}

unsafe extern "C" fn pure_calibrate_face_landmarks(
    handle: *mut c_void,
    snapshot: *const u8,
    w: u32,
    h: u32,
    stride: u32,
    lex: f32,
    ley: f32,
    rex: f32,
    rey: f32,
    nx: f32,
    ny: f32,
    lx: f32,
    ly: f32,
    cx: f32,
    cy: f32,
    out_cb: *mut f32,
    out_cr: *mut f32,
    out_rx: *mut f32,
    out_ry: *mut f32,
) -> i32 {
    if snapshot.is_null() || w == 0 || h == 0 {
        return 0;
    }
    let snap_slice = std::slice::from_raw_parts(snapshot, (h * stride) as usize);
    let pts = [(lex, ley), (rex, rey), (nx, ny), (lx, ly), (cx, cy)];
    let (cb, cr, rx, ry) = if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.calibrate_face_landmarks(snap_slice, w, h, pts)
    } else {
        let mut ai = smartcam_engine::AiOnnxSegmenter::new(smartcam_engine::AiSegmenterConfig::default());
        ai.calibrate_face_landmarks(snap_slice, w, h, pts)
    };
    if !out_cb.is_null() {
        *out_cb = cb;
    }
    if !out_cr.is_null() {
        *out_cr = cr;
    }
    if !out_rx.is_null() {
        *out_rx = rx;
    }
    if !out_ry.is_null() {
        *out_ry = ry;
    }
    1
}

unsafe extern "C" fn pure_set_face_calibration_profile(
    handle: *mut c_void,
    enabled: i32,
    skin_cb: f32,
    skin_cr: f32,
    head_rx: f32,
    head_ry: f32,
) -> i32 {
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.set_face_calibration_profile(enabled != 0, skin_cb, skin_cr, head_rx, head_ry);
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_set_face_calibration_landmarks(
    handle: *mut c_void,
    landmarks: *const f32,
) -> i32 {
    if !handle.is_null() && !landmarks.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        let lm_slice = std::slice::from_raw_parts(landmarks, 10);
        let mut lm = [0.0f32; 10];
        lm.copy_from_slice(lm_slice);
        engine.set_face_calibration_landmarks(&lm);
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_set_silhouette_mode(handle: *mut c_void, mode: i32) -> i32 {
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.set_silhouette_mode(mode);
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_get_silhouette_mode(handle: *mut c_void) -> i32 {
    if !handle.is_null() {
        let engine = &*(handle as *const SmartCamEngine);
        engine.get_silhouette_mode()
    } else {
        0
    }
}

unsafe extern "C" fn pure_set_multi_person_mode(handle: *mut c_void, mode: i32) -> i32 {
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.set_multi_person_mode(mode != 0);
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_set_reference_background(
    handle: *mut c_void,
    bg: *const u8,
    w: u32,
    h: u32,
    _stride: u32,
) -> i32 {
    if !handle.is_null() && !bg.is_null() && w > 0 && h > 0 {
        let engine = &mut *(handle as *mut SmartCamEngine);
        let slice = std::slice::from_raw_parts(bg, (w * h * 4) as usize);
        if engine.set_reference_background(slice, w, h) {
            1
        } else {
            0
        }
    } else {
        0
    }
}

unsafe extern "C" fn pure_clear_reference_background(handle: *mut c_void) {
    if !handle.is_null() {
        let engine = &mut *(handle as *mut SmartCamEngine);
        engine.clear_reference_background();
    }
}

unsafe extern "C" fn pure_has_reference_background(handle: *mut c_void) -> i32 {
    if !handle.is_null() {
        let engine = &*(handle as *const SmartCamEngine);
        if engine.has_reference_background() {
            1
        } else {
            0
        }
    } else {
        0
    }
}

unsafe extern "C" fn pure_get_subject_center(
    handle: *mut c_void,
    out_cx: *mut f32,
    out_cy: *mut f32,
) -> i32 {
    if !handle.is_null() {
        let engine = &*(handle as *const SmartCamEngine);
        let (cx, cy) = engine.get_subject_center();
        if !out_cx.is_null() {
            *out_cx = cx;
        }
        if !out_cy.is_null() {
            *out_cy = cy;
        }
        1
    } else {
        0
    }
}

unsafe extern "C" fn pure_sink_create() -> *mut c_void {
    1 as *mut c_void
}

unsafe extern "C" fn pure_sink_init(
    _handle: *mut c_void,
    _name: *const c_char,
    _w: u32,
    _h: u32,
    _fps: u32,
) -> i32 {
    1
}

unsafe extern "C" fn pure_sink_push(
    _handle: *mut c_void,
    _bgra: *const u8,
    _w: u32,
    _h: u32,
    _fps: u32,
) -> i32 {
    1
}

unsafe extern "C" fn pure_sink_active(_handle: *mut c_void) -> i32 {
    0
}

unsafe extern "C" fn pure_sink_destroy(_handle: *mut c_void) {}

// ============================================================================
// Compatibility Structure: NativeEngineLibrary
// ============================================================================

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
    pub fn load_best(_repo_root: &Path, _preferred_variant: &str, simd_pref: &str) -> Option<Self> {
        let tier_str = match simd_pref {
            "scalar" => "Scalar",
            "avx2" => "AVX2",
            "avx512" => "AVX-512",
            _ => smartcam_engine::detect_best_simd_tier().as_str(),
        };

        Some(Self {
            hmodule: std::ptr::null_mut(),
            loaded_dll_path: PathBuf::from("smartcam-engine (Pure Rust Runtime Dispatch)"),
            variant_name: format!("Pure Rust {}", tier_str),
            fn_create: Some(pure_engine_create),
            fn_destroy: Some(pure_engine_destroy),
            fn_get_simd: Some(pure_get_simd),
            fn_cfg_gs: Some(pure_cfg_gs),
            fn_autocal_gs: Some(pure_autocal_gs),
            fn_crop_scale: Some(pure_crop_scale),
            fn_gs_mask: Some(pure_gs_mask),
            fn_ai_mask: Some(pure_ai_mask),
            fn_fill_solid: Some(pure_fill_solid),
            fn_blur_bg: Some(pure_blur_bg),
            fn_blend: Some(pure_blend),
            fn_set_ref_bg: Some(pure_set_reference_background),
            fn_clear_ref_bg: Some(pure_clear_reference_background),
            fn_has_ref_bg: Some(pure_has_reference_background),
            fn_set_ai_mode_tol: None,
            fn_get_subj_center: Some(pure_get_subject_center),
            fn_set_hardware_policy: Some(pure_set_hardware_policy),
            fn_get_compute_backend: Some(pure_get_active_compute_backend),
            fn_get_compute_device_name: Some(pure_get_active_compute_device_name),
            fn_is_ai_enabled: Some(pure_is_ai_enabled),
            fn_calibrate_face_landmarks: Some(pure_calibrate_face_landmarks),
            fn_set_face_calibration_profile: Some(pure_set_face_calibration_profile),
            fn_set_face_calibration_landmarks: Some(pure_set_face_calibration_landmarks),
            fn_set_silhouette_mode: Some(pure_set_silhouette_mode),
            fn_get_silhouette_mode: Some(pure_get_silhouette_mode),
            fn_set_multi_person_mode: Some(pure_set_multi_person_mode),
            fn_cam_count: Some(smartcam_capture::windows_backend::c_api::smartcam_win_get_camera_count),
            fn_cam_info: Some(smartcam_capture::windows_backend::c_api::smartcam_win_get_camera_info),
            fn_cam_create: Some(smartcam_capture::windows_backend::c_api::smartcam_win_camera_create),
            fn_cam_open: Some(smartcam_capture::windows_backend::c_api::smartcam_win_camera_open),
            fn_cam_read: Some(smartcam_capture::windows_backend::c_api::smartcam_win_camera_read_frame),
            fn_cam_close: Some(smartcam_capture::windows_backend::c_api::smartcam_win_camera_close),
            fn_cam_destroy: Some(smartcam_capture::windows_backend::c_api::smartcam_win_camera_destroy),
            fn_sink_create: Some(smartcam_capture::windows_backend::c_api::smartcam_win_sink_create),
            fn_sink_init: Some(smartcam_capture::windows_backend::c_api::smartcam_win_sink_init),
            fn_sink_push: Some(smartcam_capture::windows_backend::c_api::smartcam_win_sink_push_frame),
            fn_sink_active: Some(smartcam_capture::windows_backend::c_api::smartcam_win_sink_is_consumer_active),
            fn_sink_destroy: Some(smartcam_capture::windows_backend::c_api::smartcam_win_sink_destroy),
        })
    }

    pub fn enumerate_cameras(&self) -> Vec<CameraDeviceEntry> {
        smartcam_capture::enumerate_camera_devices()
            .into_iter()
            .map(|d| CameraDeviceEntry {
                index: d.index,
                name: d.friendly_name,
                symbolic_link: d.symbolic_link,
                is_infrared: d.is_infrared,
            })
            .collect()
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
    pub engine: SmartCamEngine,
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
    pub ref_bg_buf: Vec<u8>,
    pub ref_bg_w: u32,
    pub ref_bg_h: u32,
    pub has_ref_bg_fallback: bool,
    pub tracked_subject_center: (f32, f32),
    pub multi_person_mode_active: bool,
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

        let tier = match cfg.compositor.simd_acceleration.as_str() {
            "scalar" => SimdTier::Scalar,
            "avx2" => SimdTier::Avx2,
            "avx512" => SimdTier::Avx512,
            "neon" => SimdTier::Neon,
            _ => SimdTier::Auto,
        };
        let engine = SmartCamEngine::new(tier);

        let engine_handle = unsafe {
            pure_engine_create(match tier {
                SimdTier::Scalar => 1,
                SimdTier::Avx2 => 2,
                SimdTier::Avx512 => 3,
                SimdTier::Neon => 4,
                SimdTier::Auto => 0,
            })
        };

        let simd_backend_label = format!("Pure Rust Runtime SIMD ({})", engine.get_active_simd_name());
        let compute_backend_label = engine.get_active_compute_backend_name().to_string();
        let compute_device_label = engine.get_active_compute_device_name();

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
            engine,
            engine_handle,
            cam_handle: std::ptr::null_mut(),
            sink_handle: std::ptr::null_mut(),
            simd_backend_label,
            compute_backend_label,
            compute_device_label,
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
                    self.engine.calibrate_face_landmarks(&bytes[8..expected], sw, sh, pts);
                }
            }
        }
    }

    pub fn apply_hardware_policy(&mut self, cfg: &SmartCamConfig) {
        self.active_ai_enabled = cfg.ai_enabled;
        let backend_id = crate::benchmark::compute_backend_str_to_id(&cfg.compute_backend);
        let simd_id =
            crate::benchmark::simd_mode_str_to_policy_id(&cfg.compositor.simd_acceleration);

        let backend = match backend_id {
            1 => smartcam_engine::ComputeBackend::Cpu,
            2 => smartcam_engine::ComputeBackend::Vulkan,
            3 => smartcam_engine::ComputeBackend::OpenCl,
            4 => smartcam_engine::ComputeBackend::Cuda,
            5 => smartcam_engine::ComputeBackend::Rocm,
            _ => smartcam_engine::ComputeBackend::Auto,
        };
        let simd = match simd_id {
            1 => SimdTier::Avx512,
            2 => SimdTier::Avx2,
            3 => SimdTier::Scalar,
            4 => SimdTier::Neon,
            _ => SimdTier::Auto,
        };
        self.engine.set_hardware_policy(backend, simd, cfg.ai_enabled);

        let simd_s = self.engine.get_active_simd_name();
        let backend_s = self.engine.get_active_compute_backend_name();
        let dev_s = self.engine.get_active_compute_device_name();

        self.compute_backend_label = backend_s.to_string();
        self.compute_device_label = dev_s.clone();

        if backend_s.eq_ignore_ascii_case("CPU") || backend_s.eq_ignore_ascii_case("Auto") {
            self.simd_backend_label = format!("CPU {} (Pure Rust)", simd_s);
        } else if !dev_s.is_empty() {
            self.simd_backend_label = format!("GPU {} — {} (+{})", backend_s, dev_s, simd_s);
        } else {
            self.simd_backend_label = format!("GPU {} (+{})", backend_s, simd_s);
        }
    }

    pub fn update_greenscreen_config(&mut self, cfg: &SmartCamConfig) {
        self.apply_hardware_policy(cfg);
        let gs = &cfg.segmentation.green_screen;
        self.engine.configure_greenscreen(GreenScreenConfig {
            hue_min: gs.hue_min,
            hue_max: gs.hue_max,
            sat_min: gs.sat_min,
            sat_max: gs.sat_max,
            val_min: gs.val_min,
            val_max: gs.val_max,
            morph_kernel_size: gs.morph_kernel_size,
            blur_radius: gs.blur_radius,
            spill_suppression: gs.spill_suppression,
        });

        let multi_active =
            self.multi_person_mode_active || cfg.is_plugin_enabled("multi_person_tracker");
        self.sync_face_and_silhouette_settings(cfg, multi_active);
    }

    pub fn sync_face_and_silhouette_settings(
        &mut self,
        cfg: &SmartCamConfig,
        multi_person_active: bool,
    ) {
        self.multi_person_mode_active = multi_person_active;
        let ai = &cfg.segmentation.ai_selfie;
        if ai.face_calibrated {
            let pts = ai.calib_landmarks;
            let eye_mid_x = 0.5 * (pts[0].0 + pts[1].0);
            let eye_mid_y = 0.5 * (pts[0].1 + pts[1].1);
            let eye_to_chin = (pts[4].1 - eye_mid_y).abs().max(0.055);
            let head_cx = (0.30 * eye_mid_x + 0.35 * pts[2].0 + 0.20 * pts[3].0 + 0.15 * pts[4].0)
                .clamp(0.12, 0.88);
            let head_cy = (eye_mid_y + 0.105 * eye_to_chin).clamp(0.14, 0.82);
            self.tracked_subject_center = (head_cx, head_cy);
        }

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
        self.engine.set_face_calibration_landmarks(&flat_lm);
        self.engine.set_face_calibration_profile(
            ai.face_calibrated,
            ai.calib_skin_cb,
            ai.calib_skin_cr,
            ai.calib_head_rx,
            ai.calib_head_ry,
        );
        self.engine.set_silhouette_mode(ai.silhouette_mode.clamp(0, 1));
        self.engine.set_multi_person_mode(multi_person_active);
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

        let (skin_cb, skin_cr, head_rx, head_ry) =
            self.engine.calibrate_face_landmarks(snapshot_bgra, w, h, pts);

        cfg.segmentation.ai_selfie.face_calibrated = true;
        cfg.segmentation.ai_selfie.calib_skin_cb = skin_cb.clamp(60.0, 175.0);
        cfg.segmentation.ai_selfie.calib_skin_cr = skin_cr.clamp(115.0, 210.0);
        cfg.segmentation.ai_selfie.calib_head_rx = head_rx.clamp(0.04, 0.40);
        cfg.segmentation.ai_selfie.calib_head_ry = head_ry.clamp(0.06, 0.48);
        cfg.segmentation.ai_selfie.calib_landmarks = pts;

        let eye_mid_x = 0.5 * (pts[0].0 + pts[1].0);
        let eye_mid_y = 0.5 * (pts[0].1 + pts[1].1);
        let eye_to_chin = (pts[4].1 - eye_mid_y).abs().max(0.055);
        let face_cx = (0.30 * eye_mid_x + 0.35 * pts[2].0 + 0.20 * pts[3].0 + 0.15 * pts[4].0)
            .clamp(0.12, 0.88);
        let face_cy = (eye_mid_y + 0.105 * eye_to_chin).clamp(0.14, 0.82);
        self.tracked_subject_center = (face_cx, face_cy);

        let multi_active =
            self.multi_person_mode_active || cfg.is_plugin_enabled("multi_person_tracker");
        self.sync_face_and_silhouette_settings(cfg, multi_active);
        true
    }

    pub fn init_sink(&mut self, cfg: &SmartCamConfig) {
        unsafe {
            if !self.sink_handle.is_null() {
                smartcam_capture::windows_backend::c_api::smartcam_win_sink_destroy(self.sink_handle);
                self.sink_handle = std::ptr::null_mut();
            }
            let sh = smartcam_capture::windows_backend::c_api::smartcam_win_sink_create();
            if !sh.is_null() {
                let cname = std::ffi::CString::new(cfg.virtual_camera.friendly_name.as_str())
                    .unwrap_or_else(|_| std::ffi::CString::new("SmartCam").unwrap());
                let ok = smartcam_capture::windows_backend::c_api::smartcam_win_sink_init(
                    sh,
                    cname.as_ptr(),
                    cfg.framing_output.output_width,
                    cfg.framing_output.output_height,
                    cfg.camera_input.fps,
                );
                if ok != 0 {
                    self.sink_handle = sh;
                } else {
                    smartcam_capture::windows_backend::c_api::smartcam_win_sink_destroy(sh);
                }
            }
        }
        self.fallback_shm = FallbackSharedMemoryWriter::open(&cfg.virtual_camera.shared_memory_name);
    }

    pub fn open_hardware_camera(
        &mut self,
        index: u32,
        width: u32,
        height: u32,
        fps: u32,
    ) -> bool {
        self.close_hardware_camera();
        self.camera_busy_error = false;
        unsafe {
            let h = smartcam_capture::windows_backend::c_api::smartcam_win_camera_create();
            if !h.is_null() {
                self.cam_handle = h;
                let ok = smartcam_capture::windows_backend::c_api::smartcam_win_camera_open(
                    h, index, width, height, fps,
                );
                if ok != 0 {
                    self.camera_open = true;
                    return true;
                } else {
                    self.camera_busy_error = true;
                    smartcam_capture::windows_backend::c_api::smartcam_win_camera_destroy(h);
                    self.cam_handle = std::ptr::null_mut();
                }
            }
        }
        self.camera_open = false;
        false
    }

    pub fn close_hardware_camera(&mut self) {
        if !self.cam_handle.is_null() {
            unsafe {
                smartcam_capture::windows_backend::c_api::smartcam_win_camera_close(self.cam_handle);
                smartcam_capture::windows_backend::c_api::smartcam_win_camera_destroy(self.cam_handle);
            }
            self.cam_handle = std::ptr::null_mut();
        }
        self.camera_open = false;
    }

    pub fn read_camera_frame(
        &mut self,
        dst: &mut Vec<u8>,
        target_w: u32,
        target_h: u32,
    ) -> Option<(u32, u32)> {
        if !self.camera_open || self.cam_handle.is_null() {
            return None;
        }
        let max_bytes = (target_w.max(1920) * target_h.max(1080) * 4) as usize;
        if dst.len() < max_bytes {
            dst.resize(max_bytes, 0);
        }
        unsafe {
            let mut actual_w: u32 = 0;
            let mut actual_h: u32 = 0;
            let ok = smartcam_capture::windows_backend::c_api::smartcam_win_camera_read_frame(
                self.cam_handle,
                dst.as_mut_ptr(),
                dst.len() as u32,
                &mut actual_w,
                &mut actual_h,
            );
            if ok != 0 && actual_w > 0 && actual_h > 0 {
                return Some((actual_w, actual_h));
            }
        }
        None
    }

    pub fn is_consumer_active(&self) -> bool {
        if !self.sink_handle.is_null() {
            unsafe {
                return smartcam_capture::windows_backend::c_api::smartcam_win_sink_is_consumer_active(
                    self.sink_handle,
                ) != 0;
            }
        }
        true
    }

    pub fn push_frame_to_sink(&mut self, bgra: &[u8], width: u32, height: u32, fps: u32) {
        if !self.sink_handle.is_null() {
            unsafe {
                let ok = smartcam_capture::windows_backend::c_api::smartcam_win_sink_push_frame(
                    self.sink_handle,
                    bgra.as_ptr(),
                    width,
                    height,
                    width * 4,
                );
                if ok != 0 {
                    return;
                }
            }
        }
        if let Some(ref mut shm) = self.fallback_shm {
            shm.write_frame(bgra, width, height, fps);
        }
    }

    pub fn auto_calibrate_greenscreen(
        &mut self,
        frame: &[u8],
        width: u32,
        height: u32,
        sample_x: i32,
        sample_y: i32,
        cfg: &mut SmartCamConfig,
    ) -> (u8, u8, u8, u8) {
        let (hmin, hmax, smin, vmin) = self.engine.autocalibrate_greenscreen(
            frame,
            width,
            height,
            width * 4,
            true,
            sample_x,
            sample_y,
        );
        cfg.segmentation.green_screen.hue_min = hmin;
        cfg.segmentation.green_screen.hue_max = hmax;
        cfg.segmentation.green_screen.sat_min = smin;
        cfg.segmentation.green_screen.sat_max = 255;
        cfg.segmentation.green_screen.val_min = vmin;
        cfg.segmentation.green_screen.val_max = 255;
        self.update_greenscreen_config(cfg);
        (hmin, hmax, smin, vmin)
    }

    pub fn set_reference_background(&mut self, bg_bgra: &[u8], width: u32, height: u32) -> bool {
        let expected = (width * height * 4) as usize;
        if expected == 0 || bg_bgra.len() < expected {
            return false;
        }
        self.ref_bg_buf.clear();
        self.ref_bg_buf.extend_from_slice(&bg_bgra[..expected]);
        self.ref_bg_w = width;
        self.ref_bg_h = height;
        self.has_ref_bg_fallback = true;
        self.engine.set_reference_background(bg_bgra, width, height)
    }

    pub fn clear_reference_background(&mut self) {
        self.ref_bg_buf.clear();
        self.ref_bg_w = 0;
        self.ref_bg_h = 0;
        self.has_ref_bg_fallback = false;
        self.engine.clear_reference_background();
    }

    pub fn has_reference_background(&self) -> bool {
        self.engine.has_reference_background()
    }

    pub fn get_subject_center(&mut self) -> (f32, f32) {
        let (cx, cy) = self.engine.get_subject_center();
        self.tracked_subject_center = (cx, cy);
        (cx, cy)
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
            self.bg_cropped_w = 0;
            return true;
        }

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
            let (a_w, a_h) =
                resolve_aspect_ratio_pair(&cfg.framing_output.aspect_ratio, src_w, src_h);
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

        let strategy = if cfg.framing_output.crop_strategy == "letterbox" {
            CropStrategy::Letterbox
        } else {
            CropStrategy::CenterCrop
        };

        // 1. Crop & Scale using pure Rust smartcam-engine
        self.engine.crop_scale_bgra(
            raw_bgra,
            src_w,
            src_h,
            src_w * 4,
            &mut self.framed_buf,
            out_w,
            out_h,
            out_w * 4,
            aw,
            ah,
            strategy,
        );

        // Passthrough shortcut
        if cfg.segmentation.mode == "passthrough" {
            self.out_buf.copy_from_slice(&self.framed_buf);
            return (out_w, out_h);
        }

        // 2. Prepare Background Buffer
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
            }
        } else if cfg.compositor.background_mode == "blur" {
            let blur_r = cfg.compositor.background_blur_radius.clamp(2, 40);
            self.engine.generate_blurred_bg(
                &self.framed_buf,
                &mut self.bg_buf,
                out_w,
                out_h,
                out_w * 4,
                blur_r,
            );
        } else {
            let [r, g, b] = cfg.compositor.background_color_rgb;
            self.engine
                .fill_solid_bg(&mut self.bg_buf, out_w, out_h, out_w * 4, r, g, b, true);
        }

        // 3. Compute Foreground Alpha Mask
        if cfg.segmentation.mode == "green_screen" {
            self.engine.compute_greenscreen_mask(
                &self.framed_buf,
                out_w,
                out_h,
                out_w * 4,
                &mut self.mask_buf,
                out_w,
                true,
            );
            self.engine.suppress_green_spill(
                &mut self.framed_buf,
                &self.mask_buf,
                out_w,
                out_h,
                out_w * 4,
                out_w,
                true,
            );
        } else {
            let multi_active =
                self.multi_person_mode_active || cfg.is_plugin_enabled("multi_person_tracker");
            self.sync_face_and_silhouette_settings(cfg, multi_active);

            self.engine.compute_ai_mask(
                &self.framed_buf,
                out_w,
                out_h,
                out_w * 4,
                &mut self.mask_buf,
                out_w,
                true,
                cfg.ai_enabled,
            );
            let (cx, cy) = self.engine.get_subject_center();
            self.tracked_subject_center = (cx, cy);
        }

        // 4. Pure Rust Runtime SIMD Alpha Blending
        self.engine.blend_bgra(
            &self.framed_buf,
            &self.bg_buf,
            &self.mask_buf,
            &mut self.out_buf,
            out_w,
            out_h,
            out_w * 4,
            out_w * 4,
            out_w,
            out_w * 4,
        );

        (out_w, out_h)
    }
}

impl Drop for SmartCamPipelineInstance {
    fn drop(&mut self) {
        self.close_hardware_camera();
        if !self.sink_handle.is_null() {
            unsafe {
                smartcam_capture::windows_backend::c_api::smartcam_win_sink_destroy(self.sink_handle);
            }
            self.sink_handle = std::ptr::null_mut();
        }
        if !self.engine_handle.is_null() {
            unsafe {
                pure_engine_destroy(self.engine_handle);
            }
            self.engine_handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// Studio Presets, Social Crop/Zoom & Frame Generators
// ============================================================================

pub use smartcam_engine::crop_scaler::compute_social_crop_rect;
pub use smartcam_engine::crop_scaler::crop_zoom_image_social;

pub fn generate_studio_preset_bgra(
    preset_id: u8,
    width: u32,
    height: u32,
) -> (Vec<u8>, u32, u32) {
    let mut buf = vec![255u8; (width * height * 4) as usize];
    for y in 0..height {
        let ny = (y as f32) / (height.max(1) as f32);
        for x in 0..width {
            let nx = (x as f32) / (width.max(1) as f32);
            let dx = nx - 0.52;
            let dy = ny - 0.42;
            let vignette = (1.0 - (dx * dx * 0.95 + dy * dy * 1.15)).clamp(0.35, 1.0);

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
                    let r = (76.0 + 95.0 * (1.0 - ny * 0.5) + 110.0 * window_panel + 40.0 * shelf_accent) * vignette;
                    let g = (58.0 + 72.0 * (1.0 - ny * 0.5) + 92.0 * window_panel + 25.0 * shelf_accent) * vignette;
                    let b = (44.0 + 50.0 * (1.0 - ny * 0.4) + 75.0 * window_panel + 12.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
                3 => {
                    let r = (135.0 + 75.0 * (1.0 - ny * 0.45) + 45.0 * window_panel - 20.0 * shelf_accent) * vignette;
                    let g = (144.0 + 78.0 * (1.0 - ny * 0.45) + 48.0 * window_panel - 15.0 * shelf_accent) * vignette;
                    let b = (154.0 + 82.0 * (1.0 - ny * 0.40) + 52.0 * window_panel - 10.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
                4 => {
                    let r = (24.0 + 36.0 * (1.0 - ny * 0.6) + 64.0 * window_panel + 45.0 * shelf_accent) * vignette;
                    let g = (30.0 + 44.0 * (1.0 - ny * 0.6) + 58.0 * window_panel + 32.0 * shelf_accent) * vignette;
                    let b = (42.0 + 58.0 * (1.0 - ny * 0.5) + 52.0 * window_panel + 20.0 * shelf_accent) * vignette;
                    (r, g, b)
                }
                _ => {
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

pub use smartcam_engine::rgb_to_hsv_fast;

pub fn bgr_to_hsv(b: u8, g: u8, r: u8) -> (u8, u8, u8) {
    rgb_to_hsv_fast(r, g, b)
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
                buf[idx] = 142;
                buf[idx + 1] = 172;
                buf[idx + 2] = 222;
                buf[idx + 3] = 255;
            } else if in_shoulders {
                buf[idx] = 115;
                buf[idx + 1] = 78;
                buf[idx + 2] = 52;
                buf[idx + 3] = 255;
            } else {
                buf[idx] = 20;
                buf[idx + 1] = 215;
                buf[idx + 2] = 20;
                buf[idx + 3] = 255;
            }
        }
    }
}

// ============================================================================
// Direct Win32 Shared Memory Writer (SharedFrameHeader v1)
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
    const PAYLOAD_OFFSET: usize = std::mem::size_of::<SharedFrameHeaderRaw>();
    const TOTAL_BYTES: usize = Self::PAYLOAD_OFFSET + Self::MAX_FRAME_BYTES;

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
            let view =
                MapViewOfFile(hmap, FILE_MAP_ALL_ACCESS, 0, 0, Self::TOTAL_BYTES) as *mut u8;
            if view.is_null() {
                CloseHandle(hmap);
                return None;
            }
            let ev_name = to_wstring("Local\\SmartCam_MF_FrameEvent_v1");
            let hev = CreateEventW(NULL, FALSE, FALSE, ev_name.as_ptr());

            let hdr = &mut *(view as *mut SharedFrameHeaderRaw);
            if hdr.magic != 0x534D434D {
                std::ptr::write_bytes(view, 0, Self::PAYLOAD_OFFSET);
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
        if self.mapped_view.is_null()
            || bytes == 0
            || bytes > Self::MAX_FRAME_BYTES
            || bgra.len() < bytes
        {
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

            let payload = self.mapped_view.add(Self::PAYLOAD_OFFSET);
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

#[cfg(test)]
mod tests {
    use super::*;

    const CPP_SHARED_FRAME_HEADER_BYTES: usize = 120;

    fn read_u32(view: *const u8, offset: usize) -> u32 {
        unsafe { std::ptr::read_unaligned(view.add(offset) as *const u32) }
    }

    #[test]
    fn fallback_writer_puts_pixels_where_the_driver_reads_them() {
        let name = format!(r"Local\SmartCam_FallbackIpcTest_{}", std::process::id());
        let mut writer = FallbackSharedMemoryWriter::open(&name).expect("open fallback mapping");
        let (width, height) = (4u32, 2u32);
        let pixels: Vec<u8> = (1..=(width * height * 4) as u8).collect();
        writer.write_frame(&pixels, width, height, 30);

        unsafe {
            let wname = to_wstring(&name);
            let hmap = OpenFileMappingW(FILE_MAP_READ, 0, wname.as_ptr());
            assert!(!hmap.is_null(), "consumer could not open the mapping");
            let view = MapViewOfFile(
                hmap,
                FILE_MAP_READ,
                0,
                0,
                CPP_SHARED_FRAME_HEADER_BYTES + pixels.len(),
            ) as *const u8;
            assert!(!view.is_null(), "consumer could not map the view");

            assert_eq!(read_u32(view, 0), 0x534D_434D, "magic");
            assert_eq!(read_u32(view, 4), 1, "version");
            let seq = std::ptr::read_unaligned(view.add(8) as *const i64);
            assert!(
                seq > 0 && seq % 2 == 0,
                "sequence_counter {seq} must be even after a complete write"
            );
            assert_eq!(read_u32(view, 16), width, "width");
            assert_eq!(read_u32(view, 20), height, "height");
            assert_eq!(read_u32(view, 24), width * 4, "stride");

            let payload =
                std::slice::from_raw_parts(view.add(CPP_SHARED_FRAME_HEADER_BYTES), pixels.len());
            assert_eq!(payload, &pixels[..], "pixels are not at sizeof(SharedFrameHeader)");

            UnmapViewOfFile(view as *const c_void);
            CloseHandle(hmap);
        }
    }

    #[test]
    fn fallback_header_layout_matches_cpp_shared_frame_header() {
        assert_eq!(
            std::mem::size_of::<SharedFrameHeaderRaw>(),
            CPP_SHARED_FRAME_HEADER_BYTES
        );
        assert_eq!(
            std::mem::offset_of!(SharedFrameHeaderRaw, sequence_counter),
            8
        );
        assert_eq!(std::mem::offset_of!(SharedFrameHeaderRaw, width), 16);
        assert_eq!(std::mem::offset_of!(SharedFrameHeaderRaw, pixel_format), 36);
        assert_eq!(std::mem::offset_of!(SharedFrameHeaderRaw, timestamp_100ns), 40);
        assert_eq!(std::mem::offset_of!(SharedFrameHeaderRaw, frame_index), 48);
    }
}
