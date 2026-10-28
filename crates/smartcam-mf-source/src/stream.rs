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
    core::{implement, IUnknown, Result, GUID, HRESULT},
    Win32::{
        Foundation::{E_POINTER, S_OK},
        Media::MediaFoundation::{
            IMFAsyncCallback, IMFAsyncResult, IMFAttributes, IMFMediaBuffer, IMFMediaEvent,
            IMFMediaEventGenerator_Impl, IMFMediaEventQueue, IMFMediaSource,
            IMFMediaStream, IMFMediaStream_Impl, IMFStreamDescriptor,
            MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS, MEMediaSample, MEStreamPaused,
            MEStreamStarted, MEStreamStopped, MFCreateAttributes, MFCreateEventQueue,
            MFCreateMemoryBuffer, MFCreateSample, MFSampleExtension_Token, MF_E_SHUTDOWN,
            MF_DEVICESTREAM_FRAMESERVER_SHARED, MF_DEVICESTREAM_STREAM_CATEGORY,
            MF_DEVICESTREAM_STREAM_ID,
        },
        System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
    },
};
use windows_core::PROPVARIANT;

use smartcam_capture::{
    SharedFrameIpc, DEFAULT_SHARED_EVENT_NAME, DEFAULT_SHARED_MAPPING_NAME,
    MAX_SHARED_FRAME_BYTES, SHARED_FRAME_MAGIC,
};

use crate::constants::{
    DEFAULT_SAMPLE_DURATION_100NS, DEFAULT_STREAM_HEIGHT, DEFAULT_STREAM_WIDTH,
    PINNAME_VIDEO_CAPTURE,
};

struct StreamInner {
    parent_source: Option<IMFMediaSource>,
    stream_descriptor: IMFStreamDescriptor,
    stream_attributes: IMFAttributes,
    event_queue: IMFMediaEventQueue,
    ipc: Option<SharedFrameIpc>,
    scratch_buf: Vec<u8>,
    sample_count: u64,
    width: u32,
    height: u32,
    is_shutdown: bool,
}

#[derive(Clone)]
#[implement(IMFMediaStream)]
pub struct SmartCamMfMediaStream {
    inner: Arc<Mutex<StreamInner>>,
}

impl SmartCamMfMediaStream {
    pub fn new(
        parent_source: Option<IMFMediaSource>,
        stream_descriptor: IMFStreamDescriptor,
    ) -> Result<Self> {
        let event_queue = unsafe { MFCreateEventQueue()? };

        let mut stream_attributes = None;
        unsafe {
            MFCreateAttributes(&mut stream_attributes, 4)?;
        }
        let stream_attributes = stream_attributes.ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?;

        unsafe {
            stream_attributes.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
            stream_attributes.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0)?;
            stream_attributes.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
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
            ipc,
            scratch_buf: Vec::new(),
            sample_count: 0,
            width: DEFAULT_STREAM_WIDTH,
            height: DEFAULT_STREAM_HEIGHT,
            is_shutdown: false,
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub fn start_stream(&self, start_pos: *const PROPVARIANT) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
        if let Some(ref ipc) = inner.ipc {
            ipc.touch_consumer_heartbeat();
        }
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
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
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
        let inner = self.inner.lock().unwrap();
        if inner.is_shutdown {
            return Err(windows::core::Error::from_hresult(MF_E_SHUTDOWN));
        }
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

    pub fn render_standby_pattern(dst: *mut u8, width: u32, height: u32, sample_count: u64) {
        let bar_offset = ((sample_count * 4) % (width as u64)) as u32;
        unsafe {
            for y in 0..height {
                let row = dst.add((y as usize) * (width as usize) * 4);
                let grad_y = ((y * 180) / height + 30) as u8;
                for x in 0..width {
                    let is_pulse = ((x + bar_offset) % 120) < 6;
                    let px = row.add((x as usize) * 4);
                    *px = if is_pulse { 240 } else { 56 }; // B
                    *px.add(1) = if is_pulse { 200 } else { grad_y }; // G
                    *px.add(2) = if is_pulse { 80 } else { 24 }; // R
                    *px.add(3) = 255; // A
                }
            }
        }
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

        let width = inner.width;
        let height = inner.height;
        let buffer_size = width * height * 4;

        let media_buffer: IMFMediaBuffer = unsafe { MFCreateMemoryBuffer(buffer_size)? };

        let mut raw_ptr = std::ptr::null_mut();
        unsafe {
            media_buffer.Lock(&mut raw_ptr, None, None)?;
        }

        let mut got_ipc_frame = false;
        let mut ts_100ns = 0u64;

        // Ensure IPC mapping is active
        if inner.ipc.is_none() {
            inner.ipc = SharedFrameIpc::open_or_create(
                DEFAULT_SHARED_MAPPING_NAME,
                DEFAULT_SHARED_EVENT_NAME,
                false,
            );
        }

        let StreamInner {
            ref ipc,
            ref mut scratch_buf,
            ref mut sample_count,
            ref event_queue,
            ..
        } = *inner;

        if let Some(ref ipc_ref) = *ipc {
            ipc_ref.touch_consumer_heartbeat();
            if let Some(hdr) = ipc_ref.header() {
                if hdr.magic == SHARED_FRAME_MAGIC && hdr.width > 0 && hdr.height > 0 {
                    let mut r_w = 0u32;
                    let mut r_h = 0u32;
                    let mut r_ts = 0u64;

                    if hdr.width == width && hdr.height == height {
                        let dst_slice = unsafe { std::slice::from_raw_parts_mut(raw_ptr, buffer_size as usize) };
                        if ipc_ref.read_frame(dst_slice, &mut r_w, &mut r_h, &mut r_ts) {
                            got_ipc_frame = true;
                            ts_100ns = r_ts;
                        }
                    } else {
                        let src_bytes = (hdr.width as usize) * (hdr.height as usize) * 4;
                        if src_bytes <= MAX_SHARED_FRAME_BYTES {
                            if scratch_buf.len() < src_bytes {
                                scratch_buf.resize(src_bytes, 0);
                            }
                            if ipc_ref.read_frame(&mut scratch_buf[..src_bytes], &mut r_w, &mut r_h, &mut r_ts)
                                && r_w > 0
                                && r_h > 0
                            {
                                // Rescale row-by-row into destination buffer
                                for dy in 0..height {
                                    let sy = (dy * r_h) / height;
                                    let dst_row = unsafe { raw_ptr.add((dy as usize) * (width as usize) * 4) };
                                    let src_row = unsafe { scratch_buf.as_ptr().add((sy as usize) * (r_w as usize) * 4) };
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
                                got_ipc_frame = true;
                                ts_100ns = r_ts;
                            }
                        }
                    }
                }
            }
        }

        let sample_idx = *sample_count;
        if !got_ipc_frame {
            SmartCamMfMediaStream::render_standby_pattern(raw_ptr, width, height, sample_idx);
            let mut qpc = 0i64;
            let mut freq = 0i64;
            unsafe {
                let _ = QueryPerformanceCounter(&mut qpc);
                let _ = QueryPerformanceFrequency(&mut freq);
            }
            ts_100ns = if freq > 0 {
                (qpc as u64 * 10_000_000) / (freq as u64)
            } else {
                0
            };
        }

        unsafe {
            let _ = media_buffer.Unlock();
            media_buffer.SetCurrentLength(buffer_size)?;
        }

        let sample = unsafe { MFCreateSample()? };
        unsafe {
            sample.AddBuffer(&media_buffer)?;
            sample.SetSampleTime(ts_100ns as i64)?;
            sample.SetSampleDuration(DEFAULT_SAMPLE_DURATION_100NS)?;
            if let Some(token) = ptoken {
                sample.SetUnknown(&MFSampleExtension_Token, token)?;
            }
        }

        *sample_count += 1;
        unsafe {
            event_queue.QueueEventParamUnk(
                MEMediaSample.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &sample,
            )
        }
    }
}
