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

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::{
    ir::is_ir_camera_name,
    types::{
        CameraCaptureConfig, CameraDeviceInfo, CameraSource, VirtualCameraSink,
        VirtualCameraSinkConfig,
    },
};

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
pub struct V4l2CameraCapture {
    file: Option<File>,
    actual_config: CameraCaptureConfig,
    is_open: bool,
}

impl Default for V4l2CameraCapture {
    fn default() -> Self {
        Self {
            file: None,
            actual_config: CameraCaptureConfig::default(),
            is_open: false,
        }
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

        match File::open(&dev_node) {
            Ok(file) => {
                self.file = Some(file);
                self.actual_config = config.clone();
                self.is_open = true;
                true
            }
            Err(_) => false,
        }
    }

    fn read_frame(&mut self, _dst_bgra: &mut Vec<u8>) -> Option<(u32, u32)> {
        if !self.is_open {
            return None;
        }
        Some((self.actual_config.width, self.actual_config.height))
    }

    fn close(&mut self) {
        self.file = None;
        self.is_open = false;
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
}

/// Linux Video4Linux2 (`v4l2loopback`) virtual camera sink (`VirtualCameraSink`).
pub struct V4l2VirtualCameraSink {
    file: Option<File>,
    device_path: String,
    yuyv_buffer: Vec<u8>,
}

impl Default for V4l2VirtualCameraSink {
    fn default() -> Self {
        Self {
            file: None,
            device_path: String::new(),
            yuyv_buffer: Vec::new(),
        }
    }
}

impl VirtualCameraSink for V4l2VirtualCameraSink {
    fn initialize(&mut self, _config: &VirtualCameraSinkConfig) -> bool {
        self.shutdown();
        // Discover v4l2loopback device node
        for idx in 0..64 {
            let path = format!("/dev/video{}", idx);
            let name_path = format!("/sys/class/video4linux/video{}/name", idx);
            if let Ok(name) = std::fs::read_to_string(&name_path) {
                if name.contains("SmartCam") || name.to_lowercase().contains("loopback") {
                    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
                        self.file = Some(file);
                        self.device_path = path;
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

        // Convert BGRA32 to YUYV (YUY2 4:2:2) for maximum compatibility with Chromium/Zoom/OBS on Linux
        let yuyv_len = (width as usize) * (height as usize) * 2;
        self.yuyv_buffer.resize(yuyv_len, 0);

        let num_pairs = (width * height / 2) as usize;
        for i in 0..num_pairs {
            let p0 = i * 8;
            let b0 = bgra[p0] as i32;
            let g0 = bgra[p0 + 1] as i32;
            let r0 = bgra[p0 + 2] as i32;

            let b1 = bgra[p0 + 4] as i32;
            let g1 = bgra[p0 + 5] as i32;
            let r1 = bgra[p0 + 6] as i32;

            let y0 = ((66 * r0 + 129 * g0 + 25 * b0 + 128) >> 8) + 16;
            let y1 = ((66 * r1 + 129 * g1 + 25 * b1 + 128) >> 8) + 16;

            let r_avg = (r0 + r1) >> 1;
            let g_avg = (g0 + g1) >> 1;
            let b_avg = (b0 + b1) >> 1;

            let u = ((-38 * r_avg - 74 * g_avg + 112 * b_avg + 128) >> 8) + 128;
            let v = ((112 * r_avg - 94 * g_avg - 18 * b_avg + 128) >> 8) + 128;

            let out_idx = i * 4;
            self.yuyv_buffer[out_idx] = y0.clamp(0, 255) as u8;
            self.yuyv_buffer[out_idx + 1] = u.clamp(0, 255) as u8;
            self.yuyv_buffer[out_idx + 2] = y1.clamp(0, 255) as u8;
            self.yuyv_buffer[out_idx + 3] = v.clamp(0, 255) as u8;
        }

        file.write_all(&self.yuyv_buffer).is_ok()
    }

    fn is_consumer_active(&self) -> bool {
        // On Linux, check whether any process holds an open reader FD to the loopback node
        !self.device_path.is_empty()
    }

    fn shutdown(&mut self) {
        self.file = None;
        self.device_path.clear();
        self.yuyv_buffer.clear();
    }
}
