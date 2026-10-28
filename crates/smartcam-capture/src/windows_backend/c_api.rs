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

use std::ffi::{c_char, c_void, CStr};

use crate::{
    types::{CameraCaptureConfig, CameraSource, VirtualCameraSink, VirtualCameraSinkConfig},
    windows_backend::{
        capture::{enumerate_camera_devices, MfCameraCapture},
        sink::MfVirtualCameraSink,
    },
};

struct CameraHandle {
    capture: MfCameraCapture,
    scratch: Vec<u8>,
}

struct SinkHandle {
    sink: MfVirtualCameraSink,
}

unsafe fn copy_str_to_buf(src: &str, dst: *mut c_char, cap: u32) {
    if dst.is_null() || cap == 0 {
        return;
    }
    let bytes = src.as_bytes();
    let n = bytes.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst as *mut u8, n);
    *dst.add(n) = 0;
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_get_camera_count() -> u32 {
    enumerate_camera_devices().len() as u32
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_get_camera_info(
    index: u32,
    out_name: *mut c_char,
    name_cap: u32,
    out_symlink: *mut c_char,
    sym_cap: u32,
    out_is_infrared: *mut i32,
) -> i32 {
    let devices = enumerate_camera_devices();
    if (index as usize) >= devices.len() {
        return 0;
    }
    let dev = &devices[index as usize];
    copy_str_to_buf(&dev.friendly_name, out_name, name_cap);
    copy_str_to_buf(&dev.symbolic_link, out_symlink, sym_cap);
    if !out_is_infrared.is_null() {
        *out_is_infrared = if dev.is_infrared { 1 } else { 0 };
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_camera_create() -> *mut c_void {
    let handle = Box::new(CameraHandle {
        capture: MfCameraCapture::new(),
        scratch: Vec::new(),
    });
    Box::into_raw(handle) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_camera_open(
    handle: *mut c_void,
    index: u32,
    width: u32,
    height: u32,
    fps: u32,
) -> i32 {
    if handle.is_null() {
        return 0;
    }
    let h = &mut *(handle as *mut CameraHandle);
    let cfg = CameraCaptureConfig {
        device_index: index,
        device_id: String::new(),
        width: if width > 0 { width } else { 1280 },
        height: if height > 0 { height } else { 720 },
        target_fps: if fps > 0 { fps } else { 30 },
        bit_depth: 32,
    };
    if h.capture.open(&cfg) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_camera_read_frame(
    handle: *mut c_void,
    dst_bgra: *mut u8,
    dst_cap: u32,
    out_actual_w: *mut u32,
    out_actual_h: *mut u32,
) -> i32 {
    if handle.is_null() || dst_bgra.is_null() {
        return 0;
    }
    let h = &mut *(handle as *mut CameraHandle);
    if let Some((w, h_res)) = h.capture.read_frame(&mut h.scratch) {
        let needed = (w as usize) * (h_res as usize) * 4;
        if !out_actual_w.is_null() {
            *out_actual_w = w;
        }
        if !out_actual_h.is_null() {
            *out_actual_h = h_res;
        }
        if needed > (dst_cap as usize) || h.scratch.len() < needed {
            return 0;
        }
        std::ptr::copy_nonoverlapping(h.scratch.as_ptr(), dst_bgra, needed);
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_camera_close(handle: *mut c_void) {
    if !handle.is_null() {
        let h = &mut *(handle as *mut CameraHandle);
        h.capture.close();
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_camera_destroy(handle: *mut c_void) {
    if !handle.is_null() {
        let mut boxed = Box::from_raw(handle as *mut CameraHandle);
        boxed.capture.close();
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_sink_create() -> *mut c_void {
    let handle = Box::new(SinkHandle {
        sink: MfVirtualCameraSink::new(),
    });
    Box::into_raw(handle) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_sink_init(
    handle: *mut c_void,
    friendly_name: *const c_char,
    width: u32,
    height: u32,
    fps: u32,
) -> i32 {
    if handle.is_null() {
        return 0;
    }
    let h = &mut *(handle as *mut SinkHandle);
    let name_str = if !friendly_name.is_null() {
        CStr::from_ptr(friendly_name).to_string_lossy().to_string()
    } else {
        "SmartCam".to_string()
    };
    let cfg = VirtualCameraSinkConfig {
        friendly_name: if name_str.is_empty() {
            "SmartCam".to_string()
        } else {
            name_str
        },
        shared_memory_name: crate::header::DEFAULT_SHARED_MAPPING_NAME.to_string(),
        width: if width > 0 { width } else { 1440 },
        height: if height > 0 { height } else { 1080 },
        fps: if fps > 0 { fps } else { 30 },
    };
    if h.sink.initialize(&cfg) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_sink_push_frame(
    handle: *mut c_void,
    src_bgra: *const u8,
    width: u32,
    height: u32,
    _stride: u32,
) -> i32 {
    if handle.is_null() || src_bgra.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let h = &mut *(handle as *mut SinkHandle);
    let frame_len = (width as usize) * (height as usize) * 4;
    let slice = std::slice::from_raw_parts(src_bgra, frame_len);
    if h.sink.push_frame(slice, width, height, 30) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_sink_is_consumer_active(handle: *mut c_void) -> i32 {
    if handle.is_null() {
        return 0;
    }
    let h = &*(handle as *const SinkHandle);
    if h.sink.is_consumer_active() {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_sink_destroy(handle: *mut c_void) {
    if !handle.is_null() {
        let mut boxed = Box::from_raw(handle as *mut SinkHandle);
        boxed.sink.shutdown();
    }
}

#[no_mangle]
pub unsafe extern "C" fn smartcam_win_sink_Destroy(handle: *mut c_void) {
    smartcam_win_sink_destroy(handle);
}
