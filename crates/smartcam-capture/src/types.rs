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

use crate::header::DEFAULT_SHARED_MAPPING_NAME;

/// Describes an enumerated physical camera device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDeviceInfo {
    pub index: u32,
    pub friendly_name: String,
    pub symbolic_link: String,
    pub is_infrared: bool,
}

impl CameraDeviceInfo {
    pub fn display_label(&self) -> String {
        if self.is_infrared {
            if self.friendly_name.contains("[Sensore IR]") {
                format!("[{}] {}", self.index, self.friendly_name)
            } else {
                format!("[{}] {} [Sensore IR]", self.index, self.friendly_name)
            }
        } else {
            format!("[{}] {}", self.index, self.friendly_name)
        }
    }
}

/// Configuration for opening a physical camera device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraCaptureConfig {
    pub device_index: u32,
    pub device_id: String,
    pub width: u32,
    pub height: u32,
    pub target_fps: u32,
    pub bit_depth: u8,
}

impl Default for CameraCaptureConfig {
    fn default() -> Self {
        Self {
            device_index: 0,
            device_id: String::new(),
            width: 1920,
            height: 1080,
            target_fps: 30,
            bit_depth: 32,
        }
    }
}

/// Configuration for initializing the OS Virtual Camera Sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualCameraSinkConfig {
    pub friendly_name: String,
    pub shared_memory_name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for VirtualCameraSinkConfig {
    fn default() -> Self {
        Self {
            friendly_name: "SmartCam".to_string(),
            shared_memory_name: DEFAULT_SHARED_MAPPING_NAME.to_string(),
            width: 1440,
            height: 1080,
            fps: 30,
        }
    }
}

/// Cross-platform trait for physical webcam acquisition.
pub trait CameraSource: Send {
    fn open(&mut self, config: &CameraCaptureConfig) -> bool;
    fn read_frame(&mut self, dst_bgra: &mut Vec<u8>) -> Option<(u32, u32)>;
    fn close(&mut self);
    fn is_open(&self) -> bool;
}

/// Cross-platform trait for Virtual Camera frame output & IPC sink.
pub trait VirtualCameraSink: Send {
    fn initialize(&mut self, config: &VirtualCameraSinkConfig) -> bool;
    fn push_frame(&mut self, bgra: &[u8], width: u32, height: u32, fps: u32) -> bool;
    fn is_consumer_active(&self) -> bool;
    fn shutdown(&mut self);
}
