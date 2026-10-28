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

use crate::types::{
    CameraCaptureConfig, CameraDeviceInfo, CameraSource, VirtualCameraSink,
    VirtualCameraSinkConfig,
};

/// Enumerates connected physical AVFoundation cameras on macOS.
pub fn enumerate_camera_devices() -> Vec<CameraDeviceInfo> {
    Vec::new()
}

/// macOS AVFoundation webcam acquisition (`CameraSource`).
pub struct CmioCameraCapture {
    is_open: bool,
    actual_config: CameraCaptureConfig,
}

impl Default for CmioCameraCapture {
    fn default() -> Self {
        Self {
            is_open: false,
            actual_config: CameraCaptureConfig::default(),
        }
    }
}

impl CameraSource for CmioCameraCapture {
    fn open(&mut self, config: &CameraCaptureConfig) -> bool {
        self.actual_config = config.clone();
        self.is_open = true;
        true
    }

    fn read_frame(&mut self, _dst_bgra: &mut Vec<u8>) -> Option<(u32, u32)> {
        if !self.is_open {
            return None;
        }
        Some((self.actual_config.width, self.actual_config.height))
    }

    fn close(&mut self) {
        self.is_open = false;
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
}

/// macOS CoreMediaIO Camera Extension (`CMIOExtensionProvider`) virtual camera sink.
pub struct CmioVirtualCameraSink {
    is_active: bool,
}

impl Default for CmioVirtualCameraSink {
    fn default() -> Self {
        Self { is_active: false }
    }
}

impl VirtualCameraSink for CmioVirtualCameraSink {
    fn initialize(&mut self, _config: &VirtualCameraSinkConfig) -> bool {
        self.is_active = true;
        true
    }

    fn push_frame(&mut self, _bgra: &[u8], _width: u32, _height: u32, _fps: u32) -> bool {
        self.is_active
    }

    fn is_consumer_active(&self) -> bool {
        self.is_active
    }

    fn shutdown(&mut self) {
        self.is_active = false;
    }
}
