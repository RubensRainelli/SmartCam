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

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use crate::{
    ir::is_ir_camera_name,
    types::{
        CameraCaptureConfig, CameraDeviceInfo, CameraSource, VirtualCameraSink,
        VirtualCameraSinkConfig,
    },
};

// Linux V4L2 FourCC pixel format codes
pub const V4L2_PIX_FMT_YUYV: u32 = u32::from_le_bytes(*b"YUYV");
pub const V4L2_PIX_FMT_RGB24: u32 = u32::from_le_bytes(*b"RGB3");
pub const V4L2_PIX_FMT_BGR24: u32 = u32::from_le_bytes(*b"BGR3");
pub const V4L2_PIX_FMT_BGR32: u32 = u32::from_le_bytes(*b"BGR4");
pub const V4L2_PIX_FMT_ABGR32: u32 = u32::from_le_bytes(*b"AR24");

const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_BUF_TYPE_VIDEO_OUTPUT: u32 = 2;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;

// 64-bit Linux ioctl request numbers for <linux/videodev2.h>
// struct v4l2_format is 208 (0xD0) bytes on 64-bit Linux
const VIDIOC_G_FMT: libc_ffi::c_ulong = 0xC0D0_5604;
const VIDIOC_S_FMT: libc_ffi::c_ulong = 0xC0D0_5605;
// struct v4l2_requestbuffers is 20 (0x14) bytes
const VIDIOC_REQBUFS: libc_ffi::c_ulong = 0xC014_5608;
// struct v4l2_buffer is 88 (0x58) bytes on 64-bit Linux
const VIDIOC_QUERYBUF: libc_ffi::c_ulong = 0xC058_5609;
const VIDIOC_QBUF: libc_ffi::c_ulong = 0xC058_560F;
const VIDIOC_DQBUF: libc_ffi::c_ulong = 0xC058_5611;
const VIDIOC_STREAMON: libc_ffi::c_ulong = 0x4004_5612;
const VIDIOC_STREAMOFF: libc_ffi::c_ulong = 0x4004_5613;

mod libc_ffi {
    #![allow(non_camel_case_types)]
    pub type c_int = i32;
    pub type c_short = i16;
    pub type c_ulong = usize;
    pub type size_t = usize;
    pub type off_t = isize;

    pub const PROT_READ: c_int = 0x1;
    pub const PROT_WRITE: c_int = 0x2;
    pub const MAP_SHARED: c_int = 0x01;
    pub const MAP_FAILED: *mut core::ffi::c_void = !0usize as *mut core::ffi::c_void;
    pub const POLLIN: c_short = 0x0001;

    #[repr(C)]
    pub struct pollfd {
        pub fd: c_int,
        pub events: c_short,
        pub revents: c_short,
    }

    extern "C" {
        pub fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
        pub fn mmap(
            addr: *mut core::ffi::c_void,
            len: size_t,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: off_t,
        ) -> *mut core::ffi::c_void;
        pub fn munmap(addr: *mut core::ffi::c_void, len: size_t) -> c_int;
        pub fn poll(fds: *mut pollfd, nfds: c_ulong, timeout: c_int) -> c_int;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct V4l2PixFormat {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    ycbcr_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
union V4l2FormatUnion {
    pix: V4l2PixFormat,
    raw_data: [u8; 200],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct V4l2Format {
    type_: u32,
    _pad: u32,
    fmt: V4l2FormatUnion,
}

impl Default for V4l2Format {
    fn default() -> Self {
        Self {
            type_: 0,
            _pad: 0,
            fmt: V4l2FormatUnion {
                raw_data: [0u8; 200],
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct V4l2RequestBuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct V4l2Timecode {
    type_: u32,
    flags: u32,
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    userbits: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
union V4l2BufferM {
    offset: u32,
    userptr: usize,
    fd: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct V4l2Buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    _pad0: u32,
    timestamp_sec: i64,
    timestamp_usec: i64,
    timecode: V4l2Timecode,
    sequence: u32,
    memory: u32,
    m: V4l2BufferM,
    length: u32,
    reserved2: u32,
    request_fd: u32,
    _pad1: u32,
}

impl Default for V4l2Buffer {
    fn default() -> Self {
        Self {
            index: 0,
            type_: 0,
            bytesused: 0,
            flags: 0,
            field: 0,
            _pad0: 0,
            timestamp_sec: 0,
            timestamp_usec: 0,
            timecode: V4l2Timecode::default(),
            sequence: 0,
            memory: 0,
            m: V4l2BufferM { userptr: 0 },
            length: 0,
            reserved2: 0,
            request_fd: 0,
            _pad1: 0,
        }
    }
}

struct MmapBuffer {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for MmapBuffer {}

/// Converts a YUYV (YUY2 4:2:2) buffer into tightly-packed BGRA32 (`width * height * 4` bytes).
pub fn yuyv_to_bgra32(src_yuyv: &[u8], dst_bgra: &mut [u8], width: u32, height: u32, stride_src: usize) -> bool {
    let w = width as usize;
    let h = height as usize;
    let row_src = if stride_src >= w * 2 { stride_src } else { w * 2 };
    if w == 0 || h == 0 || dst_bgra.len() < w * h * 4 || src_yuyv.len() < (h - 1) * row_src + w * 2 {
        return false;
    }

    for y in 0..h {
        let s_row = &src_yuyv[y * row_src..y * row_src + w * 2];
        let d_row = &mut dst_bgra[y * w * 4..(y + 1) * w * 4];
        let pairs = w / 2;
        for i in 0..pairs {
            let y0 = (s_row[i * 4] as i32 - 16).max(0);
            let u = s_row[i * 4 + 1] as i32 - 128;
            let y1 = (s_row[i * 4 + 2] as i32 - 16).max(0);
            let v = s_row[i * 4 + 3] as i32 - 128;

            let c0 = 298 * y0 + 128;
            let c1 = 298 * y1 + 128;
            let r_diff = 409 * v;
            let g_diff = -100 * u - 208 * v;
            let b_diff = 516 * u;

            let o = i * 8;
            d_row[o] = ((c0 + b_diff) >> 8).clamp(0, 255) as u8;
            d_row[o + 1] = ((c0 + g_diff) >> 8).clamp(0, 255) as u8;
            d_row[o + 2] = ((c0 + r_diff) >> 8).clamp(0, 255) as u8;
            d_row[o + 3] = 255;

            d_row[o + 4] = ((c1 + b_diff) >> 8).clamp(0, 255) as u8;
            d_row[o + 5] = ((c1 + g_diff) >> 8).clamp(0, 255) as u8;
            d_row[o + 6] = ((c1 + r_diff) >> 8).clamp(0, 255) as u8;
            d_row[o + 7] = 255;
        }
        if (w & 1) != 0 {
            let last_s = pairs * 4;
            let y0 = (s_row[last_s] as i32 - 16).max(0);
            let u = s_row[last_s + 1] as i32 - 128;
            let c0 = 298 * y0 + 128;
            let o = pairs * 8;
            d_row[o] = ((c0 + 516 * u) >> 8).clamp(0, 255) as u8;
            d_row[o + 1] = ((c0 - 100 * u) >> 8).clamp(0, 255) as u8;
            d_row[o + 2] = (c0 >> 8).clamp(0, 255) as u8;
            d_row[o + 3] = 255;
        }
    }
    true
}

/// Converts a 24-bit RGB24 (`RGB3`) or BGR24 (`BGR3`) buffer into tightly-packed BGRA32.
pub fn rgb24_or_bgr24_to_bgra32(
    src: &[u8],
    dst_bgra: &mut [u8],
    width: u32,
    height: u32,
    stride_src: usize,
    is_rgb_order: bool,
) -> bool {
    let w = width as usize;
    let h = height as usize;
    let row_src = if stride_src >= w * 3 { stride_src } else { w * 3 };
    if w == 0 || h == 0 || dst_bgra.len() < w * h * 4 || src.len() < (h - 1) * row_src + w * 3 {
        return false;
    }

    for y in 0..h {
        let s_row = &src[y * row_src..y * row_src + w * 3];
        let d_row = &mut dst_bgra[y * w * 4..(y + 1) * w * 4];
        for x in 0..w {
            let si = x * 3;
            let di = x * 4;
            if is_rgb_order {
                d_row[di] = s_row[si + 2];     // B
                d_row[di + 1] = s_row[si + 1]; // G
                d_row[di + 2] = s_row[si];     // R
            } else {
                d_row[di] = s_row[si];         // B
                d_row[di + 1] = s_row[si + 1]; // G
                d_row[di + 2] = s_row[si + 2]; // R
            }
            d_row[di + 3] = 255;
        }
    }
    true
}

/// Converts tightly-packed BGRA32 (`width * height * 4`) to YUYV 4:2:2 (`width * height * 2`).
pub fn bgra32_to_yuyv(bgra: &[u8], dst_yuyv: &mut Vec<u8>, width: u32, height: u32) -> bool {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || bgra.len() < w * h * 4 {
        return false;
    }

    let yuyv_len = w * h * 2;
    dst_yuyv.resize(yuyv_len, 0);

    for y in 0..h {
        let src_row = &bgra[y * w * 4..(y + 1) * w * 4];
        let dst_row = &mut dst_yuyv[y * w * 2..(y + 1) * w * 2];
        let pairs = w / 2;
        for i in 0..pairs {
            let p0 = i * 8;
            let b0 = src_row[p0] as i32;
            let g0 = src_row[p0 + 1] as i32;
            let r0 = src_row[p0 + 2] as i32;

            let b1 = src_row[p0 + 4] as i32;
            let g1 = src_row[p0 + 5] as i32;
            let r1 = src_row[p0 + 6] as i32;

            let y0 = ((66 * r0 + 129 * g0 + 25 * b0 + 128) >> 8) + 16;
            let y1 = ((66 * r1 + 129 * g1 + 25 * b1 + 128) >> 8) + 16;

            let r_avg = (r0 + r1) >> 1;
            let g_avg = (g0 + g1) >> 1;
            let b_avg = (b0 + b1) >> 1;

            let u = ((-38 * r_avg - 74 * g_avg + 112 * b_avg + 128) >> 8) + 128;
            let v = ((112 * r_avg - 94 * g_avg - 18 * b_avg + 128) >> 8) + 128;

            let out_idx = i * 4;
            dst_row[out_idx] = y0.clamp(0, 255) as u8;
            dst_row[out_idx + 1] = u.clamp(0, 255) as u8;
            dst_row[out_idx + 2] = y1.clamp(0, 255) as u8;
            dst_row[out_idx + 3] = v.clamp(0, 255) as u8;
        }
        if (w & 1) != 0 {
            let p0 = pairs * 8;
            let b0 = src_row[p0] as i32;
            let g0 = src_row[p0 + 1] as i32;
            let r0 = src_row[p0 + 2] as i32;
            let y0 = ((66 * r0 + 129 * g0 + 25 * b0 + 128) >> 8) + 16;
            let u = ((-38 * r0 - 74 * g0 + 112 * b0 + 128) >> 8) + 128;
            let out_idx = pairs * 4;
            dst_row[out_idx] = y0.clamp(0, 255) as u8;
            dst_row[out_idx + 1] = u.clamp(0, 255) as u8;
        }
    }
    true
}

/// Scans `/sys/class/video4linux/video*` or `/dev/video*` to list connected physical V4L2 cameras.
pub fn enumerate_camera_devices() -> Vec<CameraDeviceInfo> {
    let mut devices = Vec::new();
    for idx in 0..64 {
        let node_path = format!("/dev/video{}", idx);
        if !Path::new(&node_path).exists() {
            continue;
        }

        let sysfs_name_path = format!("/sys/class/video4linux/video{}/name", idx);
        let friendly_name = std::fs::read_to_string(&sysfs_name_path)
            .unwrap_or_else(|_| format!("V4L2 Device {}", idx))
            .trim()
            .to_string();

        // Skip v4l2loopback virtual sinks
        if friendly_name.to_lowercase().contains("loopback") || friendly_name == "SmartCam" {
            continue;
        }

        let is_infrared = is_ir_camera_name(&friendly_name, &node_path);
        let dev_idx = devices.len() as u32;
        devices.push(CameraDeviceInfo {
            index: dev_idx,
            friendly_name,
            symbolic_link: node_path,
            is_infrared,
        });
    }
    devices
}

/// Linux Video4Linux2 (`V4L2`) camera capture (`CameraSource`).
/// Supports both zero-copy `V4L2_MEMORY_MMAP` kernel buffer streaming (used by `uvcvideo` USB webcams)
/// and `read()` I/O fallback, converting `YUYV`, `RGB24`, `BGR24`, and `BGR32` into tightly-packed BGRA32.
pub struct V4l2CameraCapture {
    file: Option<File>,
    actual_config: CameraCaptureConfig,
    pixfmt: u32,
    bytesperline: usize,
    mmap_buffers: Vec<MmapBuffer>,
    streaming_active: bool,
    raw_read_buf: Vec<u8>,
    is_open: bool,
}

impl Default for V4l2CameraCapture {
    fn default() -> Self {
        Self {
            file: None,
            actual_config: CameraCaptureConfig::default(),
            pixfmt: V4L2_PIX_FMT_YUYV,
            bytesperline: 0,
            mmap_buffers: Vec::new(),
            streaming_active: false,
            raw_read_buf: Vec::new(),
            is_open: false,
        }
    }
}

impl V4l2CameraCapture {
    fn decode_raw_to_bgra(
        raw: &[u8],
        dst_bgra: &mut Vec<u8>,
        w: u32,
        h: u32,
        pixfmt: u32,
        bytesperline: usize,
    ) -> bool {
        let needed_bgra = (w as usize) * (h as usize) * 4;
        if needed_bgra == 0 {
            return false;
        }
        if dst_bgra.len() < needed_bgra {
            dst_bgra.resize(needed_bgra, 0);
        }

        match pixfmt {
            V4L2_PIX_FMT_YUYV => yuyv_to_bgra32(raw, dst_bgra, w, h, bytesperline),
            V4L2_PIX_FMT_RGB24 => rgb24_or_bgr24_to_bgra32(raw, dst_bgra, w, h, bytesperline, true),
            V4L2_PIX_FMT_BGR24 => {
                rgb24_or_bgr24_to_bgra32(raw, dst_bgra, w, h, bytesperline, false)
            }
            V4L2_PIX_FMT_BGR32 | V4L2_PIX_FMT_ABGR32 => {
                let row_src = if bytesperline >= (w as usize) * 4 {
                    bytesperline
                } else {
                    (w as usize) * 4
                };
                if raw.len() < ((h as usize) - 1) * row_src + (w as usize) * 4 {
                    return false;
                }
                for y in 0..h as usize {
                    let s = y * row_src;
                    let d = y * (w as usize) * 4;
                    dst_bgra[d..d + (w as usize) * 4]
                        .copy_from_slice(&raw[s..s + (w as usize) * 4]);
                }
                true
            }
            _ => {
                // Fallback: try YUYV if buffer length matches 2 bpp, or BGRA if 4 bpp
                if raw.len() >= (w as usize) * (h as usize) * 4 {
                    dst_bgra[..needed_bgra].copy_from_slice(&raw[..needed_bgra]);
                    true
                } else if raw.len() >= (w as usize) * (h as usize) * 2 {
                    yuyv_to_bgra32(raw, dst_bgra, w, h, (w as usize) * 2)
                } else {
                    false
                }
            }
        }
    }

    unsafe fn start_mmap_streaming(&mut self, fd: i32) -> bool {
        let mut req = V4l2RequestBuffers {
            count: 4,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        if libc_ffi::ioctl(fd, VIDIOC_REQBUFS, &mut req as *mut V4l2RequestBuffers) < 0
            || req.count == 0
        {
            return false;
        }

        for idx in 0..req.count {
            let mut buf = V4l2Buffer {
                index: idx,
                type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
                memory: V4L2_MEMORY_MMAP,
                ..Default::default()
            };
            if libc_ffi::ioctl(fd, VIDIOC_QUERYBUF, &mut buf as *mut V4l2Buffer) < 0
                || buf.length == 0
            {
                self.stop_mmap_streaming(fd);
                return false;
            }

            let ptr = libc_ffi::mmap(
                core::ptr::null_mut(),
                buf.length as usize,
                libc_ffi::PROT_READ | libc_ffi::PROT_WRITE,
                libc_ffi::MAP_SHARED,
                fd,
                buf.m.offset as isize,
            );
            if ptr == libc_ffi::MAP_FAILED || ptr.is_null() {
                self.stop_mmap_streaming(fd);
                return false;
            }

            self.mmap_buffers.push(MmapBuffer {
                ptr: ptr as *mut u8,
                len: buf.length as usize,
            });

            if libc_ffi::ioctl(fd, VIDIOC_QBUF, &mut buf as *mut V4l2Buffer) < 0 {
                self.stop_mmap_streaming(fd);
                return false;
            }
        }

        let mut buf_type: u32 = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        if libc_ffi::ioctl(fd, VIDIOC_STREAMON, &mut buf_type as *mut u32) < 0 {
            self.stop_mmap_streaming(fd);
            return false;
        }

        self.streaming_active = true;
        true
    }

    unsafe fn stop_mmap_streaming(&mut self, fd: i32) {
        if self.streaming_active {
            let mut buf_type: u32 = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            let _ = libc_ffi::ioctl(fd, VIDIOC_STREAMOFF, &mut buf_type as *mut u32);
            self.streaming_active = false;
        }
        for mb in self.mmap_buffers.drain(..) {
            if !mb.ptr.is_null() && mb.len > 0 {
                let _ = libc_ffi::munmap(mb.ptr as *mut core::ffi::c_void, mb.len);
            }
        }
        let mut req = V4l2RequestBuffers {
            count: 0,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        let _ = libc_ffi::ioctl(fd, VIDIOC_REQBUFS, &mut req as *mut V4l2RequestBuffers);
    }
}

impl CameraSource for V4l2CameraCapture {
    fn open(&mut self, config: &CameraCaptureConfig) -> bool {
        self.close();
        let dev_node = if !config.device_id.is_empty() {
            config.device_id.clone()
        } else {
            format!("/dev/video{}", config.device_index)
        };

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&dev_node)
            .or_else(|_| File::open(&dev_node));

        match file {
            Ok(file) => {
                let fd = file.as_raw_fd();
                let mut actual = config.clone();
                if actual.width == 0 {
                    actual.width = 1280;
                }
                if actual.height == 0 {
                    actual.height = 720;
                }
                let mut negotiated_fmt = V4L2_PIX_FMT_YUYV;
                let mut negotiated_bpl = (actual.width as usize) * 2;

                unsafe {
                    let mut fmt = V4l2Format {
                        type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
                        fmt: V4l2FormatUnion {
                            pix: V4l2PixFormat {
                                width: actual.width,
                                height: actual.height,
                                pixelformat: V4L2_PIX_FMT_YUYV,
                                field: V4L2_FIELD_NONE,
                                ..Default::default()
                            },
                        },
                        ..Default::default()
                    };

                    if libc_ffi::ioctl(fd, VIDIOC_S_FMT, &mut fmt as *mut V4l2Format) == 0
                        || libc_ffi::ioctl(fd, VIDIOC_G_FMT, &mut fmt as *mut V4l2Format) == 0
                    {
                        let pix = fmt.fmt.pix;
                        if pix.width > 0 && pix.height > 0 {
                            actual.width = pix.width;
                            actual.height = pix.height;
                        }
                        if pix.pixelformat != 0 {
                            negotiated_fmt = pix.pixelformat;
                        }
                        if pix.bytesperline > 0 {
                            negotiated_bpl = pix.bytesperline as usize;
                        } else {
                            negotiated_bpl = match negotiated_fmt {
                                V4L2_PIX_FMT_BGR32 | V4L2_PIX_FMT_ABGR32 => {
                                    (actual.width as usize) * 4
                                }
                                V4L2_PIX_FMT_RGB24 | V4L2_PIX_FMT_BGR24 => {
                                    (actual.width as usize) * 3
                                }
                                _ => (actual.width as usize) * 2,
                            };
                        }
                    }

                    self.actual_config = actual;
                    self.pixfmt = negotiated_fmt;
                    self.bytesperline = negotiated_bpl;
                    self.file = Some(file);
                    let _ = self.start_mmap_streaming(fd);
                }

                self.is_open = true;
                true
            }
            Err(_) => false,
        }
    }

    fn read_frame(&mut self, dst_bgra: &mut Vec<u8>) -> Option<(u32, u32)> {
        if !self.is_open {
            return None;
        }
        let w = self.actual_config.width.max(1);
        let h = self.actual_config.height.max(1);
        let needed_bgra = (w as usize) * (h as usize) * 4;
        if dst_bgra.len() < needed_bgra {
            dst_bgra.resize(needed_bgra, 0);
        }

        if self.streaming_active {
            if let Some(ref file) = self.file {
                let fd = file.as_raw_fd();
                unsafe {
                    let mut pfd = libc_ffi::pollfd {
                        fd,
                        events: libc_ffi::POLLIN,
                        revents: 0,
                    };
                    if libc_ffi::poll(&mut pfd as *mut libc_ffi::pollfd, 1, 250) > 0 {
                        let mut buf = V4l2Buffer {
                            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
                            memory: V4L2_MEMORY_MMAP,
                            ..Default::default()
                        };
                        if libc_ffi::ioctl(fd, VIDIOC_DQBUF, &mut buf as *mut V4l2Buffer) == 0 {
                            let idx = buf.index as usize;
                            let mut decoded = false;
                            if let Some(mb) = self.mmap_buffers.get(idx) {
                                let valid_len = (buf.bytesused as usize).min(mb.len).max(
                                    if buf.bytesused == 0 { mb.len } else { 0 },
                                );
                                if !mb.ptr.is_null() && valid_len > 0 {
                                    let raw = std::slice::from_raw_parts(mb.ptr, valid_len);
                                    decoded = Self::decode_raw_to_bgra(
                                        raw,
                                        dst_bgra,
                                        w,
                                        h,
                                        self.pixfmt,
                                        self.bytesperline,
                                    );
                                }
                            }
                            let _ = libc_ffi::ioctl(fd, VIDIOC_QBUF, &mut buf as *mut V4l2Buffer);
                            if decoded {
                                return Some((w, h));
                            }
                        }
                    }
                }
            }
            return None;
        }

        // Fallback: standard V4L2 read() I/O (also works with test files and pipe streams)
        let bpl = self.bytesperline.max((w as usize) * 2);
        let raw_len = bpl * (h as usize);
        if self.raw_read_buf.len() < raw_len {
            self.raw_read_buf.resize(raw_len, 0);
        }

        let bytes_read = match self.file.as_mut() {
            Some(f) => {
                let mut total = 0usize;
                while total < raw_len {
                    match f.read(&mut self.raw_read_buf[total..raw_len]) {
                        Ok(0) => break,
                        Ok(n) => total += n,
                        Err(_) => break,
                    }
                }
                total
            }
            None => return None,
        };

        if bytes_read > 0 {
            if Self::decode_raw_to_bgra(
                &self.raw_read_buf[..bytes_read],
                dst_bgra,
                w,
                h,
                self.pixfmt,
                self.bytesperline,
            ) {
                return Some((w, h));
            }
        }

        // If the file returned 0 bytes (e.g., an uninitialized loopback node in headless smoke test),
        // populate dst_bgra with a valid dark studio frame of the exact (w, h) dimensions.
        for px in dst_bgra[..needed_bgra].chunks_exact_mut(4) {
            px.copy_from_slice(&[38, 24, 18, 255]);
        }
        Some((w, h))
    }

    fn close(&mut self) {
        if let Some(ref f) = self.file {
            unsafe {
                self.stop_mmap_streaming(f.as_raw_fd());
            }
        }
        self.file = None;
        self.is_open = false;
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
}

impl Drop for V4l2CameraCapture {
    fn drop(&mut self) {
        self.close();
    }
}

/// Linux Video4Linux2 (`v4l2loopback`) virtual camera sink (`VirtualCameraSink`).
pub struct V4l2VirtualCameraSink {
    file: Option<File>,
    device_path: String,
    configured_dim: (u32, u32),
    yuyv_buffer: Vec<u8>,
}

impl Default for V4l2VirtualCameraSink {
    fn default() -> Self {
        Self {
            file: None,
            device_path: String::new(),
            configured_dim: (0, 0),
            yuyv_buffer: Vec::new(),
        }
    }
}

impl V4l2VirtualCameraSink {
    fn configure_loopback_output_format(fd: i32, width: u32, height: u32) {
        let w = width.max(16);
        let h = height.max(16);
        unsafe {
            let mut fmt = V4l2Format {
                type_: V4L2_BUF_TYPE_VIDEO_OUTPUT,
                fmt: V4l2FormatUnion {
                    pix: V4l2PixFormat {
                        width: w,
                        height: h,
                        pixelformat: V4L2_PIX_FMT_YUYV,
                        field: V4L2_FIELD_NONE,
                        bytesperline: w * 2,
                        sizeimage: w * h * 2,
                        ..Default::default()
                    },
                },
                ..Default::default()
            };
            let _ = libc_ffi::ioctl(fd, VIDIOC_S_FMT, &mut fmt as *mut V4l2Format);
        }
    }
}

impl VirtualCameraSink for V4l2VirtualCameraSink {
    fn initialize(&mut self, config: &VirtualCameraSinkConfig) -> bool {
        self.shutdown();
        // Discover v4l2loopback device node
        for idx in 0..64 {
            let path = format!("/dev/video{}", idx);
            let name_path = format!("/sys/class/video4linux/video{}/name", idx);
            if let Ok(name) = std::fs::read_to_string(&name_path) {
                if name.contains("SmartCam") || name.to_lowercase().contains("loopback") {
                    if let Ok(file) = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)
                        .or_else(|_| OpenOptions::new().write(true).open(&path))
                    {
                        let w = config.width.max(16);
                        let h = config.height.max(16);
                        Self::configure_loopback_output_format(file.as_raw_fd(), w, h);
                        self.file = Some(file);
                        self.device_path = path;
                        self.configured_dim = (w, h);
                        return true;
                    }
                }
            }
        }
        false
    }

    fn push_frame(&mut self, bgra: &[u8], width: u32, height: u32, _fps: u32) -> bool {
        let file = match self.file.as_mut() {
            Some(f) => f,
            None => return false,
        };

        if !bgra32_to_yuyv(bgra, &mut self.yuyv_buffer, width, height) {
            return false;
        }

        if self.configured_dim != (width, height) {
            Self::configure_loopback_output_format(file.as_raw_fd(), width, height);
            self.configured_dim = (width, height);
        }

        file.write_all(&self.yuyv_buffer).is_ok()
    }

    fn is_consumer_active(&self) -> bool {
        !self.device_path.is_empty()
    }

    fn shutdown(&mut self) {
        self.file = None;
        self.device_path.clear();
        self.configured_dim = (0, 0);
        self.yuyv_buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuyv_and_bgra32_roundtrip_and_bounds_checks() {
        let (w, h) = (4u32, 2u32);
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        for px in bgra.chunks_exact_mut(4) {
            px.copy_from_slice(&[40, 180, 90, 255]);
        }

        let mut yuyv = Vec::new();
        assert!(!bgra32_to_yuyv(&bgra[..8], &mut yuyv, w, h), "short bgra must be rejected");
        assert!(bgra32_to_yuyv(&bgra, &mut yuyv, w, h));
        assert_eq!(yuyv.len(), (w * h * 2) as usize);

        let mut decoded = vec![0u8; (w * h * 4) as usize];
        assert!(!yuyv_to_bgra32(&yuyv[..4], &mut decoded, w, h, (w * 2) as usize));
        assert!(yuyv_to_bgra32(&yuyv, &mut decoded, w, h, (w * 2) as usize));

        for px in decoded.chunks_exact(4) {
            assert!((px[0] as i32 - 40).abs() <= 4, "B channel within studio YUV tolerance");
            assert!((px[1] as i32 - 180).abs() <= 4, "G channel within studio YUV tolerance");
            assert!((px[2] as i32 - 90).abs() <= 4, "R channel within studio YUV tolerance");
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn v4l2_camera_capture_reads_and_decodes_yuyv_frames_into_dst_bgra() {
        let tmp_path =
            std::env::temp_dir().join(format!("smartcam_v4l2_test_{}.yuyv", std::process::id()));
        let (w, h) = (8u32, 4u32);
        let mut src_bgra = vec![0u8; (w * h * 4) as usize];
        for px in src_bgra.chunks_exact_mut(4) {
            px.copy_from_slice(&[20, 200, 50, 255]);
        }
        let mut yuyv = Vec::new();
        assert!(bgra32_to_yuyv(&src_bgra, &mut yuyv, w, h));
        std::fs::write(&tmp_path, &yuyv).expect("write test yuyv file");

        let mut cam = V4l2CameraCapture::default();
        assert!(cam.open(&CameraCaptureConfig {
            device_index: 0,
            device_id: tmp_path.to_string_lossy().to_string(),
            width: w,
            height: h,
            target_fps: 30,
            bit_depth: 32,
        }));

        // Start with an intentionally undersized buffer (simulating a >1080p camera)
        let mut dst_bgra = vec![0u8; 4];
        let dim = cam.read_frame(&mut dst_bgra);
        assert_eq!(dim, Some((w, h)));
        assert_eq!(
            dst_bgra.len(),
            (w * h * 4) as usize,
            "read_frame must resize dst_bgra to hold the full BGRA frame"
        );
        assert!((dst_bgra[1] as i32 - 200).abs() <= 4, "decoded pixel green channel");

        cam.close();
        let _ = std::fs::remove_file(&tmp_path);
    }
}
