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
pub use windows_backend::{
    ensure_system_virtual_camera_started, enumerate_camera_devices, remove_system_virtual_camera,
    MfCameraCapture, MfVirtualCameraSink,
};

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
        #[cfg(windows)]
        let consumer = SharedFrameIpc::open_or_create(&mapping_name, &event_name, false)
            .expect("open consumer");
        #[cfg(not(windows))]
        let consumer = &producer;

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

    #[cfg(windows)]
    #[test]
    fn test_live_vcam_frameserver_and_source_reader_roundtrip() {
        use std::ffi::c_void;
        use windows::{
            core::PWSTR,
            Win32::{
                Media::MediaFoundation::{
                    IMFActivate, IMFMediaSource, MFCreateAttributes,
                    MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources,
                    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
                },
                System::Com::CoTaskMemFree,
            },
        };

        let mut sink = MfVirtualCameraSink::new();
        assert!(sink.initialize(&VirtualCameraSinkConfig {
            friendly_name: "SmartCam".to_string(),
            shared_memory_name: DEFAULT_SHARED_MAPPING_NAME.to_string(),
            width: 1440,
            height: 1080,
            fps: 30,
        }));

        let width = 1440u32;
        let height = 1080u32;
        let mut frame = vec![0u8; (width * height * 4) as usize];
        for px in frame.chunks_exact_mut(4) {
            px[0] = 42;  // B
            px[1] = 188; // G
            px[2] = 212; // R
            px[3] = 255; // A
        }
        assert!(sink.push_frame(&frame, width, height, 30));

        unsafe {
            let mut attr = None;
            MFCreateAttributes(&mut attr, 1).expect("MFCreateAttributes");
            let attr = attr.unwrap();
            attr.SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .expect("SetGUID");

            let mut pp_devices: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            if MFEnumDeviceSources(&attr, &mut pp_devices, &mut count).is_ok()
                && !pp_devices.is_null()
            {
                let activates = std::slice::from_raw_parts(pp_devices, count as usize);
                let mut smartcam_activates = Vec::new();
                for act in activates.iter().flatten() {
                    let mut name_ptr = PWSTR::null();
                    let mut name_len = 0u32;
                    if act
                        .GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                            &mut name_ptr,
                            &mut name_len,
                        )
                        .is_ok()
                        && !name_ptr.is_null()
                    {
                        let slice = std::slice::from_raw_parts(name_ptr.0, name_len as usize);
                        let name = String::from_utf16_lossy(slice);
                        CoTaskMemFree(Some(name_ptr.0 as *mut c_void));
                        println!("[test_live_vcam] Enumerated device: {}", name);
                        assert!(
                            !name.contains("SmartCam Virtual Camera"),
                            "Legacy 'SmartCam Virtual Camera' name must not exist"
                        );
                        if name == "SmartCam" || name.starts_with("SmartCam (") {
                            smartcam_activates.push(act.clone());
                        }
                    }
                }
                CoTaskMemFree(Some(pp_devices as *mut c_void));

                assert_eq!(
                    smartcam_activates.len(),
                    1,
                    "Exactly 1 SmartCam virtual camera must be enumerated"
                );
                {
                    let t0 = std::time::Instant::now();
                    let vcam_source: IMFMediaSource = smartcam_activates[0]
                        .ActivateObject()
                        .expect("ActivateObject on SmartCam virtual camera");
                    println!("[test_live_vcam] +{:?} ActivateObject done", t0.elapsed());
                    let reader = MFCreateSourceReaderFromMediaSource(&vcam_source, None)
                        .expect("MFCreateSourceReaderFromMediaSource on SmartCam");
                    println!("[test_live_vcam] +{:?} MFCreateSourceReaderFromMediaSource done", t0.elapsed());

                    let mut got_sample = None;
                    for attempt in 1..=5 {
                        let _ = sink.push_frame(&frame, width, height, 30);
                        println!("[test_live_vcam] +{:?} calling ReadSample #{}", t0.elapsed(), attempt);
                        let mut actual_stream = 0u32;
                        let mut flags = 0u32;
                        let mut ts = 0i64;
                        let mut sample = None;
                        reader
                            .ReadSample(
                                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                                0,
                                Some(&mut actual_stream),
                                Some(&mut flags),
                                Some(&mut ts),
                                Some(&mut sample),
                            )
                            .expect("IMFSourceReader::ReadSample from SmartCam");
                        println!(
                            "[test_live_vcam] +{:?} ReadSample #{}: stream={} flags=0x{:x} ts={} has_sample={}",
                            t0.elapsed(),
                            attempt,
                            actual_stream,
                            flags,
                            ts,
                            sample.is_some()
                        );
                        if sample.is_some() {
                            got_sample = sample;
                            break;
                        }
                    }
                    let sample = got_sample.expect("ReadSample must return a non-null IMFSample");
                    let buf = sample
                        .ConvertToContiguousBuffer()
                        .expect("ConvertToContiguousBuffer");
                    let mut ptr = std::ptr::null_mut();
                    let mut cur_len = 0u32;
                    buf.Lock(&mut ptr, None, Some(&mut cur_len)).expect("Lock");
                    assert!(
                        cur_len >= 1280 * 720 * 3 / 2,
                        "Sample buffer length ({}) must be a valid HD/FHD frame",
                        cur_len
                    );
                    let _ = buf.Unlock();
                }
            }
        }

        sink.shutdown();
    }
}
