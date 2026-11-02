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
//! Implements `IClassFactory`, `IMFActivate`, `IMFAttributes`, `IMFMediaSourceEx`,
//! `IMFMediaSource`, `IMFMediaStream`, `IMFMediaEventGenerator`, and `IMFGetService`
//! using the official `windows` crate.

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
    MAX_SHARED_FRAME_BYTES, SHARED_FRAME_HEADER_BYTES, SHARED_FRAME_MAGIC, SHARED_FRAME_VERSION,
    SMARTCAM_MF_CLSID_STR, TOTAL_SHARED_FRAME_BYTES,
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
    #[cfg(windows)]
    use windows::{
        core::{IUnknown, Interface, GUID},
        Win32::{
            Foundation::{BOOL, S_OK},
            Media::MediaFoundation::{
                IMFActivate, IMFAttributes, IMFMediaEventGenerator, IMFMediaSource,
                IMFMediaSourceEx, IMFMediaStream, IMFStreamDescriptor,
                MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS, MFShutdown, MFStartup,
                MFMEDIASOURCE_IS_LIVE, MF_DEVICEMFT_SENSORPROFILE_COLLECTION,
                MF_DEVICESTREAM_STREAM_CATEGORY, MF_VERSION,
            },
            System::Com::{CoInitializeEx, CoUninitialize, IClassFactory, COINIT_MULTITHREADED},
        },
    };
    #[cfg(windows)]
    use windows_core::PROPVARIANT;

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

        #[cfg(windows)]
        {
            // Test frame written by smartcam-capture producer is read back identically by mf-source consumer
            let map_name = format!(r"Local\SmartCam_MfSourceTest_{}", std::process::id());
            let ev_name = format!(r"Local\SmartCam_MfSourceEv_{}", std::process::id());

            let producer =
                smartcam_capture::SharedFrameIpc::open_or_create(&map_name, &ev_name, true)
                    .expect("open producer");
            let consumer =
                smartcam_capture::SharedFrameIpc::open_or_create(&map_name, &ev_name, false)
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
    }

    #[cfg(windows)]
    #[test]
    fn test_dll_get_class_object_and_com_interfaces() {
        unsafe {
            let pid = std::process::id();
            smartcam_capture::ipc::set_default_ipc_names_for_test(Some((
                format!(r"Local\SmartCam_MfComTest_{pid}"),
                format!(r"Local\SmartCam_MfComEv_{pid}"),
            )));

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

            // 2. FrameServer activates via IID_IMFActivate
            let activate: IMFActivate = factory
                .CreateInstance(None)
                .expect("CreateInstance(IID_IMFActivate) must succeed");

            let source: IMFMediaSource = activate
                .ActivateObject()
                .expect("IMFActivate::ActivateObject(IID_IMFMediaSource) must succeed");

            // 3. Responds to QueryInterface: IMFMediaSourceEx & IMFAttributes
            let source_ex: IMFMediaSourceEx = source
                .cast()
                .expect("QueryInterface for IMFMediaSourceEx must succeed");
            let source_attrs: IMFAttributes = source
                .cast()
                .expect("QueryInterface for IMFAttributes on source must succeed");
            let _sensor_profile_unk: IUnknown = source_attrs
                .GetUnknown(&MF_DEVICEMFT_SENSORPROFILE_COLLECTION)
                .expect("MF_DEVICEMFT_SENSORPROFILE_COLLECTION must be present on source");

            // 4. Responds to QueryInterface: IMFMediaEventGenerator
            let _: IMFMediaEventGenerator = source
                .cast()
                .expect("QueryInterface for IMFMediaEventGenerator must succeed");

            // 5. Supports GetCharacteristics(MFMEDIASOURCE_IS_LIVE)
            let chars = source
                .GetCharacteristics()
                .expect("GetCharacteristics must succeed");
            assert_eq!(
                chars, MFMEDIASOURCE_IS_LIVE.0 as u32,
                "Characteristics must be MFMEDIASOURCE_IS_LIVE"
            );

            // 6. Supports CreatePresentationDescriptor & stream attributes
            let pres_desc = source
                .CreatePresentationDescriptor()
                .expect("CreatePresentationDescriptor must succeed");
            let stream_count = pres_desc
                .GetStreamDescriptorCount()
                .expect("GetStreamDescriptorCount must succeed");
            assert_eq!(
                stream_count, 1,
                "Virtual camera must have exactly 1 stream descriptor"
            );

            let mut selected = BOOL::default();
            let mut sd_opt: Option<IMFStreamDescriptor> = None;
            pres_desc
                .GetStreamDescriptorByIndex(0, &mut selected, &mut sd_opt)
                .expect("GetStreamDescriptorByIndex(0) must succeed");
            assert!(selected.as_bool());
            let sd = sd_opt.expect("StreamDescriptor must be Some");
            let cat = sd
                .GetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY)
                .expect("MF_DEVICESTREAM_STREAM_CATEGORY on stream descriptor");
            assert_eq!(cat, PINNAME_VIDEO_CAPTURE);

            // 7. Start source -> MENewStream -> IMFMediaStream::GetMediaSource -> RequestSample
            let start_pos = PROPVARIANT::default();
            source
                .Start(&pres_desc, &GUID::zeroed(), &start_pos)
                .expect("IMFMediaSource::Start must succeed");

            let ev1 = source
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("GetEvent(MENewStream)");
            let ev1_val = ev1.GetValue().expect("MENewStream value");
            let stream_unk: IUnknown = IUnknown::try_from(&ev1_val).expect("MENewStream IUnknown");
            let stream: IMFMediaStream = stream_unk.cast().expect("cast to IMFMediaStream");
            let _: IMFAttributes = stream.cast().expect("stream must implement IMFAttributes");

            let parent_src = stream
                .GetMediaSource()
                .expect("IMFMediaStream::GetMediaSource must succeed");
            assert_eq!(
                parent_src.as_raw(),
                source.as_raw(),
                "GetMediaSource must return the exact same COM IMFMediaSource identity"
            );

            // Drain MESourceStarted and MEStreamStarted, then RequestSample
            let _ = source
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("GetEvent(MESourceStarted)");
            let _ = stream
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("GetEvent(MEStreamStarted)");

            stream
                .RequestSample(None)
                .expect("IMFMediaStream::RequestSample must succeed");
            let sample_ev = stream
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("GetEvent(MEMediaSample)");
            assert_eq!(
                sample_ev.GetType().expect("GetType"),
                windows::Win32::Media::MediaFoundation::MEMediaSample.0 as u32
            );

            // Clean shutdown
            let _ = source.Stop();
            let _ = source.Shutdown();
            let _ = source_ex;
            let _ = MFShutdown();
            CoUninitialize();
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_no_green_banding_or_standby_flash_on_seqlock_contention() {
        use std::sync::atomic::Ordering;
        use windows::Win32::Media::MediaFoundation::IMFSample;

        let mut buf = vec![0u8; 240 * 8 * 4];
        SmartCamMfMediaStream::render_standby_pattern(buf.as_mut_ptr(), 240, 8, 5);
        for px in buf.chunks_exact(4) {
            assert_eq!(
                px,
                &[38, 24, 18, 255],
                "Standby pattern must be uniform dark slate without vertical cyan/green bars"
            );
        }

        unsafe {
            let pid = std::process::id();
            smartcam_capture::ipc::set_default_ipc_names_for_test(Some((
                format!(r"Local\SmartCam_MfComTest_{pid}"),
                format!(r"Local\SmartCam_MfComEv_{pid}"),
            )));

            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, 0);

            let producer = smartcam_capture::SharedFrameIpc::open_default_producer()
                .expect("open default producer");
            let width = DEFAULT_STREAM_WIDTH;
            let height = DEFAULT_STREAM_HEIGHT;
            let mut live_frame = vec![0u8; (width * height * 4) as usize];
            for px in live_frame.chunks_exact_mut(4) {
                px[0] = 77;  // B
                px[1] = 123; // G
                px[2] = 201; // R
                px[3] = 255; // A
            }
            assert!(producer.write_frame(&live_frame, width, height, width * 4, 30, 1, 111_222, 1));

            let source = SmartCamMfMediaSource::create_com().expect("create_com");
            let pres_desc = source.CreatePresentationDescriptor().expect("pres_desc");
            let start_pos = PROPVARIANT::default();
            source
                .Start(&pres_desc, &GUID::zeroed(), &start_pos)
                .expect("Start");

            let ev1 = source
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("MENewStream");
            let ev1_val = ev1.GetValue().expect("MENewStream val");
            let stream_unk: IUnknown = IUnknown::try_from(&ev1_val).expect("stream IUnknown");
            let stream: IMFMediaStream = stream_unk.cast().expect("IMFMediaStream");
            let _ = source
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("MESourceStarted");
            let _ = stream
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("MEStreamStarted");

            // 1st sample: reads valid live frame
            assert!(producer.write_frame(&live_frame, width, height, width * 4, 30, 1, 111_222, 1));
            stream.RequestSample(None).expect("RequestSample 1");
            let sample_ev1 = stream
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("MEMediaSample 1");
            let sample_val1 = sample_ev1.GetValue().expect("sample val 1");
            let sample1: IMFSample =
                IUnknown::try_from(&sample_val1).unwrap().cast().unwrap();
            let mb1 = sample1.ConvertToContiguousBuffer().unwrap();
            let mut ptr1 = std::ptr::null_mut();
            mb1.Lock(&mut ptr1, None, None).unwrap();
            let first_px: [u8; 4] = std::slice::from_raw_parts(ptr1, 4).try_into().unwrap();
            let _ = mb1.Unlock();

            // Simulate producer mid-write contention (odd sequence_counter) while FrameServer requests a sample
            if let Some(hdr) = producer.header() {
                hdr.sequence_counter.fetch_or(1, Ordering::SeqCst);
            }

            stream.RequestSample(None).expect("RequestSample 2 during contention");
            let sample_ev2 = stream
                .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                .expect("MEMediaSample 2");
            let sample_val2 = sample_ev2.GetValue().expect("sample val 2");
            let sample2: IMFSample =
                IUnknown::try_from(&sample_val2).unwrap().cast().unwrap();
            let mb2 = sample2.ConvertToContiguousBuffer().unwrap();
            let mut ptr2 = std::ptr::null_mut();
            mb2.Lock(&mut ptr2, None, None).unwrap();
            let second_px: [u8; 4] = std::slice::from_raw_parts(ptr2, 4).try_into().unwrap();
            assert_eq!(
                second_px, first_px,
                "Stream must hold the last valid live frame during seqlock contention instead of flashing standby!"
            );
            let _ = mb2.Unlock();

            if let Some(hdr) = producer.header() {
                let s = hdr.sequence_counter.load(Ordering::SeqCst);
                if (s & 1) != 0 {
                    hdr.sequence_counter.fetch_add(1, Ordering::SeqCst);
                }
            }

            let _ = source.Stop();
            let _ = source.Shutdown();
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
