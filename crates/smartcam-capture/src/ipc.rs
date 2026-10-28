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

use std::sync::atomic::Ordering;

use crate::header::{
    SharedFrameHeader, DEFAULT_SHARED_EVENT_NAME, DEFAULT_SHARED_MAPPING_NAME,
    MAX_SHARED_FRAME_BYTES, SHARED_FRAME_HEADER_BYTES, SHARED_FRAME_MAGIC,
    TOTAL_SHARED_FRAME_BYTES,
};

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE},
        Security::{
            InitializeSecurityDescriptor, SetSecurityDescriptorDacl,
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
        },
        System::{
            Memory::{
                CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
                FILE_MAP_ALL_ACCESS, FILE_MAP_READ, PAGE_READWRITE,
            },
            SystemInformation::GetTickCount64,
            Threading::{CreateEventW, OpenEventW, SetEvent, EVENT_MODIFY_STATE},
        },
    },
};

#[cfg(windows)]
const SECURITY_DESCRIPTOR_REVISION: u32 = 1;

/// High-performance shared memory inter-process communication wrapper.
/// Compatible with both the Windows Media Foundation COM Virtual Camera driver
/// and the SmartCam user-mode compositing engine (`FallbackSharedMemoryWriter`).
pub struct SharedFrameIpc {
    #[cfg(windows)]
    mapping_handle: Option<HANDLE>,
    #[cfg(windows)]
    event_handle: Option<HANDLE>,
    mapped_view: *mut u8,
    is_producer: bool,
    read_only: bool,
    #[cfg(not(windows))]
    fallback_buf: Option<Vec<u8>>,
}

unsafe impl Send for SharedFrameIpc {}
unsafe impl Sync for SharedFrameIpc {}

impl SharedFrameIpc {
    /// Creates or opens a named shared memory mapping and synchronization event.
    #[cfg(windows)]
    pub fn open_or_create(
        mapping_name: &str,
        event_name: &str,
        is_producer: bool,
    ) -> Option<Self> {
        let wide_map: Vec<u16> = mapping_name.encode_utf16().chain(std::iter::once(0)).collect();
        let wide_ev: Vec<u16> = event_name.encode_utf16().chain(std::iter::once(0)).collect();

        let total_bytes = TOTAL_SHARED_FRAME_BYTES;
        let size_high = ((total_bytes as u64 >> 32) & 0xFFFF_FFFF) as u32;
        let size_low = (total_bytes as u64 & 0xFFFF_FFFF) as u32;

        unsafe {
            let mut sd = SECURITY_DESCRIPTOR::default();
            let mut sa = SECURITY_ATTRIBUTES::default();
            let mut p_sa: *const SECURITY_ATTRIBUTES = std::ptr::null();

            if InitializeSecurityDescriptor(
                PSECURITY_DESCRIPTOR(&mut sd as *mut _ as *mut core::ffi::c_void),
                SECURITY_DESCRIPTOR_REVISION,
            )
            .is_ok()
                && SetSecurityDescriptorDacl(
                    PSECURITY_DESCRIPTOR(&mut sd as *mut _ as *mut core::ffi::c_void),
                    true,
                    None,
                    false,
                )
                .is_ok()
            {
                sa.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
                sa.lpSecurityDescriptor = &mut sd as *mut _ as *mut core::ffi::c_void;
                sa.bInheritHandle = false.into();
                p_sa = &sa;
            }

            let mapping_handle: Option<HANDLE>;
            let mut read_only = false;
            let mut newly_created = false;

            if !is_producer {
                let h = OpenFileMappingW(
                    FILE_MAP_ALL_ACCESS.0,
                    false,
                    PCWSTR(wide_map.as_ptr()),
                );
                if let Ok(h) = h {
                    mapping_handle = Some(h);
                } else {
                    let h_ro = OpenFileMappingW(
                        FILE_MAP_READ.0,
                        false,
                        PCWSTR(wide_map.as_ptr()),
                    );
                    if let Ok(h_ro) = h_ro {
                        mapping_handle = Some(h_ro);
                        read_only = true;
                    } else {
                        return None;
                    }
                }
            } else {
                let h = CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    if p_sa.is_null() { None } else { Some(p_sa) },
                    PAGE_READWRITE,
                    size_high,
                    size_low,
                    PCWSTR(wide_map.as_ptr()),
                );

                if let Ok(h) = h {
                    newly_created = GetLastError() != ERROR_ALREADY_EXISTS;
                    mapping_handle = Some(h);
                } else {
                    let h_open = OpenFileMappingW(
                        FILE_MAP_ALL_ACCESS.0,
                        false,
                        PCWSTR(wide_map.as_ptr()),
                    );
                    if let Ok(h_open) = h_open {
                        mapping_handle = Some(h_open);
                    } else {
                        return None;
                    }
                }
            }

            let handle = mapping_handle?;
            let access = if read_only {
                FILE_MAP_READ
            } else {
                FILE_MAP_ALL_ACCESS
            };

            let view = MapViewOfFile(handle, access, 0, 0, total_bytes);
            let mut mapped_view = view.Value as *mut u8;

            if mapped_view.is_null() && !read_only {
                let view_ro = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, total_bytes);
                mapped_view = view_ro.Value as *mut u8;
                if !mapped_view.is_null() {
                    read_only = true;
                }
            }

            if mapped_view.is_null() {
                let _ = CloseHandle(handle);
                return None;
            }

            let mut event_handle: Option<HANDLE> = None;
            if is_producer {
                let ev = CreateEventW(
                    if p_sa.is_null() { None } else { Some(p_sa) },
                    false,
                    false,
                    PCWSTR(wide_ev.as_ptr()),
                );
                if let Ok(ev) = ev {
                    event_handle = Some(ev);
                }
            } else {
                let ev = OpenEventW(
                    EVENT_MODIFY_STATE,
                    false,
                    PCWSTR(wide_ev.as_ptr()),
                );
                if let Ok(ev) = ev {
                    event_handle = Some(ev);
                }
            }

            if !read_only {
                let hdr = &mut *(mapped_view as *mut SharedFrameHeader);
                if newly_created || hdr.magic != SHARED_FRAME_MAGIC {
                    std::ptr::write_bytes(mapped_view, 0, SHARED_FRAME_HEADER_BYTES);
                    hdr.magic = SHARED_FRAME_MAGIC;
                    hdr.version = 1;
                    hdr.sequence_counter = std::sync::atomic::AtomicI64::new(0);
                    hdr.width = 1440;
                    hdr.height = 1080;
                    hdr.stride = 1440 * 4;
                    hdr.fps_num = 30;
                    hdr.fps_den = 1;
                    hdr.pixel_format = 4; // BGRA32
                }
            }

            Some(Self {
                mapping_handle: Some(handle),
                event_handle,
                mapped_view,
                is_producer,
                read_only,
            })
        }
    }

    /// Cross-platform memory fallback (for Linux, macOS, and simulated tests).
    #[cfg(not(windows))]
    pub fn open_or_create(
        _mapping_name: &str,
        _event_name: &str,
        is_producer: bool,
    ) -> Option<Self> {
        let total_bytes = TOTAL_SHARED_FRAME_BYTES;
        let mut buf = vec![0u8; total_bytes];
        let ptr = buf.as_mut_ptr();
        if is_producer {
            unsafe {
                let hdr = &mut *(ptr as *mut SharedFrameHeader);
                hdr.magic = SHARED_FRAME_MAGIC;
                hdr.version = 1;
                hdr.width = 1440;
                hdr.height = 1080;
                hdr.stride = 1440 * 4;
                hdr.fps_num = 30;
                hdr.fps_den = 1;
                hdr.pixel_format = 4;
            }
        }
        Some(Self {
            mapped_view: ptr,
            is_producer,
            read_only: false,
            fallback_buf: Some(buf),
        })
    }

    /// Convenience default open for producer.
    pub fn open_default_producer() -> Option<Self> {
        Self::open_or_create(DEFAULT_SHARED_MAPPING_NAME, DEFAULT_SHARED_EVENT_NAME, true)
    }

    /// Convenience default open for consumer.
    pub fn open_default_consumer() -> Option<Self> {
        Self::open_or_create(DEFAULT_SHARED_MAPPING_NAME, DEFAULT_SHARED_EVENT_NAME, false)
    }

    /// Returns true if this IPC instance was created as a producer.
    pub fn is_producer(&self) -> bool {
        self.is_producer
    }

    /// Returns whether the mapping is currently open.
    pub fn is_open(&self) -> bool {
        !self.mapped_view.is_null()
    }

    /// Returns a reference to the 120-byte `SharedFrameHeader` if open.
    pub fn header(&self) -> Option<&SharedFrameHeader> {
        if self.mapped_view.is_null() {
            None
        } else {
            unsafe { (self.mapped_view as *const SharedFrameHeader).as_ref() }
        }
    }

    /// Returns a mutable reference to the 120-byte `SharedFrameHeader` if open and not read-only.
    pub fn header_mut(&mut self) -> Option<&mut SharedFrameHeader> {
        if self.mapped_view.is_null() || self.read_only {
            None
        } else {
            unsafe { (self.mapped_view as *mut SharedFrameHeader).as_mut() }
        }
    }

    /// Returns a raw pointer to the pixel payload (offset 120).
    pub fn payload(&self) -> Option<*const u8> {
        if self.mapped_view.is_null() {
            None
        } else {
            unsafe { Some(self.mapped_view.add(SHARED_FRAME_HEADER_BYTES)) }
        }
    }

    /// Returns a raw mutable pointer to the pixel payload (offset 120).
    pub fn payload_mut(&mut self) -> Option<*mut u8> {
        if self.mapped_view.is_null() || self.read_only {
            None
        } else {
            unsafe { Some(self.mapped_view.add(SHARED_FRAME_HEADER_BYTES)) }
        }
    }

    /// Writes a new frame into shared memory using double-counter sequence lock semantics.
    pub fn write_frame(
        &self,
        src_data: &[u8],
        width: u32,
        height: u32,
        src_stride: u32,
        fps_num: u32,
        fps_den: u32,
        timestamp_100ns: u64,
        frame_index: u64,
    ) -> bool {
        if self.mapped_view.is_null() || self.read_only || width == 0 || height == 0 {
            return false;
        }

        let dst_stride = width as usize * 4;
        let frame_bytes = dst_stride * height as usize;
        if frame_bytes == 0 || frame_bytes > MAX_SHARED_FRAME_BYTES || src_data.len() < frame_bytes {
            return false;
        }

        unsafe {
            let hdr = &*(self.mapped_view as *const SharedFrameHeader);

            // Increment sequence counter to odd value (writer in progress)
            hdr.sequence_counter.fetch_add(1, Ordering::SeqCst);

            let hdr_mut = &mut *(self.mapped_view as *mut SharedFrameHeader);
            hdr_mut.width = width;
            hdr_mut.height = height;
            hdr_mut.stride = dst_stride as u32;
            hdr_mut.fps_num = fps_num;
            hdr_mut.fps_den = fps_den;
            hdr_mut.timestamp_100ns = timestamp_100ns;
            hdr_mut.frame_index = frame_index;

            let dst_payload = self.mapped_view.add(SHARED_FRAME_HEADER_BYTES);

            if src_stride as usize == dst_stride {
                std::ptr::copy_nonoverlapping(src_data.as_ptr(), dst_payload, frame_bytes);
            } else {
                for y in 0..height as usize {
                    let src_row = src_data.as_ptr().add(y * src_stride as usize);
                    let dst_row = dst_payload.add(y * dst_stride);
                    std::ptr::copy_nonoverlapping(src_row, dst_row, dst_stride);
                }
            }

            // Increment sequence counter to even value (consistent frame ready)
            hdr.sequence_counter.fetch_add(1, Ordering::SeqCst);

            #[cfg(windows)]
            if let Some(ev) = self.event_handle {
                let _ = SetEvent(ev);
            }

            true
        }
    }

    /// Reads the latest consistent frame from shared memory using sequence-lock verification.
    pub fn read_frame(
        &self,
        dst_buffer: &mut [u8],
        out_width: &mut u32,
        out_height: &mut u32,
        out_timestamp_100ns: &mut u64,
    ) -> bool {
        if self.mapped_view.is_null() {
            return false;
        }

        self.touch_consumer_heartbeat();

        let hdr = match self.header() {
            Some(h) => h,
            None => return false,
        };

        if hdr.magic != SHARED_FRAME_MAGIC || hdr.sequence_counter.load(Ordering::SeqCst) == 0 {
            return false;
        }

        for _ in 0..3 {
            let seq_before = hdr.sequence_counter.load(Ordering::SeqCst);
            if (seq_before & 1) != 0 {
                std::thread::yield_now();
                continue;
            }

            let w = hdr.width;
            let h = hdr.height;
            let needed = (w as usize) * (h as usize) * 4;
            let ts = hdr.timestamp_100ns;

            if needed == 0 || needed > dst_buffer.len() || needed > MAX_SHARED_FRAME_BYTES {
                return false;
            }

            unsafe {
                let payload = self.mapped_view.add(SHARED_FRAME_HEADER_BYTES);
                std::ptr::copy_nonoverlapping(payload, dst_buffer.as_mut_ptr(), needed);
            }

            let seq_after = hdr.sequence_counter.load(Ordering::SeqCst);
            if seq_before == seq_after && seq_after > 0 {
                *out_width = w;
                *out_height = h;
                *out_timestamp_100ns = ts;
                return true;
            }
        }

        false
    }

    /// Records consumer activity heartbeat and increments request counter.
    pub fn touch_consumer_heartbeat(&self) {
        if self.mapped_view.is_null() || self.read_only {
            return;
        }
        if let Some(hdr) = self.header() {
            if hdr.magic == SHARED_FRAME_MAGIC {
                #[cfg(windows)]
                let now_ms = unsafe { GetTickCount64() as i64 };
                #[cfg(not(windows))]
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                hdr.last_consumer_heartbeat_ms.store(now_ms, Ordering::SeqCst);
                hdr.consumer_requests.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// Checks if a consumer (Zoom, Teams, Chrome, etc.) requested a frame recently.
    pub fn is_consumer_active(&self, timeout_ms: u64) -> bool {
        let hdr = match self.header() {
            Some(h) => h,
            None => return false,
        };
        if hdr.magic != SHARED_FRAME_MAGIC {
            return false;
        }
        let last_hb = hdr.last_consumer_heartbeat_ms.load(Ordering::SeqCst);
        if last_hb <= 0 {
            return false;
        }

        #[cfg(windows)]
        let now_ms = unsafe { GetTickCount64() };
        #[cfg(not(windows))]
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let hb_ms = last_hb as u64;
        (now_ms >= hb_ms) && ((now_ms - hb_ms) <= timeout_ms)
    }

    /// Closes and releases shared memory and synchronization objects.
    pub fn close(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.mapped_view.is_null() {
                let _ = UnmapViewOfFile(windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.mapped_view as *mut core::ffi::c_void,
                });
                self.mapped_view = std::ptr::null_mut();
            }
            if let Some(h) = self.mapping_handle.take() {
                let _ = CloseHandle(h);
            }
            if let Some(ev) = self.event_handle.take() {
                let _ = CloseHandle(ev);
            }
        }

        #[cfg(not(windows))]
        {
            self.mapped_view = std::ptr::null_mut();
            self.fallback_buf = None;
        }

        self.read_only = false;
    }
}

impl Drop for SharedFrameIpc {
    fn drop(&mut self) {
        self.close();
    }
}
