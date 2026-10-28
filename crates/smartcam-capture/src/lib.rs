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

//! SmartCam v2.0 Cross-Platform Camera Capture & Virtual Camera Sink (`smartcam-capture`).
//! Uses pure Media Foundation on Windows (with zero DirectShow legacy), V4L2 on Linux,
//! and CoreMediaIO on macOS, plus the 120-byte `SharedFrameHeader` IPC contract.

pub mod header;
pub mod ipc;
pub mod ir;
pub mod types;

#[cfg(windows)]
pub mod windows_backend;

#[cfg(target_os = "linux")]
pub mod linux_backend;

#[cfg(target_os = "macos")]
pub mod macos_backend;

pub use header::{
    SharedFrameHeader, DEFAULT_SHARED_EVENT_NAME, DEFAULT_SHARED_MAPPING_NAME,
    MAX_SHARED_FRAME_BYTES, SHARED_FRAME_HEADER_BYTES, SHARED_FRAME_MAGIC, SHARED_FRAME_VERSION,
    SMARTCAM_MF_CLSID_STR, TOTAL_SHARED_FRAME_BYTES,
};
pub use ipc::SharedFrameIpc;
pub use ir::is_ir_camera_name;
pub use types::{
    CameraCaptureConfig, CameraDeviceInfo, CameraSource, VirtualCameraSink, VirtualCameraSinkConfig,
};

#[cfg(windows)]
pub use windows_backend::{enumerate_camera_devices, MfCameraCapture, MfVirtualCameraSink};

#[cfg(target_os = "linux")]
pub use linux_backend::{enumerate_camera_devices, V4l2CameraCapture, V4l2VirtualCameraSink};

#[cfg(target_os = "macos")]
pub use macos_backend::{enumerate_camera_devices, CmioCameraCapture, CmioVirtualCameraSink};

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn test_shared_frame_header_abi_layout() {
        assert_eq!(size_of::<SharedFrameHeader>(), 120);
        assert_eq!(align_of::<SharedFrameHeader>(), 8);

        let uninit = std::mem::MaybeUninit::<SharedFrameHeader>::uninit();
        let base = uninit.as_ptr() as usize;

        unsafe {
            let p_magic = std::ptr::addr_of!((*uninit.as_ptr()).magic) as usize;
            let p_ver = std::ptr::addr_of!((*uninit.as_ptr()).version) as usize;
            let p_seq = std::ptr::addr_of!((*uninit.as_ptr()).sequence_counter) as usize;
            let p_w = std::ptr::addr_of!((*uninit.as_ptr()).width) as usize;
            let p_h = std::ptr::addr_of!((*uninit.as_ptr()).height) as usize;
            let p_stride = std::ptr::addr_of!((*uninit.as_ptr()).stride) as usize;
            let p_fps_num = std::ptr::addr_of!((*uninit.as_ptr()).fps_num) as usize;
            let p_fps_den = std::ptr::addr_of!((*uninit.as_ptr()).fps_den) as usize;
            let p_pixfmt = std::ptr::addr_of!((*uninit.as_ptr()).pixel_format) as usize;
            let p_ts = std::ptr::addr_of!((*uninit.as_ptr()).timestamp_100ns) as usize;
            let p_fi = std::ptr::addr_of!((*uninit.as_ptr()).frame_index) as usize;
            let p_hb = std::ptr::addr_of!((*uninit.as_ptr()).last_consumer_heartbeat_ms) as usize;
            let p_cr = std::ptr::addr_of!((*uninit.as_ptr()).consumer_requests) as usize;
            let p_res = std::ptr::addr_of!((*uninit.as_ptr()).reserved) as usize;

            assert_eq!(p_magic - base, 0, "magic offset");
            assert_eq!(p_ver - base, 4, "version offset");
            assert_eq!(p_seq - base, 8, "sequence_counter offset");
            assert_eq!(p_w - base, 16, "width offset");
            assert_eq!(p_h - base, 20, "height offset");
            assert_eq!(p_stride - base, 24, "stride offset");
            assert_eq!(p_fps_num - base, 28, "fps_num offset");
            assert_eq!(p_fps_den - base, 32, "fps_den offset");
            assert_eq!(p_pixfmt - base, 36, "pixel_format offset");
            assert_eq!(p_ts - base, 40, "timestamp_100ns offset");
            assert_eq!(p_fi - base, 48, "frame_index offset");
            assert_eq!(p_hb - base, 56, "last_consumer_heartbeat_ms offset");
            assert_eq!(p_cr - base, 64, "consumer_requests offset");
            assert_eq!(p_res - base, 72, "reserved offset");
        }
    }

    #[test]
    fn test_ipc_producer_consumer_roundtrip() {
        let mapping_name = format!(r"Local\SmartCam_TestCaptureIPC_{}", std::process::id());
        let event_name = format!(r"Local\SmartCam_TestCaptureEv_{}", std::process::id());

        let producer = SharedFrameIpc::open_or_create(&mapping_name, &event_name, true)
            .expect("open producer");
        let consumer = SharedFrameIpc::open_or_create(&mapping_name, &event_name, false)
            .expect("open consumer");

        let (width, height) = (16u32, 8u32);
        let test_pixels: Vec<u8> = (0..(width * height * 4) as usize)
            .map(|i| (i % 255) as u8)
            .collect();

        let ok = producer.write_frame(&test_pixels, width, height, width * 4, 30, 1, 1234567890, 1);
        assert!(ok, "producer write_frame must succeed");

        let mut read_buf = vec![0u8; (width * height * 4) as usize];
        let mut r_w = 0u32;
        let mut r_h = 0u32;
        let mut r_ts = 0u64;

        let read_ok = consumer.read_frame(&mut read_buf, &mut r_w, &mut r_h, &mut r_ts);
        assert!(read_ok, "consumer read_frame must succeed");
        assert_eq!(r_w, width);
        assert_eq!(r_h, height);
        assert_eq!(r_ts, 1234567890);
        assert_eq!(
            read_buf, test_pixels,
            "read pixel buffer must match written data exactly"
        );

        // Verify consumer heartbeat
        assert!(producer.is_consumer_active(5000));
    }
}
