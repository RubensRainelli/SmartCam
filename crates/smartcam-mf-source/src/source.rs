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

use std::sync::{Arc, Mutex};
use windows::{
    core::{implement, w, IUnknown, Interface, Result, GUID, HRESULT, PCWSTR, PWSTR},
    Win32::{
        Foundation::{BOOL, E_INVALIDARG, E_POINTER, S_OK},
        Media::MediaFoundation::{
            IMFActivate, IMFActivate_Impl, IMFAsyncCallback, IMFAsyncResult, IMFAttributes,
            IMFAttributes_Impl, IMFGetService, IMFGetService_Impl, IMFMediaEvent,
            IMFMediaEventGenerator_Impl, IMFMediaEventQueue, IMFMediaSource, IMFMediaSourceEx,
            IMFMediaSourceEx_Impl, IMFMediaSource_Impl, IMFMediaStream, IMFMediaType,
            IMFPresentationDescriptor, IMFSampleAllocatorControl, IMFSampleAllocatorControl_Impl,
            MENewStream, MESourcePaused, MESourceStarted, MESourceStopped, MEUpdatedStream,
            MFCreateAttributes, MFCreateEventQueue, MFCreateMediaType,
            MFCreatePresentationDescriptor, MFCreateSensorProfile, MFCreateSensorProfileCollection,
            MFCreateStreamDescriptor, MFGetSystemTime, MFMediaType_Video, MFNominalRange_0_255,
            MFSampleAllocatorUsage, MFSampleAllocatorUsage_UsesProvidedAllocator,
            MFT_TRANSFORM_CLSID_Attribute, MFVideoFormat_NV12, MFVideoFormat_RGB32,
            MFVideoInterlace_Progressive, MFVideoPrimaries_BT709, MFVideoTransFunc_709,
            MFVideoTransferMatrix_BT709, MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS,
            MFMEDIASOURCE_IS_LIVE, MF_ATTRIBUTES_MATCH_TYPE, MF_ATTRIBUTE_TYPE,
            MF_DEVICEMFT_SENSORPROFILE_COLLECTION, MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES,
            MF_DEVICESTREAM_FRAMESERVER_SHARED, MF_DEVICESTREAM_STREAM_CATEGORY,
            MF_DEVICESTREAM_STREAM_ID, MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_MAX_BUFFERS, MF_E_SHUTDOWN,
            MF_E_UNSUPPORTED_SERVICE, MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AVG_BITRATE,
            MF_MT_DEFAULT_STRIDE, MF_MT_FIXED_SIZE_SAMPLES, MF_MT_FRAME_RATE,
            MF_MT_FRAME_RATE_RANGE_MAX, MF_MT_FRAME_RATE_RANGE_MIN, MF_MT_FRAME_SIZE,
            MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SAMPLE_SIZE,
            MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_NOMINAL_RANGE,
            MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX,
            MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES,
        },
    },
};
use windows_core::{IUnknownImpl, PROPVARIANT};

use crate::{
    constants::{
        trace_mf, IKsControl, IKsControl_Impl, CLSID_SMARTCAM_MF_SOURCE, DEFAULT_STREAM_FPS,
        DEFAULT_STREAM_HEIGHT, DEFAULT_STREAM_WIDTH, HR_ERROR_SET_NOT_FOUND, PINNAME_VIDEO_CAPTURE,
    },
    stream::SmartCamMfMediaStream,
    factory::ModuleRef,
};

// KSCAMERAPROFILE_Legacy {B4794D77-2B21-4090-B02B-707D6A0E035C}
const KSCAMERAPROFILE_LEGACY: GUID = GUID::from_u128(0xb4794d77_2b21_4090_b02b_707d6a0e035c);
// KSCAMERAPROFILE_HighFrameRate {C996873B-C319-4A28-AD21-225F3B786F87}
const KSCAMERAPROFILE_HIGHFRAMERATE: GUID = GUID::from_u128(0xc996873b_c319_4a28_ad21_225f3b786f87);

fn make_i64_propvariant(val: i64) -> PROPVARIANT {
    let mut pv = PROPVARIANT::default();
    unsafe {
        let p = &mut pv as *mut PROPVARIANT as *mut u8;
        *(p as *mut u16) = 20; // VT_I8
        *(p.add(8) as *mut i64) = val;
    }
    pv
}

fn set_attribute_size(attrs: &IMFAttributes, guid: &GUID, width: u32, height: u32) -> Result<()> {
    unsafe { attrs.SetUINT64(guid, ((width as u64) << 32) | (height as u32 as u64)) }
}

fn set_attribute_ratio(attrs: &IMFAttributes, guid: &GUID, num: u32, den: u32) -> Result<()> {
    unsafe { attrs.SetUINT64(guid, ((num as u64) << 32) | (den as u32 as u64)) }
}

fn create_video_media_type(width: u32, height: u32, fps: u32, subtype: &GUID) -> Result<IMFMediaType> {
    let mt = unsafe { MFCreateMediaType()? };
    let is_nv12 = *subtype == MFVideoFormat_NV12;
    let stride = if is_nv12 { width } else { width * 4 };
    let sample_size = if is_nv12 {
        (width * height * 3) / 2
    } else {
        width * height * 4
    };
    let bitrate = sample_size.saturating_mul(8).saturating_mul(fps);

    unsafe {
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        mt.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        mt.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        set_attribute_size(&mt, &MF_MT_FRAME_SIZE, width, height)?;
        set_attribute_ratio(&mt, &MF_MT_FRAME_RATE, fps, 1)?;
        set_attribute_ratio(&mt, &MF_MT_FRAME_RATE_RANGE_MAX, fps, 1)?;
        set_attribute_ratio(&mt, &MF_MT_FRAME_RATE_RANGE_MIN, fps, 1)?;
        set_attribute_ratio(&mt, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
        mt.SetUINT32(&MF_MT_DEFAULT_STRIDE, stride)?;
        mt.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)?;
        mt.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        mt.SetUINT32(&MF_MT_SAMPLE_SIZE, sample_size)?;
        mt.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
        mt.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
        mt.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
        mt.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
        mt.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_0_255.0 as u32)?;
    }
    Ok(mt)
}

struct SourceInner {
    event_queue: IMFMediaEventQueue,
    source_attributes: IMFAttributes,
    pres_descriptor: IMFPresentationDescriptor,
    stream: Option<SmartCamMfMediaStream>,
    stream_com: Option<IMFMediaStream>,
    stream_started_once: bool,
    is_shutdown: bool,
    /// Keeps the DLL loaded while this object is alive (see `DllCanUnloadNow`).
    _module: ModuleRef,
}

#[derive(Clone)]
#[implement(
    IMFMediaSourceEx,
    IMFActivate,
    IMFAttributes,
    IMFGetService,
    IMFSampleAllocatorControl,
    IKsControl
)]
pub struct SmartCamMfMediaSource {
    inner: Arc<Mutex<SourceInner>>,
}

unsafe impl Send for SourceInner {}
unsafe impl Sync for SourceInner {}

impl SmartCamMfMediaSource {
    pub fn new() -> Result<Self> {
        let event_queue = unsafe { MFCreateEventQueue()? };

        let mut source_attributes = None;
        unsafe {
            MFCreateAttributes(&mut source_attributes, 10)?;
        }
        let source_attributes =
            source_attributes.ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?;

        unsafe {
            let _ = source_attributes.SetUINT32(&MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES, 1);
            let _ = source_attributes.SetGUID(&MFT_TRANSFORM_CLSID_Attribute, &CLSID_SMARTCAM_MF_SOURCE);
            let _ = source_attributes.SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            );
            let _ = source_attributes.SetString(&MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, w!("SmartCam"));
            let _ = source_attributes.SetUINT32(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_MAX_BUFFERS, 3);

            // Mandatory Windows 11 FrameServer Legacy & HighFrameRate Sensor Profile Collection
            if let Ok(collection) = MFCreateSensorProfileCollection() {
                if let Ok(profile) = MFCreateSensorProfile(&KSCAMERAPROFILE_LEGACY, 0, None) {
                    let _ = profile.AddProfileFilter(0, w!("((RES==;FRT<=30,1;SUT==))"));
                    let _ = collection.AddProfile(&profile);
                }
                if let Ok(profile_hfr) = MFCreateSensorProfile(&KSCAMERAPROFILE_HIGHFRAMERATE, 0, None) {
                    let _ = profile_hfr.AddProfileFilter(0, w!("((RES==;FRT>=60,1;SUT==))"));
                    let _ = collection.AddProfile(&profile_hfr);
                }
                let _ = source_attributes
                    .SetUnknown(&MF_DEVICEMFT_SENSORPROFILE_COLLECTION, &collection);
            }
        }

        // Standard FrameServer & browser-compatible media types (RGB32 & NV12)
        let mt_rgb_1440 = create_video_media_type(
            DEFAULT_STREAM_WIDTH,
            DEFAULT_STREAM_HEIGHT,
            DEFAULT_STREAM_FPS,
            &MFVideoFormat_RGB32,
        )?;
        let mt_nv12_1440 = create_video_media_type(
            DEFAULT_STREAM_WIDTH,
            DEFAULT_STREAM_HEIGHT,
            DEFAULT_STREAM_FPS,
            &MFVideoFormat_NV12,
        )?;
        let mt_rgb_1080 = create_video_media_type(1920, 1080, DEFAULT_STREAM_FPS, &MFVideoFormat_RGB32)?;
        let mt_nv12_1080 = create_video_media_type(1920, 1080, DEFAULT_STREAM_FPS, &MFVideoFormat_NV12)?;
        let mt_rgb_720 = create_video_media_type(1280, 720, DEFAULT_STREAM_FPS, &MFVideoFormat_RGB32)?;
        let mt_nv12_720 = create_video_media_type(1280, 720, DEFAULT_STREAM_FPS, &MFVideoFormat_NV12)?;

        let stream_desc = unsafe {
            MFCreateStreamDescriptor(
                0,
                &[
                    Some(mt_rgb_1440.clone()),
                    Some(mt_nv12_1440),
                    Some(mt_rgb_1080),
                    Some(mt_nv12_1080),
                    Some(mt_rgb_720),
                    Some(mt_nv12_720),
                ],
            )?
        };

        unsafe {
            let _ = stream_desc.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE);
            let _ = stream_desc.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0);
            let _ = stream_desc.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1);
            let _ = stream_desc.SetUINT32(&MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, 1);
        }

        let handler = unsafe { stream_desc.GetMediaTypeHandler()? };
        unsafe {
            handler.SetCurrentMediaType(&mt_rgb_1440)?;
        }

        let pres_descriptor =
            unsafe { MFCreatePresentationDescriptor(Some(&[Some(stream_desc.clone())]))? };
        unsafe {
            pres_descriptor.SelectStream(0)?;
        }

        let stream = SmartCamMfMediaStream::new(None, stream_desc)?;
        let stream_com: IMFMediaStream = stream.clone().into();

        let inner = SourceInner {
            event_queue,
            source_attributes,
            pres_descriptor,
            stream: Some(stream.clone()),
            stream_com: Some(stream_com),
            stream_started_once: false,
            is_shutdown: false,
            _module: ModuleRef::new(),
        };

        let source_obj = Self {
            inner: Arc::new(Mutex::new(inner)),
        };

        Ok(source_obj)
    }

    pub fn create_com() -> Result<IMFMediaSourceEx> {
        let source_obj = Self::new()?;
        let stream_opt = source_obj.inner.lock().unwrap().stream.clone();
        let mf_source_ex: IMFMediaSourceEx = source_obj.into();
        if let Some(stream) = stream_opt {
            let mf_source: IMFMediaSource = mf_source_ex.clone().into();
            stream.set_parent_source(Some(mf_source));
        }
        Ok(mf_source_ex)
    }

    fn attrs(&self) -> IMFAttributes {
        self.inner.lock().unwrap().source_attributes.clone()
    }
}

impl IMFAttributes_Impl for SmartCamMfMediaSource_Impl {
    fn GetItem(&self, guidkey: *const GUID, pvalue: *mut PROPVARIANT) -> Result<()> {
        let k = unsafe { *guidkey };
        let res = unsafe { self.this.attrs().GetItem(guidkey, Some(pvalue)) };
        trace_mf(&format!("SourceAttr::GetItem {:?} -> {:?}", k, res));
        res
    }
    fn GetItemType(&self, guidkey: *const GUID) -> Result<MF_ATTRIBUTE_TYPE> {
        unsafe { self.this.attrs().GetItemType(guidkey) }
    }
    fn CompareItem(&self, guidkey: *const GUID, value: *const PROPVARIANT) -> Result<BOOL> {
        unsafe { self.this.attrs().CompareItem(guidkey, value) }
    }
    fn Compare(&self, ptheirs: Option<&IMFAttributes>, matchtype: MF_ATTRIBUTES_MATCH_TYPE) -> Result<BOOL> {
        unsafe { self.this.attrs().Compare(ptheirs, matchtype) }
    }
    fn GetUINT32(&self, guidkey: *const GUID) -> Result<u32> {
        let k = unsafe { *guidkey };
        let res = unsafe { self.this.attrs().GetUINT32(guidkey) };
        trace_mf(&format!("SourceAttr::GetUINT32 {:?} -> {:?}", k, res));
        res
    }
    fn GetUINT64(&self, guidkey: *const GUID) -> Result<u64> {
        let k = unsafe { *guidkey };
        let res = unsafe { self.this.attrs().GetUINT64(guidkey) };
        trace_mf(&format!("SourceAttr::GetUINT64 {:?} -> {:?}", k, res));
        res
    }
    fn GetDouble(&self, guidkey: *const GUID) -> Result<f64> {
        unsafe { self.this.attrs().GetDouble(guidkey) }
    }
    fn GetGUID(&self, guidkey: *const GUID) -> Result<GUID> {
        let k = unsafe { *guidkey };
        let res = unsafe { self.this.attrs().GetGUID(guidkey) };
        trace_mf(&format!("SourceAttr::GetGUID {:?} -> {:?}", k, res));
        res
    }
    fn GetStringLength(&self, guidkey: *const GUID) -> Result<u32> {
        unsafe { self.this.attrs().GetStringLength(guidkey) }
    }
    fn GetString(&self, guidkey: *const GUID, pwszvalue: PWSTR, cchbufsize: u32, pcchlength: *mut u32) -> Result<()> {
        let slice = unsafe { std::slice::from_raw_parts_mut(pwszvalue.0, cchbufsize as usize) };
        let len_opt = if pcchlength.is_null() { None } else { Some(pcchlength) };
        unsafe { self.this.attrs().GetString(guidkey, slice, len_opt) }
    }
    fn GetAllocatedString(&self, guidkey: *const GUID, ppwszvalue: *mut PWSTR, pcchlength: *mut u32) -> Result<()> {
        let k = unsafe { *guidkey };
        let res = unsafe { self.this.attrs().GetAllocatedString(guidkey, ppwszvalue, pcchlength) };
        trace_mf(&format!("SourceAttr::GetAllocatedString {:?} -> {:?}", k, res));
        res
    }
    fn GetBlobSize(&self, guidkey: *const GUID) -> Result<u32> {
        unsafe { self.this.attrs().GetBlobSize(guidkey) }
    }
    fn GetBlob(&self, guidkey: *const GUID, pbuf: *mut u8, cbbufsize: u32, pcbblobsize: *mut u32) -> Result<()> {
        let slice = unsafe { std::slice::from_raw_parts_mut(pbuf, cbbufsize as usize) };
        let len_opt = if pcbblobsize.is_null() { None } else { Some(pcbblobsize) };
        unsafe { self.this.attrs().GetBlob(guidkey, slice, len_opt) }
    }
    fn GetAllocatedBlob(&self, guidkey: *const GUID, ppbuf: *mut *mut u8, pcbsize: *mut u32) -> Result<()> {
        unsafe { self.this.attrs().GetAllocatedBlob(guidkey, ppbuf, pcbsize) }
    }
    fn GetUnknown(&self, guidkey: *const GUID, riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> Result<()> {
        let k = unsafe { *guidkey };
        unsafe {
            let unk: IUnknown = match self.this.attrs().GetUnknown(guidkey) {
                Ok(u) => u,
                Err(e) => {
                    trace_mf(&format!("SourceAttr::GetUnknown {:?} -> Err({:?})", k, e));
                    return Err(e);
                }
            };
            let hr = unk.query(riid, ppv);
            trace_mf(&format!("SourceAttr::GetUnknown {:?} -> {:?}", k, hr));
            hr.ok()
        }
    }
    fn SetItem(&self, guidkey: *const GUID, value: *const PROPVARIANT) -> Result<()> {
        unsafe { self.this.attrs().SetItem(guidkey, value) }
    }
    fn DeleteItem(&self, guidkey: *const GUID) -> Result<()> {
        unsafe { self.this.attrs().DeleteItem(guidkey) }
    }
    fn DeleteAllItems(&self, ) -> Result<()> {
        unsafe { self.this.attrs().DeleteAllItems() }
    }
    fn SetUINT32(&self, guidkey: *const GUID, unvalue: u32) -> Result<()> {
        unsafe { self.this.attrs().SetUINT32(guidkey, unvalue) }
    }
    fn SetUINT64(&self, guidkey: *const GUID, unvalue: u64) -> Result<()> {
        unsafe { self.this.attrs().SetUINT64(guidkey, unvalue) }
    }
    fn SetDouble(&self, guidkey: *const GUID, fvalue: f64) -> Result<()> {
        unsafe { self.this.attrs().SetDouble(guidkey, fvalue) }
    }
    fn SetGUID(&self, guidkey: *const GUID, guidvalue: *const GUID) -> Result<()> {
        unsafe { self.this.attrs().SetGUID(guidkey, guidvalue) }
    }
    fn SetString(&self, guidkey: *const GUID, wszvalue: &PCWSTR) -> Result<()> {
        unsafe { self.this.attrs().SetString(guidkey, *wszvalue) }
    }
    fn SetBlob(&self, guidkey: *const GUID, pbuf: *const u8, cbbufsize: u32) -> Result<()> {
        let slice = unsafe { std::slice::from_raw_parts(pbuf, cbbufsize as usize) };
        unsafe { self.this.attrs().SetBlob(guidkey, slice) }
    }
    fn SetUnknown(&self, guidkey: *const GUID, punknown: Option<&IUnknown>) -> Result<()> {
        unsafe { self.this.attrs().SetUnknown(guidkey, punknown) }
    }
    fn LockStore(&self) -> Result<()> {
        unsafe { self.this.attrs().LockStore() }
    }
    fn UnlockStore(&self) -> Result<()> {
        unsafe { self.this.attrs().UnlockStore() }
    }
    fn GetCount(&self) -> Result<u32> {
        unsafe { self.this.attrs().GetCount() }
    }
    fn GetItemByIndex(&self, unindex: u32, pguidkey: *mut GUID, pvalue: *mut PROPVARIANT) -> Result<()> {
        unsafe { self.this.attrs().GetItemByIndex(unindex, pguidkey, Some(pvalue)) }
    }
    fn CopyAllItems(&self, pdest: Option<&IMFAttributes>) -> Result<()> {
        unsafe { self.this.attrs().CopyAllItems(pdest) }
    }
}

impl IMFActivate_Impl for SmartCamMfMediaSource_Impl {
    fn ActivateObject(&self, riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> Result<()> {
        if ppv.is_null() || riid.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }
        unsafe {
            let requested = *riid;
            let hr = self
                .to_object()
                .as_interface::<IUnknown>()
                .query(riid, ppv);
            trace_mf(&format!("ActivateObject riid={:?} -> hr={:?}", requested, hr));
            hr.ok()
        }
    }

    fn ShutdownObject(&self) -> Result<()> {
        trace_mf("ShutdownObject");
        Ok(())
    }

    fn DetachObject(&self) -> Result<()> {
        trace_mf("DetachObject");
        Ok(())
    }
}

impl IMFMediaEventGenerator_Impl for SmartCamMfMediaSource_Impl {
    fn GetEvent(&self, dwflags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> Result<IMFMediaEvent> {
        let queue = {
            let inner = self.inner.lock().unwrap();
            if inner.is_shutdown {
                return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
            }
            inner.event_queue.clone()
        };
        unsafe { queue.GetEvent(dwflags.0) }
    }

    fn BeginGetEvent(
        &self,
        pcallback: Option<&IMFAsyncCallback>,
        punkstate: Option<&IUnknown>,
    ) -> Result<()> {
        let queue = {
            let inner = self.inner.lock().unwrap();
            if inner.is_shutdown {
                return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
            }
            inner.event_queue.clone()
        };
        unsafe { queue.BeginGetEvent(pcallback, punkstate) }
    }

    fn EndGetEvent(&self, presult: Option<&IMFAsyncResult>) -> Result<IMFMediaEvent> {
        let queue = {
            let inner = self.inner.lock().unwrap();
            if inner.is_shutdown {
                return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
            }
            inner.event_queue.clone()
        };
        unsafe { queue.EndGetEvent(presult) }
    }

    fn QueueEvent(
        &self,
        met: u32,
        guidextendedtype: *const GUID,
        hrstatus: HRESULT,
        pvvalue: *const PROPVARIANT,
    ) -> Result<()> {
        let queue = {
            let inner = self.inner.lock().unwrap();
            if inner.is_shutdown {
                return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
            }
            inner.event_queue.clone()
        };
        unsafe { queue.QueueEventParamVar(met, guidextendedtype, hrstatus, pvvalue) }
    }
}

impl IMFMediaSource_Impl for SmartCamMfMediaSource_Impl {
    fn GetCharacteristics(&self) -> Result<u32> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        } else {
            Ok(MFMEDIASOURCE_IS_LIVE.0 as u32)
        }
    }

    fn CreatePresentationDescriptor(&self) -> Result<IMFPresentationDescriptor> {
        trace_mf("CreatePresentationDescriptor");
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        } else {
            unsafe { inner.pres_descriptor.Clone() }
        }
    }

    fn Start(
        &self,
        ppresentationdescriptor: Option<&IMFPresentationDescriptor>,
        _pguidtimeformat: *const GUID,
        _pvarstartposition: *const PROPVARIANT,
    ) -> Result<()> {
        trace_mf("IMFMediaSource::Start");
        let pd = ppresentationdescriptor
            .ok_or_else(|| windows::core::Error::from_hresult(E_INVALIDARG))?;

        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }

        // Extract negotiated media type from caller's presentation descriptor
        let mut negotiated_mt: Option<IMFMediaType> = None;
        unsafe {
            let mut selected = BOOL::default();
            let mut caller_sd = None;
            if pd
                .GetStreamDescriptorByIndex(0, &mut selected, &mut caller_sd)
                .is_ok()
            {
                if let Some(sd) = caller_sd {
                    if let Ok(handler) = sd.GetMediaTypeHandler() {
                        negotiated_mt = handler.GetCurrentMediaType().ok();
                    }
                }
            }
            let _ = inner.pres_descriptor.SelectStream(0);
        }

        let time_pv = make_i64_propvariant(unsafe { MFGetSystemTime() });

        let event_type = if inner.stream_started_once {
            MEUpdatedStream.0 as u32
        } else {
            inner.stream_started_once = true;
            MENewStream.0 as u32
        };

        if let (Some(ref stream), Some(ref stream_com)) = (&inner.stream, &inner.stream_com) {
            let stream_punk: IUnknown = stream_com.clone().into();
            unsafe {
                inner.event_queue.QueueEventParamUnk(
                    event_type,
                    &GUID::zeroed(),
                    S_OK,
                    &stream_punk,
                )?;
            }
            stream.start_stream(negotiated_mt.as_ref(), &time_pv)?;
        }

        let res = unsafe {
            inner.event_queue.QueueEventParamVar(
                MESourceStarted.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &time_pv,
            )
        };
        trace_mf(&format!("IMFMediaSource::Start done -> {:?}", res));
        res
    }

    fn Stop(&self) -> Result<()> {
        trace_mf("IMFMediaSource::Stop");
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        let time_pv = make_i64_propvariant(unsafe { MFGetSystemTime() });
        if let Some(ref stream) = inner.stream {
            stream.stop_stream()?;
        }
        unsafe {
            let _ = inner.pres_descriptor.DeselectStream(0);
        }
        inner.stream_started_once = false;
        unsafe {
            inner.event_queue.QueueEventParamVar(
                MESourceStopped.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &time_pv,
            )
        }
    }

    fn Pause(&self) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref stream) = inner.stream {
            stream.pause_stream()?;
        }
        unsafe {
            inner.event_queue.QueueEventParamVar(
                MESourcePaused.0 as u32,
                &GUID::zeroed(),
                S_OK,
                std::ptr::null(),
            )
        }
    }

    fn Shutdown(&self) -> Result<()> {
        trace_mf("IMFMediaSource::Shutdown");
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Ok(());
        }
        inner.is_shutdown = true;
        inner.stream_com = None;
        if let Some(stream) = inner.stream.take() {
            let _ = stream.shutdown_stream();
        }
        unsafe {
            let _ = inner.event_queue.Shutdown();
        }
        Ok(())
    }
}

impl IMFMediaSourceEx_Impl for SmartCamMfMediaSource_Impl {
    fn GetSourceAttributes(&self) -> Result<IMFAttributes> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        } else {
            drop(inner);
            self.to_object().cast::<IMFAttributes>()
        }
    }

    fn GetStreamAttributes(&self, dwstreamidentifier: u32) -> Result<IMFAttributes> {
        if dwstreamidentifier != 0 {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref stream_com) = inner.stream_com {
            stream_com.cast::<IMFAttributes>()
        } else {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        }
    }

    fn SetD3DManager(&self, pmanager: Option<&IUnknown>) -> Result<()> {
        trace_mf(&format!("Source::SetD3DManager has_mgr={}", pmanager.is_some()));
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref stream) = inner.stream {
            stream.set_d3d_manager(pmanager)?;
        }
        Ok(())
    }
}

impl IMFSampleAllocatorControl_Impl for SmartCamMfMediaSource_Impl {
    fn SetDefaultAllocator(
        &self,
        dwoutputstreamid: u32,
        pallocator: Option<&IUnknown>,
    ) -> Result<()> {
        trace_mf(&format!(
            "Source::SetDefaultAllocator stream={} has_alloc={}",
            dwoutputstreamid,
            pallocator.is_some()
        ));
        if dwoutputstreamid != 0 {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref stream) = inner.stream {
            stream.set_allocator(pallocator)?;
        }
        Ok(())
    }

    fn GetAllocatorUsage(
        &self,
        dwoutputstreamid: u32,
        pdwinputstreamid: *mut u32,
        peusage: *mut MFSampleAllocatorUsage,
    ) -> Result<()> {
        trace_mf(&format!("Source::GetAllocatorUsage stream={}", dwoutputstreamid));
        if dwoutputstreamid != 0 {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }
        if pdwinputstreamid.is_null() || peusage.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }
        unsafe {
            *pdwinputstreamid = dwoutputstreamid;
            *peusage = MFSampleAllocatorUsage_UsesProvidedAllocator;
        }
        Ok(())
    }
}

impl IMFGetService_Impl for SmartCamMfMediaSource_Impl {
    fn GetService(
        &self,
        guidservice: *const GUID,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        if ppvobject.is_null() || riid.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }
        unsafe {
            *ppvobject = std::ptr::null_mut();
            let svc = if guidservice.is_null() {
                GUID::zeroed()
            } else {
                *guidservice
            };
            let req = *riid;
            let hr = self
                .to_object()
                .as_interface::<IUnknown>()
                .query(riid, ppvobject);
            if hr.is_ok() {
                return Ok(());
            }
            trace_mf(&format!(
                "Source::GetService svc={:?} riid={:?} -> MF_E_UNSUPPORTED_SERVICE",
                svc, req
            ));
            Err(windows::core::Error::from_hresult(MF_E_UNSUPPORTED_SERVICE))
        }
    }
}

impl IKsControl_Impl for SmartCamMfMediaSource_Impl {
    unsafe fn KsProperty(
        &self,
        property: *const core::ffi::c_void,
        _property_length: u32,
        _property_data: *mut core::ffi::c_void,
        _data_length: u32,
        bytes_returned: *mut u32,
    ) -> windows_core::HRESULT {
        if property.is_null() || bytes_returned.is_null() {
            return E_POINTER;
        }
        trace_mf("Source::KsProperty -> ERROR_SET_NOT_FOUND");
        HR_ERROR_SET_NOT_FOUND
    }

    unsafe fn KsMethod(
        &self,
        method: *const core::ffi::c_void,
        _method_length: u32,
        _method_data: *mut core::ffi::c_void,
        _data_length: u32,
        bytes_returned: *mut u32,
    ) -> windows_core::HRESULT {
        if method.is_null() || bytes_returned.is_null() {
            return E_POINTER;
        }
        trace_mf("Source::KsMethod -> ERROR_SET_NOT_FOUND");
        HR_ERROR_SET_NOT_FOUND
    }

    unsafe fn KsEvent(
        &self,
        _event: *const core::ffi::c_void,
        _event_length: u32,
        _event_data: *mut core::ffi::c_void,
        _data_length: u32,
        bytes_returned: *mut u32,
    ) -> windows_core::HRESULT {
        if bytes_returned.is_null() {
            return E_POINTER;
        }
        trace_mf("Source::KsEvent -> ERROR_SET_NOT_FOUND");
        HR_ERROR_SET_NOT_FOUND
    }
}
