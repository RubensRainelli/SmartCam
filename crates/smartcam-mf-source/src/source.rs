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
    core::{implement, IUnknown, Interface, Result, GUID, HRESULT},
    Win32::{
        Foundation::{E_INVALIDARG, E_POINTER, S_OK},
        Media::MediaFoundation::{
            IMFAsyncCallback, IMFAsyncResult, IMFAttributes, IMFGetService_Impl,
            IMFMediaEvent, IMFMediaEventGenerator_Impl, IMFMediaEventQueue,
            IMFMediaSourceEx, IMFMediaSourceEx_Impl,
            IMFMediaSource_Impl, IMFMediaStream, IMFPresentationDescriptor,
            MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS, MENewStream, MESourcePaused,
            MESourceStarted, MESourceStopped, MFCreateAttributes, MFCreateEventQueue,
            MFCreateMediaType, MFCreatePresentationDescriptor,
            MFCreateStreamDescriptor, MFMEDIASOURCE_IS_LIVE, MFMediaType_Video,
            MFVideoFormat_RGB32, MFVideoInterlace_Progressive, MF_E_SHUTDOWN,
            MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_DEFAULT_STRIDE,
            MF_MT_FIXED_SIZE_SAMPLES, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
            MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO,
            MF_MT_SAMPLE_SIZE, MF_MT_SUBTYPE,
        },
    },
};
use windows_core::{IUnknownImpl, PROPVARIANT};

use crate::{
    constants::{
        DEFAULT_STREAM_FPS, DEFAULT_STREAM_HEIGHT, DEFAULT_STREAM_WIDTH,
        MF_VIDEO_FORMAT_ARGB64,
    },
    stream::SmartCamMfMediaStream,
};

fn set_attribute_size(attrs: &IMFAttributes, guid: &GUID, width: u32, height: u32) -> Result<()> {
    unsafe { attrs.SetUINT64(guid, ((width as u64) << 32) | (height as u64)) }
}

fn set_attribute_ratio(attrs: &IMFAttributes, guid: &GUID, num: u32, den: u32) -> Result<()> {
    unsafe { attrs.SetUINT64(guid, ((num as u64) << 32) | (den as u64)) }
}

struct SourceInner {
    event_queue: IMFMediaEventQueue,
    source_attributes: IMFAttributes,
    pres_descriptor: IMFPresentationDescriptor,
    stream: Option<SmartCamMfMediaStream>,
    is_shutdown: bool,
}

#[derive(Clone)]
#[implement(IMFMediaSourceEx, windows::Win32::Media::MediaFoundation::IMFGetService)]
pub struct SmartCamMfMediaSource {
    inner: Arc<Mutex<SourceInner>>,
}

impl SmartCamMfMediaSource {
    pub fn new() -> Result<Self> {
        let event_queue = unsafe { MFCreateEventQueue()? };

        let mut source_attributes = None;
        unsafe {
            MFCreateAttributes(&mut source_attributes, 4)?;
        }
        let source_attributes = source_attributes.ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?;

        // 1. MediaType 0: 1440x1080 @ 30fps RGB32/BGRA32
        let media_type = unsafe { MFCreateMediaType()? };
        unsafe {
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            set_attribute_size(&media_type, &MF_MT_FRAME_SIZE, DEFAULT_STREAM_WIDTH, DEFAULT_STREAM_HEIGHT)?;
            set_attribute_ratio(&media_type, &MF_MT_FRAME_RATE, DEFAULT_STREAM_FPS, 1)?;
            set_attribute_ratio(&media_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
            media_type.SetUINT32(&MF_MT_DEFAULT_STRIDE, DEFAULT_STREAM_WIDTH * 4)?;
            media_type.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)?;
            media_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
            media_type.SetUINT32(&MF_MT_SAMPLE_SIZE, DEFAULT_STREAM_WIDTH * DEFAULT_STREAM_HEIGHT * 4)?;
        }

        // 2. MediaType 1: 1440x1080 @ 30fps ARGB64 (Deep Color 16-bit per channel)
        let media_type_64 = unsafe { MFCreateMediaType()? };
        unsafe {
            media_type_64.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type_64.SetGUID(&MF_MT_SUBTYPE, &MF_VIDEO_FORMAT_ARGB64)?;
            media_type_64.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            set_attribute_size(&media_type_64, &MF_MT_FRAME_SIZE, DEFAULT_STREAM_WIDTH, DEFAULT_STREAM_HEIGHT)?;
            set_attribute_ratio(&media_type_64, &MF_MT_FRAME_RATE, DEFAULT_STREAM_FPS, 1)?;
            set_attribute_ratio(&media_type_64, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
            media_type_64.SetUINT32(&MF_MT_DEFAULT_STRIDE, DEFAULT_STREAM_WIDTH * 8)?;
            media_type_64.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)?;
            media_type_64.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
            media_type_64.SetUINT32(&MF_MT_SAMPLE_SIZE, DEFAULT_STREAM_WIDTH * DEFAULT_STREAM_HEIGHT * 8)?;
        }

        let stream_desc = unsafe {
            MFCreateStreamDescriptor(
                0,
                &[Some(media_type.clone()), Some(media_type_64)],
            )?
        };

        let handler = unsafe { stream_desc.GetMediaTypeHandler()? };
        unsafe {
            handler.SetCurrentMediaType(&media_type)?;
        }

        let pres_descriptor = unsafe {
            MFCreatePresentationDescriptor(Some(&[Some(stream_desc.clone())]))?
        };
        unsafe {
            pres_descriptor.SelectStream(0)?;
        }

        let stream = SmartCamMfMediaStream::new(None, stream_desc)?;

        let inner = SourceInner {
            event_queue,
            source_attributes,
            pres_descriptor,
            stream: Some(stream),
            is_shutdown: false,
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
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
        unsafe { queue.GetEvent(dwflags.0 as u32) }
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
        pvarstartposition: *const PROPVARIANT,
    ) -> Result<()> {
        if ppresentationdescriptor.is_none() {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }

        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }

        if let Some(ref stream) = inner.stream {
            let stream_com: IMFMediaStream = stream.clone().into();
            let stream_punk: IUnknown = stream_com.into();
            unsafe {
                inner.event_queue.QueueEventParamUnk(
                    MENewStream.0 as u32,
                    &GUID::zeroed(),
                    S_OK,
                    &stream_punk,
                )?;
            }
            stream.start_stream(pvarstartposition)?;
        }

        unsafe {
            inner.event_queue.QueueEventParamVar(
                MESourceStarted.0 as u32,
                &GUID::zeroed(),
                S_OK,
                pvarstartposition,
            )
        }
    }

    fn Stop(&self) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref stream) = inner.stream {
            stream.stop_stream()?;
        }
        unsafe {
            inner.event_queue.QueueEventParamVar(
                MESourceStopped.0 as u32,
                &GUID::zeroed(),
                S_OK,
                std::ptr::null(),
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
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Ok(());
        }
        inner.is_shutdown = true;
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
            Ok(inner.source_attributes.clone())
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
        if let Some(ref stream) = inner.stream {
            stream.attributes()
        } else {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        }
    }

    fn SetD3DManager(&self, _pmanager: Option<&IUnknown>) -> Result<()> {
        Ok(())
    }
}

impl IMFGetService_Impl for SmartCamMfMediaSource_Impl {
    fn GetService(
        &self,
        _guidservice: *const GUID,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        if ppvobject.is_null() || riid.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }
        unsafe {
            self.to_object().as_interface::<IUnknown>().query(riid, ppvobject).ok()
        }
    }
}
