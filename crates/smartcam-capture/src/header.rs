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

use std::sync::atomic::AtomicI64;

/// CLSID for SmartCam Windows 10/11 Media Foundation Virtual Camera Source:
/// `{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}`
pub const SMARTCAM_MF_CLSID_STR: &str = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}";

/// Default Windows Named Shared Memory mapping for zero-elevation IPC.
pub const DEFAULT_SHARED_MAPPING_NAME: &str = "Local\\SmartCam_MF_SharedFrame_v1";

/// Default Windows Named Event signaled on each new frame.
pub const DEFAULT_SHARED_EVENT_NAME: &str = "Local\\SmartCam_MF_FrameEvent_v1";

/// Magic identifier `"SMCM"` (`0x534D434D`).
pub const SHARED_FRAME_MAGIC: u32 = 0x534D_434D;

/// Protocol version (`1`).
pub const SHARED_FRAME_VERSION: u32 = 1;

/// Maximum frame payload size (`3840 * 2160 * 4` = 33,177,600 bytes for 4K BGRA32).
pub const MAX_SHARED_FRAME_BYTES: usize = 3840 * 2160 * 4;

/// Header size in bytes.
pub const SHARED_FRAME_HEADER_BYTES: usize = 120;

/// Total shared memory mapping size.
pub const TOTAL_SHARED_FRAME_BYTES: usize = SHARED_FRAME_HEADER_BYTES + MAX_SHARED_FRAME_BYTES;

/// Binary-compatible 120-byte Shared Memory IPC header shared between the SmartCam
/// user-mode producer (`smartcam-capture` / `FallbackSharedMemoryWriter`) and the Media Foundation
/// COM Virtual Camera consumer (`smartcam-mf-source`).
#[repr(C)]
pub struct SharedFrameHeader {
    pub magic: u32,
    pub version: u32,
    pub sequence_counter: AtomicI64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub pixel_format: u32,
    pub timestamp_100ns: u64,
    pub frame_index: u64,
    pub last_consumer_heartbeat_ms: AtomicI64,
    pub consumer_requests: AtomicI64,
    pub reserved: [u8; 48],
}

const _: () = assert!(
    std::mem::size_of::<SharedFrameHeader>() == SHARED_FRAME_HEADER_BYTES,
    "SharedFrameHeader must be exactly 120 bytes to preserve the IPC ABI"
);
const _: () = assert!(
    std::mem::align_of::<SharedFrameHeader>() == 8,
    "SharedFrameHeader alignment must be 8 bytes"
);

impl Default for SharedFrameHeader {
    fn default() -> Self {
        Self {
            magic: SHARED_FRAME_MAGIC,
            version: SHARED_FRAME_VERSION,
            sequence_counter: AtomicI64::new(0),
            width: 1440,
            height: 1080,
            stride: 1440 * 4,
            fps_num: 30,
            fps_den: 1,
            pixel_format: 4, // BGRA32
            timestamp_100ns: 0,
            frame_index: 0,
            last_consumer_heartbeat_ms: AtomicI64::new(0),
            consumer_requests: AtomicI64::new(0),
            reserved: [0u8; 48],
        }
    }
}
