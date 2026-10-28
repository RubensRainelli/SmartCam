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

//! SmartCam v2.0 Pure Media Foundation In-Process COM Virtual Camera Source (`smartcam_mf_source.dll`).
//! Implements `IClassFactory`, `IMFMediaSourceEx`, `IMFMediaSource`, `IMFMediaStream`,
//! `IMFMediaEventGenerator`, and `IMFGetService` using the official `windows` crate.

#[cfg(windows)]
pub mod constants;
#[cfg(windows)]
pub mod dll;
#[cfg(windows)]
pub mod factory;
#[cfg(windows)]
pub mod source;
#[cfg(windows)]
pub mod stream;

pub use smartcam_capture::{
    SharedFrameHeader, DEFAULT_SHARED_EVENT_NAME, DEFAULT_SHARED_MAPPING_NAME,
    MAX_SHARED_FRAME_BYTES, SHARED_FRAME_HEADER_BYTES, SHARED_FRAME_MAGIC,
    SHARED_FRAME_VERSION, SMARTCAM_MF_CLSID_STR, TOTAL_SHARED_FRAME_BYTES,
};

#[cfg(windows)]
pub use constants::*;
#[cfg(windows)]
pub use dll::*;
#[cfg(windows)]
pub use factory::*;
#[cfg(windows)]
pub use source::*;
#[cfg(windows)]
pub use stream::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};
    use windows::{
        core::{Interface, IUnknown},
        Win32::{
            Foundation::S_OK,
            Media::MediaFoundation::{
                IMFMediaEventGenerator, IMFMediaSource, IMFMediaSourceEx,
                MFMEDIASOURCE_IS_LIVE, MFShutdown, MFStartup, MF_VERSION,
            },
            System::Com::{CoInitializeEx, CoUninitialize, IClassFactory, COINIT_MULTITHREADED},
        },
    };

    #[test]
    fn test_shared_frame_header_abi_and_cross_crate_roundtrip() {
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

        // Test frame written by smartcam-capture producer is read back identically by mf-source consumer
        let map_name = format!(r"Local\SmartCam_MfSourceTest_{}", std::process::id());
        let ev_name = format!(r"Local\SmartCam_MfSourceEv_{}", std::process::id());

        let producer = smartcam_capture::SharedFrameIpc::open_or_create(&map_name, &ev_name, true)
            .expect("open producer");
        let consumer = smartcam_capture::SharedFrameIpc::open_or_create(&map_name, &ev_name, false)
            .expect("open consumer");

        let (width, height) = (32u32, 16u32);
        let pixels: Vec<u8> = (0..(width * height * 4) as usize)
            .map(|i| (i * 7 % 256) as u8)
            .collect();

        assert!(producer.write_frame(&pixels, width, height, width * 4, 30, 1, 9876543210, 42));

        let mut read_buf = vec![0u8; (width * height * 4) as usize];
        let mut r_w = 0u32;
        let mut r_h = 0u32;
        let mut r_ts = 0u64;

        assert!(consumer.read_frame(&mut read_buf, &mut r_w, &mut r_h, &mut r_ts));
        assert_eq!(r_w, width);
        assert_eq!(r_h, height);
        assert_eq!(r_ts, 9876543210);
        assert_eq!(read_buf, pixels);
    }

    #[test]
    fn test_dll_get_class_object_and_com_interfaces() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, 0);

            // 1. DllGetClassObject for CLSID_SmartCamMfVirtualCameraSource
            let mut factory_ptr = std::ptr::null_mut();
            let hr = DllGetClassObject(
                &CLSID_SMARTCAM_MF_SOURCE,
                &IClassFactory::IID,
                &mut factory_ptr,
            );
            assert_eq!(hr, S_OK, "DllGetClassObject must succeed with S_OK");
            assert!(!factory_ptr.is_null(), "factory pointer must not be null");

            let factory: IClassFactory = IClassFactory::from_raw(factory_ptr);

            // 2. Calling CreateInstance returns a valid COM object
            let unk: IUnknown = factory.CreateInstance(None).expect("CreateInstance must succeed");

            // 3. Responds to QueryInterface: IMFMediaSource
            let source: IMFMediaSource = unk.cast().expect("QueryInterface for IMFMediaSource must succeed");

            // 4. Responds to QueryInterface: IMFMediaSourceEx
            let source_ex: IMFMediaSourceEx = unk.cast().expect("QueryInterface for IMFMediaSourceEx must succeed");
            assert_eq!(source_ex.GetCharacteristics().expect("GetCharacteristics on source_ex"), MFMEDIASOURCE_IS_LIVE.0 as u32);

            // 5. Responds to QueryInterface: IMFMediaEventGenerator
            let _: IMFMediaEventGenerator = unk.cast().expect("QueryInterface for IMFMediaEventGenerator must succeed");

            // 6. Supports GetCharacteristics(MFMEDIASOURCE_IS_LIVE)
            let chars = source.GetCharacteristics().expect("GetCharacteristics must succeed");
            assert_eq!(chars, MFMEDIASOURCE_IS_LIVE.0 as u32, "Characteristics must be MFMEDIASOURCE_IS_LIVE");

            // 7. Supports CreatePresentationDescriptor
            let pres_desc = source.CreatePresentationDescriptor().expect("CreatePresentationDescriptor must succeed");
            let stream_count = pres_desc.GetStreamDescriptorCount().expect("GetStreamDescriptorCount must succeed");
            assert_eq!(stream_count, 1, "Virtual camera must have exactly 1 stream descriptor");

            // Clean shutdown
            let _ = source.Shutdown();
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
