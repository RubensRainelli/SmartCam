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

use std::sync::{
    atomic::Ordering,
    Arc, Mutex,
};
use windows::{
    core::{implement, IUnknown, Interface, Result, GUID, HRESULT},
    Win32::{
        Foundation::{BOOL, E_POINTER, S_OK},
        Media::MediaFoundation::{
            IMF2DBuffer2, IMFAsyncCallback, IMFAsyncResult, IMFAttributes, IMFAttributes_Impl,
            IMFMediaBuffer, IMFMediaEvent, IMFMediaEventGenerator_Impl, IMFMediaEventQueue,
            IMFMediaSource, IMFMediaStream, IMFMediaStream2, IMFMediaStream2_Impl,
            IMFMediaStream_Impl, IMFMediaType, IMFSample, IMFStreamDescriptor,
            IMFVideoSampleAllocator, IMFVideoSampleAllocatorEx, MEMediaSample, MEStreamPaused,
            MEStreamStarted, MEStreamStopped, MF2DBuffer_LockFlags_Write, MFCreateAttributes,
            MFCreateEventQueue, MFCreateMemoryBuffer, MFCreateSample, MFGetSystemTime,
            MFSampleExtension_CleanPoint, MFSampleExtension_Token, MFVideoFormat_NV12,
            MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS, MF_ATTRIBUTES_MATCH_TYPE, MF_ATTRIBUTE_TYPE,
            MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MF_DEVICESTREAM_FRAMESERVER_SHARED,
            MF_DEVICESTREAM_STREAM_CATEGORY, MF_DEVICESTREAM_STREAM_ID,
            MF_E_INVALID_STATE_TRANSITION, MF_E_SHUTDOWN, MF_MT_FRAME_SIZE, MF_MT_SUBTYPE,
            MF_STREAM_STATE, MF_STREAM_STATE_PAUSED, MF_STREAM_STATE_RUNNING,
            MF_STREAM_STATE_STOPPED, MFNominalRange, MFNominalRange_16_235, MFVideoTransferMatrix,
            MFVideoTransferMatrix_BT601,
        },
    },
};
use windows_core::{PCWSTR, PROPVARIANT, PWSTR};

/// Colour encoding written by `SmartCamMfMediaStream::bgra_to_nv12_pitched`: BT.601 matrix and
/// studio range (Y 16..235, UV 16..240). NV12 media types must advertise exactly this, or
/// consumers decode the frames with the wrong matrix and range.
pub(crate) const NV12_YUV_MATRIX: MFVideoTransferMatrix = MFVideoTransferMatrix_BT601;
pub(crate) const NV12_NOMINAL_RANGE: MFNominalRange = MFNominalRange_16_235;

use smartcam_capture::{
    SharedFrameIpc, DEFAULT_SHARED_EVENT_NAME, DEFAULT_SHARED_MAPPING_NAME, MAX_SHARED_FRAME_BYTES,
    SHARED_FRAME_MAGIC,
};

use crate::constants::{
    trace_mf, IKsControl, IKsControl_Impl, DEFAULT_SAMPLE_DURATION_100NS, DEFAULT_STREAM_HEIGHT,
    DEFAULT_STREAM_WIDTH, HR_ERROR_SET_NOT_FOUND, PINNAME_VIDEO_CAPTURE,
};

struct StreamInner {
    parent_source: Option<IMFMediaSource>,
    stream_descriptor: IMFStreamDescriptor,
    stream_attributes: IMFAttributes,
    event_queue: IMFMediaEventQueue,
    allocator: Option<IMFVideoSampleAllocator>,
    ipc: Option<SharedFrameIpc>,
    scratch_buf: Vec<u8>,
    bgra_frame_buf: Vec<u8>,
    has_valid_frame: bool,
    last_valid_width: u32,
    last_valid_height: u32,
    last_ipc_seq: i64,
    last_frame_index: u64,
    nv12_cache_buf: Vec<u8>,
    nv12_cache_seq: i64,
    nv12_cache_frame_idx: u64,
    nv12_cache_w: u32,
    nv12_cache_h: u32,
    standby_rendered_dim: (u32, u32),
    sample_count: u64,
    width: u32,
    height: u32,
    is_nv12: bool,
    state: MF_STREAM_STATE,
    is_shutdown: bool,
}

#[derive(Clone)]
#[implement(IMFMediaStream2, IMFMediaStream, IMFAttributes, IKsControl)]
pub struct SmartCamMfMediaStream {
    inner: Arc<Mutex<StreamInner>>,
}

unsafe impl Send for StreamInner {}
unsafe impl Sync for StreamInner {}

impl SmartCamMfMediaStream {
    pub fn new(
        parent_source: Option<IMFMediaSource>,
        stream_descriptor: IMFStreamDescriptor,
    ) -> Result<Self> {
        let event_queue = unsafe { MFCreateEventQueue()? };

        let mut stream_attributes = None;
        unsafe {
            MFCreateAttributes(&mut stream_attributes, 6)?;
        }
        let stream_attributes =
            stream_attributes.ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?;

        unsafe {
            stream_attributes.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
            stream_attributes.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0)?;
            stream_attributes.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
            stream_attributes.SetUINT32(&MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, 1)?;
        }

        let ipc = SharedFrameIpc::open_or_create(
            DEFAULT_SHARED_MAPPING_NAME,
            DEFAULT_SHARED_EVENT_NAME,
            false,
        );

        let inner = StreamInner {
            parent_source,
            stream_descriptor,
            stream_attributes,
            event_queue,
            allocator: None,
            ipc,
            scratch_buf: Vec::new(),
            bgra_frame_buf: Vec::new(),
            has_valid_frame: false,
            last_valid_width: 0,
            last_valid_height: 0,
            last_ipc_seq: 0,
            last_frame_index: 0,
            nv12_cache_buf: Vec::new(),
            nv12_cache_seq: -1,
            nv12_cache_frame_idx: u64::MAX,
            nv12_cache_w: 0,
            nv12_cache_h: 0,
            standby_rendered_dim: (0, 0),
            sample_count: 0,
            width: DEFAULT_STREAM_WIDTH,
            height: DEFAULT_STREAM_HEIGHT,
            is_nv12: false,
            state: MF_STREAM_STATE_STOPPED,
            is_shutdown: false,
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub fn set_parent_source(&self, parent: Option<IMFMediaSource>) {
        let mut inner = self.inner.lock().unwrap();
        inner.parent_source = parent;
    }

    pub fn set_allocator(&self, allocator_unk: Option<&IUnknown>) -> Result<()> {
        let unk = allocator_unk.ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?;
        let alloc: IMFVideoSampleAllocator = unk.cast()?;
        trace_mf("Stream::set_allocator succeeded");
        let mut inner = self.inner.lock().unwrap();
        inner.allocator = Some(alloc);
        Ok(())
    }

    pub fn set_d3d_manager(&self, manager: Option<&IUnknown>) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        if let Some(ref alloc) = inner.allocator {
            unsafe {
                let _ = alloc.SetDirectXManager(manager);
            }
        }
        Ok(())
    }

    pub fn get_stream_state(&self) -> MF_STREAM_STATE {
        self.inner.lock().unwrap().state
    }

    fn attrs(&self) -> IMFAttributes {
        self.inner.lock().unwrap().stream_attributes.clone()
    }

    pub fn start_stream(
        &self,
        media_type: Option<&IMFMediaType>,
        start_pos: *const PROPVARIANT,
    ) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }

        let active_mt = if let Some(mt) = media_type {
            Some(mt.clone())
        } else {
            unsafe {
                inner
                    .stream_descriptor
                    .GetMediaTypeHandler()
                    .and_then(|h| h.GetCurrentMediaType())
                    .ok()
            }
        };

        if let Some(ref mt) = active_mt {
            unsafe {
                if let Ok(size_val) = mt.GetUINT64(&MF_MT_FRAME_SIZE) {
                    let w = (size_val >> 32) as u32;
                    let h = (size_val & 0xFFFF_FFFF) as u32;
                    if w > 0 && h > 0 {
                        inner.width = w;
                        inner.height = h;
                    }
                }
                if let Ok(sub) = mt.GetGUID(&MF_MT_SUBTYPE) {
                    inner.is_nv12 = sub == MFVideoFormat_NV12;
                }
                if let Ok(handler) = inner.stream_descriptor.GetMediaTypeHandler() {
                    let _ = handler.SetCurrentMediaType(mt);
                }
            }
        }

        if let (Some(ref alloc), Some(ref mt)) = (&inner.allocator, &active_mt) {
            unsafe {
                let _ = alloc.UninitializeSampleAllocator();
                if let Ok(alloc_ex) = alloc.cast::<IMFVideoSampleAllocatorEx>() {
                    let hr = alloc_ex.InitializeSampleAllocatorEx(4, 10, None, mt);
                    trace_mf(&format!("Stream::start_stream InitializeSampleAllocatorEx -> {:?}", hr));
                    if hr.is_err() {
                        let hr2 = alloc.InitializeSampleAllocator(10, mt);
                        trace_mf(&format!("Stream::start_stream InitializeSampleAllocator fallback -> {:?}", hr2));
                    }
                } else {
                    let hr = alloc.InitializeSampleAllocator(10, mt);
                    trace_mf(&format!("Stream::start_stream InitializeSampleAllocator -> {:?}", hr));
                }
            }
        }

        if let Some(ref ipc) = inner.ipc {
            ipc.touch_consumer_heartbeat();
        }

        inner.state = MF_STREAM_STATE_RUNNING;
        unsafe {
            inner.event_queue.QueueEventParamVar(
                MEStreamStarted.0 as u32,
                &GUID::zeroed(),
                S_OK,
                start_pos,
            )
        }
    }

    pub fn stop_stream(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref alloc) = inner.allocator {
            unsafe {
                let _ = alloc.UninitializeSampleAllocator();
            }
        }
        inner.state = MF_STREAM_STATE_STOPPED;
        unsafe {
            inner.event_queue.QueueEventParamVar(
                MEStreamStopped.0 as u32,
                &GUID::zeroed(),
                S_OK,
                std::ptr::null(),
            )
        }
    }

    pub fn pause_stream(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        inner.state = MF_STREAM_STATE_PAUSED;
        unsafe {
            inner.event_queue.QueueEventParamVar(
                MEStreamPaused.0 as u32,
                &GUID::zeroed(),
                S_OK,
                std::ptr::null(),
            )
        }
    }

    pub fn shutdown_stream(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Ok(());
        }
        inner.is_shutdown = true;
        inner.state = MF_STREAM_STATE_STOPPED;
        if let Some(alloc) = inner.allocator.take() {
            unsafe {
                let _ = alloc.UninitializeSampleAllocator();
            }
        }
        inner.parent_source = None;
        if let Some(mut ipc) = inner.ipc.take() {
            ipc.close();
        }
        unsafe {
            let _ = inner.event_queue.Shutdown();
        }
        Ok(())
    }

    pub fn attributes(&self) -> Result<IMFAttributes> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        } else {
            Ok(inner.stream_attributes.clone())
        }
    }

    pub fn render_standby_pattern(dst: *mut u8, width: u32, height: u32, _sample_count: u64) {
        let count = (width as usize) * (height as usize);
        if dst.is_null() || count == 0 {
            return;
        }
        let pixel = u32::from_le_bytes([38, 24, 18, 255]); // Dark Studio Slate BGRA
        unsafe {
            if (dst as usize) % align_of::<u32>() == 0 {
                let slice_u32 = std::slice::from_raw_parts_mut(dst as *mut u32, count);
                slice_u32.fill(pixel);
            } else {
                for i in 0..count {
                    let px = dst.add(i * 4);
                    *px = 38;
                    *px.add(1) = 24;
                    *px.add(2) = 18;
                    *px.add(3) = 255;
                }
            }
        }
    }

    fn bgra_to_nv12_pitched(
        src_bgra: &[u8],
        dst_nv12: *mut u8,
        dst_pitch: usize,
        width: u32,
        height: u32,
    ) {
        let w = width as usize;
        let h = height as usize;
        let y_plane = dst_nv12;
        let uv_plane = unsafe { dst_nv12.add(dst_pitch * h) };

        unsafe {
            for y in 0..h {
                let src_row = src_bgra.as_ptr().add(y * w * 4);
                let y_row = y_plane.add(y * dst_pitch);
                for x in 0..w {
                    let px = src_row.add(x * 4);
                    let b = *px as i32;
                    let g = *px.add(1) as i32;
                    let r = *px.add(2) as i32;
                    let luma = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
                    *y_row.add(x) = luma.clamp(16, 235) as u8;
                }
            }

            for y in (0..h).step_by(2) {
                let src_row = src_bgra.as_ptr().add(y * w * 4);
                let uv_row = uv_plane.add((y / 2) * dst_pitch);
                for x in (0..w).step_by(2) {
                    let px = src_row.add(x * 4);
                    let b = *px as i32;
                    let g = *px.add(1) as i32;
                    let r = *px.add(2) as i32;
                    let u = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
                    let v = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
                    *uv_row.add(x) = u.clamp(16, 240) as u8;
                    *uv_row.add(x + 1) = v.clamp(16, 240) as u8;
                }
            }
        }
    }
}

impl IMFAttributes_Impl for SmartCamMfMediaStream_Impl {
    fn GetItem(&self, guidkey: *const GUID, pvalue: *mut PROPVARIANT) -> Result<()> {
        unsafe { self.this.attrs().GetItem(guidkey, Some(pvalue)) }
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
        unsafe { self.this.attrs().GetUINT32(guidkey) }
    }
    fn GetUINT64(&self, guidkey: *const GUID) -> Result<u64> {
        unsafe { self.this.attrs().GetUINT64(guidkey) }
    }
    fn GetDouble(&self, guidkey: *const GUID) -> Result<f64> {
        unsafe { self.this.attrs().GetDouble(guidkey) }
    }
    fn GetGUID(&self, guidkey: *const GUID) -> Result<GUID> {
        unsafe { self.this.attrs().GetGUID(guidkey) }
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
        unsafe { self.this.attrs().GetAllocatedString(guidkey, ppwszvalue, pcchlength) }
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
        let unk: IUnknown = unsafe { self.this.attrs().GetUnknown(guidkey)? };
        unsafe { unk.query(riid, ppv).ok() }
    }
    fn SetItem(&self, guidkey: *const GUID, value: *const PROPVARIANT) -> Result<()> {
        unsafe { self.this.attrs().SetItem(guidkey, value) }
    }
    fn DeleteItem(&self, guidkey: *const GUID) -> Result<()> {
        unsafe { self.this.attrs().DeleteItem(guidkey) }
    }
    fn DeleteAllItems(&self) -> Result<()> {
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
    fn SetUnknown(&self, guidkey: *const GUID, punk: Option<&IUnknown>) -> Result<()> {
        unsafe { self.this.attrs().SetUnknown(guidkey, punk) }
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

impl IMFMediaEventGenerator_Impl for SmartCamMfMediaStream_Impl {
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

impl IMFMediaStream_Impl for SmartCamMfMediaStream_Impl {
    fn GetMediaSource(&self) -> Result<IMFMediaSource> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        inner
            .parent_source
            .clone()
            .ok_or_else(|| windows::core::Error::from_hresult(MF_E_SHUTDOWN))
    }

    fn GetStreamDescriptor(&self) -> Result<IMFStreamDescriptor> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN))
        } else {
            Ok(inner.stream_descriptor.clone())
        }
    }

    fn RequestSample(&self, ptoken: Option<&IUnknown>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }

        // Check negotiated media type dimensions and subtype (RGB32 vs NV12)
        let mut width = inner.width;
        let mut height = inner.height;
        let mut is_nv12 = inner.is_nv12;

        unsafe {
            if let Ok(handler) = inner.stream_descriptor.GetMediaTypeHandler() {
                if let Ok(mt) = handler.GetCurrentMediaType() {
                    if let Ok(size_val) = mt.GetUINT64(&MF_MT_FRAME_SIZE) {
                        let w = (size_val >> 32) as u32;
                        let h = (size_val & 0xFFFF_FFFF) as u32;
                        if w > 0 && h > 0 {
                            width = w;
                            height = h;
                            inner.width = w;
                            inner.height = h;
                        }
                    }
                    if let Ok(sub) = mt.GetGUID(&MF_MT_SUBTYPE) {
                        is_nv12 = sub == MFVideoFormat_NV12;
                        inner.is_nv12 = is_nv12;
                    }
                }
            }
        }

        let bgra_bytes = (width as usize) * (height as usize) * 4;
        if inner.bgra_frame_buf.len() < bgra_bytes {
            inner.bgra_frame_buf.resize(bgra_bytes, 0);
        }

        // Ensure IPC mapping is active
        if inner.ipc.is_none() {
            inner.ipc = SharedFrameIpc::open_or_create(
                DEFAULT_SHARED_MAPPING_NAME,
                DEFAULT_SHARED_EVENT_NAME,
                false,
            );
        }

        let mut got_ipc_frame = false;
        {
            let StreamInner {
                ref ipc,
                ref mut scratch_buf,
                ref mut bgra_frame_buf,
                ref mut has_valid_frame,
                ref mut last_valid_width,
                ref mut last_valid_height,
                ref mut last_ipc_seq,
                ref mut last_frame_index,
                ref mut standby_rendered_dim,
                ..
            } = *inner;

            if let Some(ref ipc_ref) = *ipc {
                if let Some(hdr) = ipc_ref.header() {
                    if hdr.magic == SHARED_FRAME_MAGIC && hdr.width > 0 && hdr.height > 0 {
                        let cur_seq = hdr.sequence_counter.load(Ordering::Acquire);
                        let cur_fidx = hdr.frame_index;
                        if cur_seq > 0
                            && (cur_seq & 1) == 0
                            && cur_seq == *last_ipc_seq
                            && cur_fidx == *last_frame_index
                            && *has_valid_frame
                            && *last_valid_width == width
                            && *last_valid_height == height
                        {
                            // Producer has not published a newer frame since the last read;
                            // reuse the already-validated frame in `bgra_frame_buf` without re-copying.
                            ipc_ref.touch_consumer_heartbeat();
                            got_ipc_frame = true;
                        } else {
                            let mut r_w = 0u32;
                            let mut r_h = 0u32;
                            let mut r_ts = 0u64;

                            // Always read into `scratch_buf` first so that a mid-copy seqlock
                            // collision never partially overwrites `bgra_frame_buf`.
                            let src_bytes = (hdr.width as usize) * (hdr.height as usize) * 4;
                            if src_bytes > 0 && src_bytes <= MAX_SHARED_FRAME_BYTES {
                                if scratch_buf.len() < src_bytes {
                                    scratch_buf.resize(src_bytes, 0);
                                }
                                if ipc_ref.read_frame(
                                    &mut scratch_buf[..src_bytes],
                                    &mut r_w,
                                    &mut r_h,
                                    &mut r_ts,
                                ) && r_w > 0
                                    && r_h > 0
                                {
                                    let read_bytes = (r_w as usize) * (r_h as usize) * 4;
                                    if r_w == width && r_h == height && read_bytes == bgra_bytes {
                                        bgra_frame_buf[..bgra_bytes]
                                            .copy_from_slice(&scratch_buf[..bgra_bytes]);
                                    } else {
                                        let dst_ptr = bgra_frame_buf.as_mut_ptr();
                                        for dy in 0..height {
                                            let sy = (dy * r_h) / height;
                                            let dst_row = unsafe {
                                                dst_ptr.add((dy as usize) * (width as usize) * 4)
                                            };
                                            let src_row = unsafe {
                                                scratch_buf
                                                    .as_ptr()
                                                    .add((sy as usize) * (r_w as usize) * 4)
                                            };
                                            for dx in 0..width {
                                                let sx = (dx * r_w) / width;
                                                unsafe {
                                                    let spx = src_row.add((sx as usize) * 4);
                                                    let dpx = dst_row.add((dx as usize) * 4);
                                                    *dpx = *spx;
                                                    *dpx.add(1) = *spx.add(1);
                                                    *dpx.add(2) = *spx.add(2);
                                                    *dpx.add(3) = 255;
                                                }
                                            }
                                        }
                                    }
                                    *has_valid_frame = true;
                                    *last_valid_width = width;
                                    *last_valid_height = height;
                                    *last_ipc_seq =
                                        hdr.sequence_counter.load(Ordering::Acquire) & !1;
                                    *last_frame_index = hdr.frame_index;
                                    *standby_rendered_dim = (0, 0);
                                    got_ipc_frame = true;
                                }
                            } else {
                                ipc_ref.touch_consumer_heartbeat();
                            }
                        }
                    } else {
                        ipc_ref.touch_consumer_heartbeat();
                    }
                }
            }
        }

        // If `read_frame` transiently failed (e.g., producer mid-write collision), preserve the
        // last valid frame in `bgra_frame_buf` ("Last-Good-Frame Hold") instead of flashing standby.
        if !got_ipc_frame
            && inner.has_valid_frame
            && inner.last_valid_width == width
            && inner.last_valid_height == height
        {
            got_ipc_frame = true;
        }

        let sample_idx = inner.sample_count;
        if !got_ipc_frame && inner.standby_rendered_dim != (width, height) {
            SmartCamMfMediaStream::render_standby_pattern(
                inner.bgra_frame_buf.as_mut_ptr(),
                width,
                height,
                sample_idx,
            );
            inner.standby_rendered_dim = (width, height);
            inner.nv12_cache_seq = -2;
        }

        // If NV12 is negotiated, maintain a cached tight NV12 buffer so repeated consumer requests
        // for the same producer frame copy pre-converted planes instead of recomputing BT.601 math.
        let nv12_tight_bytes = ((width as usize) * (height as usize) * 3) / 2;
        if is_nv12 {
            let cache_seq_key = if got_ipc_frame {
                inner.last_ipc_seq
            } else {
                -2
            };
            let cache_fidx_key = if got_ipc_frame {
                inner.last_frame_index
            } else {
                0
            };
            if inner.nv12_cache_seq != cache_seq_key
                || inner.nv12_cache_frame_idx != cache_fidx_key
                || inner.nv12_cache_w != width
                || inner.nv12_cache_h != height
                || inner.nv12_cache_buf.len() < nv12_tight_bytes
            {
                if inner.nv12_cache_buf.len() < nv12_tight_bytes {
                    inner.nv12_cache_buf.resize(nv12_tight_bytes, 0);
                }
                let StreamInner {
                    ref bgra_frame_buf,
                    ref mut nv12_cache_buf,
                    ..
                } = *inner;
                SmartCamMfMediaStream::bgra_to_nv12_pitched(
                    &bgra_frame_buf[..bgra_bytes],
                    nv12_cache_buf.as_mut_ptr(),
                    width as usize,
                    width,
                    height,
                );
                inner.nv12_cache_seq = cache_seq_key;
                inner.nv12_cache_frame_idx = cache_fidx_key;
                inner.nv12_cache_w = width;
                inner.nv12_cache_h = height;
            }
        }

        // Allocate sample from FrameServer's provided IMFVideoSampleAllocator if available,
        // otherwise fall back to MFCreateMemoryBuffer + MFCreateSample.
        let (sample, used_provided_allocator): (IMFSample, bool) =
            if let Some(ref alloc) = inner.allocator {
                match unsafe { alloc.AllocateSample() } {
                    Ok(s) => (s, true),
                    Err(e) => {
                        trace_mf(&format!("AllocateSample failed ({:?}), falling back", e));
                        let buf_size = if is_nv12 {
                            (width * height * 3) / 2
                        } else {
                            width * height * 4
                        };
                        let mb = unsafe { MFCreateMemoryBuffer(buf_size)? };
                        let s = unsafe { MFCreateSample()? };
                        unsafe {
                            s.AddBuffer(&mb)?;
                        }
                        (s, false)
                    }
                }
            } else {
                let buf_size = if is_nv12 {
                    (width * height * 3) / 2
                } else {
                    width * height * 4
                };
                let mb = unsafe { MFCreateMemoryBuffer(buf_size)? };
                let s = unsafe { MFCreateSample()? };
                unsafe {
                    s.AddBuffer(&mb)?;
                }
                (s, false)
            };

        let media_buffer: IMFMediaBuffer = unsafe { sample.GetBufferByIndex(0)? };

        if let Ok(buffer_2d) = media_buffer.cast::<IMF2DBuffer2>() {
            let mut scanline = std::ptr::null_mut();
            let mut pitch = 0i32;
            let mut start = std::ptr::null_mut();
            let mut length = 0u32;
            unsafe {
                buffer_2d.Lock2DSize(
                    MF2DBuffer_LockFlags_Write,
                    &mut scanline,
                    &mut pitch,
                    &mut start,
                    &mut length,
                )?;
            }

            let abs_pitch = pitch.unsigned_abs() as usize;
            let w_usize = width as usize;
            let h_usize = height as usize;
            if is_nv12 {
                if abs_pitch == w_usize {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            inner.nv12_cache_buf.as_ptr(),
                            scanline,
                            nv12_tight_bytes,
                        );
                    }
                } else {
                    let row_copy = w_usize.min(abs_pitch);
                    let total_nv12_rows = h_usize + h_usize / 2;
                    for r in 0..total_nv12_rows {
                        unsafe {
                            let src_row = inner.nv12_cache_buf.as_ptr().add(r * w_usize);
                            let dst_row = scanline.add(r * abs_pitch);
                            std::ptr::copy_nonoverlapping(src_row, dst_row, row_copy);
                        }
                    }
                }
            } else {
                let row_bytes = w_usize * 4;
                if pitch >= 0 && abs_pitch == row_bytes {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            inner.bgra_frame_buf.as_ptr(),
                            scanline,
                            bgra_bytes,
                        );
                    }
                } else {
                    for y in 0..h_usize {
                        unsafe {
                            let src_row = inner.bgra_frame_buf.as_ptr().add(y * row_bytes);
                            let dst_row = if pitch >= 0 {
                                scanline.add(y * abs_pitch)
                            } else {
                                scanline.sub(y * abs_pitch)
                            };
                            std::ptr::copy_nonoverlapping(
                                src_row,
                                dst_row,
                                row_bytes.min(abs_pitch),
                            );
                        }
                    }
                }
            }

            unsafe {
                let _ = buffer_2d.Unlock2D();
            }
        } else {
            let mut raw_ptr = std::ptr::null_mut();
            unsafe {
                media_buffer.Lock(&mut raw_ptr, None, None)?;
            }
            let buffer_size = if is_nv12 {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        inner.nv12_cache_buf.as_ptr(),
                        raw_ptr,
                        nv12_tight_bytes,
                    );
                }
                (width * height * 3) / 2
            } else {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        inner.bgra_frame_buf.as_ptr(),
                        raw_ptr,
                        bgra_bytes,
                    );
                }
                width * height * 4
            };
            unsafe {
                let _ = media_buffer.Unlock();
                let _ = media_buffer.SetCurrentLength(buffer_size);
            }
        }

        let sys_time = unsafe { MFGetSystemTime() };
        unsafe {
            sample.SetSampleTime(sys_time)?;
            sample.SetSampleDuration(DEFAULT_SAMPLE_DURATION_100NS)?;
            let _ = sample.SetUINT32(&MFSampleExtension_CleanPoint, 1);
            if let Some(token) = ptoken {
                sample.SetUnknown(&MFSampleExtension_Token, token)?;
            }
        }

        inner.sample_count += 1;
        let count = inner.sample_count;
        let res = unsafe {
            inner
                .event_queue
                .QueueEventParamUnk(MEMediaSample.0 as u32, &GUID::zeroed(), S_OK, &sample)
        };
        trace_mf(&format!(
            "RequestSample #{}: w={} h={} nv12={} ipc={} alloc={} -> {:?}",
            count, width, height, is_nv12, got_ipc_frame, used_provided_allocator, res
        ));
        res
    }
}

impl IMFMediaStream2_Impl for SmartCamMfMediaStream_Impl {
    fn SetStreamState(&self, value: MF_STREAM_STATE) -> Result<()> {
        trace_mf(&format!("Stream::SetStreamState {:?}", value.0));
        let current = self.this.get_stream_state();
        if current == value {
            return Ok(());
        }
        match value {
            MF_STREAM_STATE_PAUSED => {
                if current != MF_STREAM_STATE_RUNNING {
                    return Err(windows::core::Error::from_hresult(
                        MF_E_INVALID_STATE_TRANSITION,
                    ));
                }
                self.this.pause_stream()
            }
            MF_STREAM_STATE_RUNNING => {
                let mut time_pv = PROPVARIANT::default();
                unsafe {
                    let p = &mut time_pv as *mut PROPVARIANT as *mut u8;
                    *(p as *mut u16) = 20; // VT_I8
                    *(p.add(8) as *mut i64) = MFGetSystemTime();
                }
                self.this.start_stream(None, &time_pv)
            }
            MF_STREAM_STATE_STOPPED => self.this.stop_stream(),
            _ => Err(windows::core::Error::from_hresult(
                MF_E_INVALID_STATE_TRANSITION,
            )),
        }
    }

    fn GetStreamState(&self) -> Result<MF_STREAM_STATE> {
        let state = self.this.get_stream_state();
        trace_mf(&format!("Stream::GetStreamState -> {:?}", state.0));
        Ok(state)
    }
}

impl IKsControl_Impl for SmartCamMfMediaStream_Impl {
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
        trace_mf("Stream::KsProperty -> ERROR_SET_NOT_FOUND");
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
        trace_mf("Stream::KsMethod -> ERROR_SET_NOT_FOUND");
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
        trace_mf("Stream::KsEvent -> ERROR_SET_NOT_FOUND");
        HR_ERROR_SET_NOT_FOUND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converts a uniform 2x2 BGRA frame and returns (Y, U, V) of the result.
    fn nv12_of(b: u8, g: u8, r: u8) -> (u8, u8, u8) {
        let src: Vec<u8> = (0..4).flat_map(|_| [b, g, r, 255]).collect();
        let mut dst = vec![0u8; 2 * 2 * 3 / 2];
        SmartCamMfMediaStream::bgra_to_nv12_pitched(&src, dst.as_mut_ptr(), 2, 2, 2);
        assert!(dst[..4].iter().all(|&y| y == dst[0]), "uniform luma");
        (dst[0], dst[4], dst[5])
    }

    #[test]
    fn nv12_converter_writes_the_encoding_the_media_type_advertises() {
        assert_eq!(NV12_YUV_MATRIX, MFVideoTransferMatrix_BT601);
        assert_eq!(NV12_NOMINAL_RANGE, MFNominalRange_16_235);
        // BT.601, studio range: black and white at the range ends, red with the BT.601 luma
        // weight (16 + 219 * 0.299 = 81.5; BT.709 would give about 63).
        assert_eq!(nv12_of(0, 0, 0), (16, 128, 128), "black");
        assert_eq!(nv12_of(255, 255, 255), (235, 128, 128), "white");
        assert_eq!(nv12_of(0, 0, 255), (82, 90, 240), "red");
    }
}
