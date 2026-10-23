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

use std::fs;
use std::path::{Path, PathBuf};

pub const PEFL_YAML_HEADER: &str = "\
# Personal and Educational Free License (PEF License / PEFL)
# Version 1.0 - 2026-05-26
# Copyright (c) 2026 Rubens Rainelli
#
# Contact Details:
#   Email: rubens@rainelli.it
#   GitHub: https://github.com/RubensRainelli/
#   Tel: (+39) 342 811 5882
#
# Personal and educational free use permitted.
# Corporate, business or enterprise use prohibited without written authorization and fee.
# The full license can be found in the LICENSE file.
";

#[derive(Clone, Debug)]
pub struct CameraInputConfig {
    pub device_index: u32,
    pub device_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bit_depth: u8, // 64 = 64-bit Deep Color (BGRA64 / 16-bit ch), 32 = 32-bit Standard (BGRA32 / 8-bit ch)
}

#[derive(Clone, Debug)]
pub struct FramingOutputConfig {
    pub aspect_ratio: String,
    pub resolution_preset: String, // "1080p", "900p", "720p", "540p", "480p", "360p", "240p"
    pub crop_strategy: String,
    pub output_width: u32,
    pub output_height: u32,
}

#[derive(Clone, Debug)]
pub struct GreenScreenConfig {
    pub hue_min: u8,
    pub hue_max: u8,
    pub sat_min: u8,
    pub sat_max: u8,
    pub val_min: u8,
    pub val_max: u8,
    pub morph_kernel_size: u8,
    pub blur_radius: u8,
    pub spill_suppression: f32,
}

#[derive(Clone, Debug)]
pub struct AiSelfieConfig {
    pub ai_enabled: bool,
    pub model_path: String,
    pub input_width: u32,
    pub input_height: u32,
    pub confidence_threshold: f32,
    pub temporal_smoothing: f32,
    pub engine_mode: i32,       // 0 = Auto/Ibrido, 1 = Sagoma & Volto AI, 2 = Differenza Sfondo (Telo Virtuale)
    pub bg_diff_tolerance: f32, // [0.10 .. 0.95] Tolleranza Luce/LED & Posizione
    pub silhouette_mode: i32,   // 0 = Volto + Busto, 1 = Solo Volto
    pub face_calibrated: bool,
    pub calib_skin_cb: f32,
    pub calib_skin_cr: f32,
    pub calib_head_rx: f32,
    pub calib_head_ry: f32,
    pub calib_landmarks: [(f32, f32); 5], // [0]=Occhio SX, [1]=Occhio DX, [2]=Punta Naso, [3]=Labbra, [4]=Mento
    pub intra_op_threads: u32,
    pub prefer_igpu_directml: bool,
}

#[derive(Clone, Debug)]
pub struct SegmentationConfig {
    pub mode: String, // "passthrough", "green_screen", "ai_selfie"
    pub green_screen: GreenScreenConfig,
    pub ai_selfie: AiSelfieConfig,
}

#[derive(Clone, Debug)]
pub struct CompositorConfig {
    pub compute_backend: String,   // "auto", "cpu", "vulkan", "opencl", "cuda", "rocm"
    pub simd_acceleration: String, // "auto", "avx512", "avx2", "scalar"
    pub background_mode: String,   // "solid_color", "image", "blur"
    pub background_color_rgb: [u8; 3],
    pub background_image_path: String,
    pub background_blur_radius: u32,
    pub background_zoom: f32,
    pub background_pan_x: f32,
    pub background_pan_y: f32,
    pub background_mirror_h: bool, // Mirror virtual background horizontally (for apps that mirror webcam by default)
    pub background_mirror_v: bool, // Mirror virtual background vertically
}

#[derive(Clone, Debug)]
pub struct VirtualCameraConfig {
    pub friendly_name: String,
    pub backend: String,
    pub shared_memory_name: String,
}

#[derive(Clone, Debug)]
pub struct SmartCamConfig {
    pub app_icon_theme: String,  // "cyber_lens", "emerald_cam", "neon_iris"
    pub compute_backend: String, // "auto", "cpu", "vulkan", "opencl", "cuda", "rocm"
    pub ai_enabled: bool,        // true = AI Active, false = AI Disabled (lightweight mode)
    pub enabled_plugins: Vec<String>,
    pub phone_camera_url: String,
    pub phone_camera_port: u16,
    pub camera_input: CameraInputConfig,
    pub framing_output: FramingOutputConfig,
    pub segmentation: SegmentationConfig,
    pub compositor: CompositorConfig,
    pub virtual_camera: VirtualCameraConfig,
}

impl Default for SmartCamConfig {
    fn default() -> Self {
        let mut cfg = Self {
            app_icon_theme: "cyber_lens".to_string(),
            compute_backend: "auto".to_string(),
            ai_enabled: true,
            enabled_plugins: Vec::new(),
            phone_camera_url: "http://192.168.1.100:8080/video".to_string(),
            phone_camera_port: 8080,
            camera_input: CameraInputConfig {
                device_index: 0,
                device_id: String::new(),
                width: 1920,
                height: 1080,
                fps: 30,
                bit_depth: 64,
            },
            framing_output: FramingOutputConfig {
                aspect_ratio: "4:3".to_string(),
                resolution_preset: "1080p".to_string(),
                crop_strategy: "center_crop".to_string(),
                output_width: 1440,
                output_height: 1080,
            },
            segmentation: SegmentationConfig {
                mode: "ai_selfie".to_string(),
                green_screen: GreenScreenConfig {
                    hue_min: 35,
                    hue_max: 85,
                    sat_min: 45,
                    sat_max: 255,
                    val_min: 40,
                    val_max: 255,
                    morph_kernel_size: 3,
                    blur_radius: 2,
                    spill_suppression: 0.35,
                },
                ai_selfie: AiSelfieConfig {
                    ai_enabled: true,
                    model_path: "models/selfie_segmentation.onnx".to_string(),
                    input_width: 256,
                    input_height: 144,
                    confidence_threshold: 0.50,
                    temporal_smoothing: 0.25,
                    engine_mode: 0,
                    bg_diff_tolerance: 0.50,
                    silhouette_mode: 0,
                    face_calibrated: false,
                    calib_skin_cb: 113.0,
                    calib_skin_cr: 148.0,
                    calib_head_rx: 0.105,
                    calib_head_ry: 0.175,
                    calib_landmarks: [
                        (0.45, 0.36),
                        (0.55, 0.36),
                        (0.50, 0.42),
                        (0.50, 0.48),
                        (0.50, 0.54),
                    ],
                    intra_op_threads: 2,
                    prefer_igpu_directml: true,
                },
            },
            compositor: CompositorConfig {
                compute_backend: "auto".to_string(),
                simd_acceleration: "auto".to_string(),
                background_mode: "image".to_string(),
                background_color_rgb: [24, 34, 56],
                background_image_path: "assets/backgrounds/preset_1_studio_blue.jpg".to_string(),
                background_blur_radius: 15,
                background_zoom: 1.0,
                background_pan_x: 0.5,
                background_pan_y: 0.5,
                background_mirror_h: false,
                background_mirror_v: false,
            },
            virtual_camera: VirtualCameraConfig {
                friendly_name: "SmartCam".to_string(),
                backend: "media_foundation".to_string(),
                shared_memory_name: "Local\\SmartCam_MF_SharedFrame_v1".to_string(),
            },
        };
        cfg.update_computed_dimensions();
        cfg
    }
}

pub fn compute_center_crop_rect(src_w: u32, src_h: u32, aspect_w: u32, aspect_h: u32) -> (u32, u32, u32, u32) {
    if src_w == 0 || src_h == 0 || aspect_w == 0 || aspect_h == 0 {
        return (0, 0, src_w, src_h);
    }
    let lhs = (src_w as u64) * (aspect_h as u64);
    let rhs = (src_h as u64) * (aspect_w as u64);
    if lhs > rhs {
        let mut target_w = ((rhs / (aspect_h as u64)) as u32) & !1;
        if target_w == 0 {
            target_w = src_w;
        }
        let x = (src_w.saturating_sub(target_w)) / 2;
        return (x, 0, target_w, src_h);
    }
    if lhs < rhs {
        let mut target_h = ((lhs / (aspect_w as u64)) as u32) & !1;
        if target_h == 0 {
            target_h = src_h;
        }
        let y = (src_h.saturating_sub(target_h)) / 2;
        return (0, y, src_w, target_h);
    }
    (0, 0, src_w, src_h)
}

pub fn resolve_aspect_ratio_pair(aspect_str: &str, src_w: u32, src_h: u32) -> (u32, u32) {
    match aspect_str {
        "16:9" => (16, 9),
        "4:3" => (4, 3),
        "1:1" => (1, 1),
        "9:16" => (9, 16),
        _ => (src_w.max(1), src_h.max(1)),
    }
}

pub fn compute_output_dimensions_for_preset(aspect_ratio: &str, resolution_preset: &str, in_w: u32, in_h: u32) -> (u32, u32) {
    let base_h: u32 = match resolution_preset {
        "1080p" => 1080,
        "900p" => 900,
        "720p" => 720,
        "540p" => 540,
        "480p" => 480,
        "360p" => 360,
        "240p" => 240,
        _ => 1080,
    };

    if aspect_ratio == "native" {
        return (in_w & !1, in_h & !1);
    }

    match (aspect_ratio, base_h) {
        // 16:9
        ("16:9", 1080) => (1920, 1080),
        ("16:9", 900) => (1600, 900),
        ("16:9", 720) => (1280, 720),
        ("16:9", 540) => (960, 540),
        ("16:9", 480) => (854, 480),
        ("16:9", 360) => (640, 360),
        ("16:9", 240) => (426, 240),
        // 4:3
        ("4:3", 1080) => (1440, 1080),
        ("4:3", 900) => (1200, 900),
        ("4:3", 720) => (960, 720),
        ("4:3", 540) => (720, 540),
        ("4:3", 480) => (640, 480),
        ("4:3", 360) => (480, 360),
        ("4:3", 240) => (320, 240),
        // 1:1
        ("1:1", h) => (h & !1, h & !1),
        // 9:16
        ("9:16", 1080) => (608, 1080),
        ("9:16", 900) => (506, 900),
        ("9:16", 720) => (404, 720),
        ("9:16", 540) => (304, 540),
        ("9:16", 480) => (270, 480),
        ("9:16", 360) => (202, 360),
        ("9:16", 240) => (136, 240),
        _ => {
            let (aw, ah) = resolve_aspect_ratio_pair(aspect_ratio, in_w, in_h);
            let w = (((base_h as u64) * (aw as u64) + (ah as u64 / 2)) / (ah.max(1) as u64)) as u32;
            let even_w = ((w + 1) & !1).max(128);
            let even_h = (base_h & !1).max(120);
            (even_w, even_h)
        }
    }
}

impl FramingOutputConfig {
    pub fn compute_output_dimensions_for_preset(aspect_ratio: &str, resolution_preset: &str) -> (u32, u32) {
        compute_output_dimensions_for_preset(aspect_ratio, resolution_preset, 1920, 1080)
    }

    pub fn sync_dimensions_from_preset(&mut self) {
        let (w, h) = Self::compute_output_dimensions_for_preset(&self.aspect_ratio, &self.resolution_preset);
        self.output_width = w;
        self.output_height = h;
    }
}

impl SmartCamConfig {
    pub fn update_computed_dimensions(&mut self) {
        let in_w = self.camera_input.width.clamp(160, 7680);
        let in_h = self.camera_input.height.clamp(120, 4320);
        self.camera_input.width = in_w;
        self.camera_input.height = in_h;
        self.camera_input.fps = self.camera_input.fps.clamp(1, 240);
        if self.camera_input.bit_depth != 32 && self.camera_input.bit_depth != 64 {
            self.camera_input.bit_depth = 64;
        }

        if !matches!(
            self.framing_output.resolution_preset.as_str(),
            "1080p" | "900p" | "720p" | "540p" | "480p" | "360p" | "240p"
        ) {
            self.framing_output.resolution_preset = "1080p".to_string();
        }

        let (ow, oh) = compute_output_dimensions_for_preset(
            &self.framing_output.aspect_ratio,
            &self.framing_output.resolution_preset,
            in_w,
            in_h,
        );
        self.framing_output.output_width = ow;
        self.framing_output.output_height = oh;

        if self.segmentation.green_screen.hue_min > self.segmentation.green_screen.hue_max {
            std::mem::swap(
                &mut self.segmentation.green_screen.hue_min,
                &mut self.segmentation.green_screen.hue_max,
            );
        }
        self.segmentation.green_screen.hue_max = self.segmentation.green_screen.hue_max.min(180);
        self.segmentation.green_screen.hue_min = self.segmentation.green_screen.hue_min.min(self.segmentation.green_screen.hue_max);
        if !matches!(self.segmentation.green_screen.morph_kernel_size, 0 | 3 | 5) {
            self.segmentation.green_screen.morph_kernel_size = 3;
        }
        self.segmentation.green_screen.blur_radius = self.segmentation.green_screen.blur_radius.min(15);
        self.segmentation.green_screen.spill_suppression =
            self.segmentation.green_screen.spill_suppression.clamp(0.0, 1.0);
        self.segmentation.ai_selfie.confidence_threshold =
            self.segmentation.ai_selfie.confidence_threshold.clamp(0.10, 0.90);
        self.segmentation.ai_selfie.temporal_smoothing =
            self.segmentation.ai_selfie.temporal_smoothing.clamp(0.0, 0.90);
        self.segmentation.ai_selfie.engine_mode =
            self.segmentation.ai_selfie.engine_mode.clamp(0, 2);
        self.segmentation.ai_selfie.bg_diff_tolerance =
            self.segmentation.ai_selfie.bg_diff_tolerance.clamp(0.10, 0.95);
        self.segmentation.ai_selfie.silhouette_mode =
            self.segmentation.ai_selfie.silhouette_mode.clamp(0, 1);
        self.segmentation.ai_selfie.calib_skin_cb =
            self.segmentation.ai_selfie.calib_skin_cb.clamp(60.0, 175.0);
        self.segmentation.ai_selfie.calib_skin_cr =
            self.segmentation.ai_selfie.calib_skin_cr.clamp(115.0, 210.0);
        self.segmentation.ai_selfie.calib_head_rx =
            self.segmentation.ai_selfie.calib_head_rx.clamp(0.04, 0.40);
        self.segmentation.ai_selfie.calib_head_ry =
            self.segmentation.ai_selfie.calib_head_ry.clamp(0.06, 0.48);
        self.compositor.background_blur_radius = self.compositor.background_blur_radius.clamp(1, 45);
        self.compositor.background_zoom = self.compositor.background_zoom.clamp(1.0, 4.0);
        self.compositor.background_pan_x = self.compositor.background_pan_x.clamp(0.0, 1.0);
        self.compositor.background_pan_y = self.compositor.background_pan_y.clamp(0.0, 1.0);
        if !matches!(
            self.app_icon_theme.as_str(),
            "cyber_lens" | "emerald_cam" | "neon_iris"
        ) {
            self.app_icon_theme = "cyber_lens".to_string();
        }
        let cb_norm = self.compute_backend.trim().to_ascii_lowercase();
        self.compute_backend = match cb_norm.as_str() {
            "cpu" | "vulkan" | "opencl" | "cuda" | "rocm" => cb_norm,
            _ => "auto".to_string(),
        };
        self.compositor.compute_backend = self.compute_backend.clone();
        self.segmentation.ai_selfie.ai_enabled = self.ai_enabled;
        if self.phone_camera_url.trim().is_empty() {
            self.phone_camera_url = "http://192.168.1.100:8080/video".to_string();
        }
        if self.phone_camera_port < 1024 {
            self.phone_camera_port = 8080;
        }
        if self.virtual_camera.friendly_name.is_empty()
            || self.virtual_camera.friendly_name == "SmartCam Virtual Camera"
        {
            self.virtual_camera.friendly_name = "SmartCam".to_string();
        }
    }

    pub fn is_plugin_enabled(&self, id: &str) -> bool {
        self.enabled_plugins
            .iter()
            .any(|p| p.eq_ignore_ascii_case(id))
    }

    pub fn set_plugin_enabled(&mut self, id: &str, enabled: bool) {
        let id_clean = id.trim().to_string();
        if id_clean.is_empty() {
            return;
        }
        self.enabled_plugins
            .retain(|p| !p.eq_ignore_ascii_case(&id_clean));
        if enabled {
            self.enabled_plugins.push(id_clean);
        }
    }

    #[allow(dead_code)]
    pub fn toggle_plugin(&mut self, id: &str) -> bool {
        let next = !self.is_plugin_enabled(id);
        self.set_plugin_enabled(id, next);
        next
    }

    pub fn load_from_file(path: &Path) -> Self {
        let mut cfg = Self::default();
        let Ok(raw) = fs::read_to_string(path) else {
            return cfg;
        };

        let mut section = String::new();
        let mut subsection = String::new();
        let mut in_enabled_plugins_list = false;

        for raw_line in raw.lines() {
            let line = strip_yaml_comment(raw_line);
            if line.trim().is_empty() {
                continue;
            }
            let indent = line.len() - line.trim_start_matches(' ').len();
            let trimmed = line.trim();

            if in_enabled_plugins_list && trimmed.starts_with("- ") {
                let item = clean_scalar(trimmed[2..].trim());
                if !item.is_empty() && !cfg.is_plugin_enabled(&item) {
                    cfg.enabled_plugins.push(item);
                }
                continue;
            }

            let Some((k, v)) = trimmed.split_once(':') else {
                continue;
            };
            in_enabled_plugins_list = false;
            let key = k.trim();
            let val = clean_scalar(v.trim());

            if indent == 0 {
                match key {
                    "app_icon_theme" if !val.is_empty() => {
                        cfg.app_icon_theme = val;
                        continue;
                    }
                    "compute_backend" if !val.is_empty() => {
                        cfg.compute_backend = val.clone();
                        cfg.compositor.compute_backend = val;
                        continue;
                    }
                    "ai_enabled" if !val.is_empty() => {
                        let b = parse_yaml_bool(&val, true);
                        cfg.ai_enabled = b;
                        cfg.segmentation.ai_selfie.ai_enabled = b;
                        continue;
                    }
                    "phone_camera_url" if !val.is_empty() => {
                        cfg.phone_camera_url = val;
                        continue;
                    }
                    "phone_camera_port" if !val.is_empty() => {
                        if let Ok(p) = val.parse::<u16>() {
                            cfg.phone_camera_port = p.clamp(1024, 65535);
                        }
                        continue;
                    }
                    "enabled_plugins" => {
                        if val.is_empty() {
                            cfg.enabled_plugins.clear();
                            in_enabled_plugins_list = true;
                        } else {
                            cfg.enabled_plugins = parse_string_array(&val);
                        }
                        continue;
                    }
                    _ => {}
                }
                section = key.to_string();
                subsection.clear();
                continue;
            } else if indent == 2 && val.is_empty() {
                if key == "enabled_plugins" {
                    cfg.enabled_plugins.clear();
                    in_enabled_plugins_list = true;
                    continue;
                }
                subsection = key.to_string();
                continue;
            }

            match (section.as_str(), subsection.as_str(), key) {
                ("ui", _, "app_icon_theme") | ("", _, "app_icon_theme") => {
                    cfg.app_icon_theme = val;
                }
                (_, _, "compute_backend") => {
                    cfg.compute_backend = val.clone();
                    cfg.compositor.compute_backend = val;
                }
                (_, _, "ai_enabled") => {
                    let b = parse_yaml_bool(&val, true);
                    cfg.ai_enabled = b;
                    cfg.segmentation.ai_selfie.ai_enabled = b;
                }
                (_, _, "phone_camera_url") => {
                    if !val.is_empty() {
                        cfg.phone_camera_url = val;
                    }
                }
                (_, _, "phone_camera_port") => {
                    if let Ok(p) = val.parse::<u16>() {
                        cfg.phone_camera_port = p.clamp(1024, 65535);
                    }
                }
                (_, _, "enabled_plugins") => {
                    if val.is_empty() {
                        cfg.enabled_plugins.clear();
                        in_enabled_plugins_list = true;
                    } else {
                        cfg.enabled_plugins = parse_string_array(&val);
                    }
                }
                ("camera_input", _, "device_index") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.camera_input.device_index = n;
                    }
                }
                ("camera_input", _, "device_id") => cfg.camera_input.device_id = val,
                ("camera_input", _, "width") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.camera_input.width = n;
                    }
                }
                ("camera_input", _, "height") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.camera_input.height = n;
                    }
                }
                ("camera_input", _, "fps") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.camera_input.fps = n;
                    }
                }
                ("camera_input", _, "bit_depth") | (_, _, "source_bit_depth") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.camera_input.bit_depth = if n == 32 { 32 } else { 64 };
                    }
                }
                ("framing_output", _, "aspect_ratio") => cfg.framing_output.aspect_ratio = val,
                ("framing_output", _, "resolution_preset") => cfg.framing_output.resolution_preset = val,
                ("framing_output", _, "crop_strategy") => cfg.framing_output.crop_strategy = val,
                ("framing_output", _, "output_width") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.framing_output.output_width = n;
                    }
                }
                ("framing_output", _, "output_height") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.framing_output.output_height = n;
                    }
                }
                ("segmentation", "", "mode") => cfg.segmentation.mode = val,
                ("segmentation", "green_screen", "hue_min") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.hue_min = n;
                    }
                }
                ("segmentation", "green_screen", "hue_max") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.hue_max = n;
                    }
                }
                ("segmentation", "green_screen", "sat_min") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.sat_min = n;
                    }
                }
                ("segmentation", "green_screen", "sat_max") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.sat_max = n;
                    }
                }
                ("segmentation", "green_screen", "val_min") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.val_min = n;
                    }
                }
                ("segmentation", "green_screen", "val_max") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.val_max = n;
                    }
                }
                ("segmentation", "green_screen", "morph_kernel_size") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.morph_kernel_size = n;
                    }
                }
                ("segmentation", "green_screen", "blur_radius") => {
                    if let Ok(n) = val.parse::<u8>() {
                        cfg.segmentation.green_screen.blur_radius = n;
                    }
                }
                ("segmentation", "green_screen", "spill_suppression") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.green_screen.spill_suppression = n;
                    }
                }
                ("segmentation", "ai_selfie", "model_path") => cfg.segmentation.ai_selfie.model_path = val,
                ("segmentation", "ai_selfie", "input_width") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.segmentation.ai_selfie.input_width = n;
                    }
                }
                ("segmentation", "ai_selfie", "input_height") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.segmentation.ai_selfie.input_height = n;
                    }
                }
                ("segmentation", "ai_selfie", "confidence_threshold") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.confidence_threshold = n;
                    }
                }
                ("segmentation", "ai_selfie", "temporal_smoothing") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.temporal_smoothing = n;
                    }
                }
                ("segmentation", "ai_selfie", "engine_mode") => {
                    if let Ok(n) = val.parse::<i32>() {
                        cfg.segmentation.ai_selfie.engine_mode = n;
                    }
                }
                ("segmentation", "ai_selfie", "bg_diff_tolerance") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.bg_diff_tolerance = n;
                    }
                }
                ("segmentation", "ai_selfie", "silhouette_mode") => {
                    if let Ok(n) = val.parse::<i32>() {
                        cfg.segmentation.ai_selfie.silhouette_mode = n.clamp(0, 1);
                    }
                }
                ("segmentation", "ai_selfie", "face_calibrated") => {
                    cfg.segmentation.ai_selfie.face_calibrated = parse_yaml_bool(&val, false);
                }
                ("segmentation", "ai_selfie", "calib_skin_cb") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.calib_skin_cb = n;
                    }
                }
                ("segmentation", "ai_selfie", "calib_skin_cr") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.calib_skin_cr = n;
                    }
                }
                ("segmentation", "ai_selfie", "calib_head_rx") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.calib_head_rx = n;
                    }
                }
                ("segmentation", "ai_selfie", "calib_head_ry") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.segmentation.ai_selfie.calib_head_ry = n;
                    }
                }
                ("segmentation", "ai_selfie", "calib_landmarks") => {
                    if let Some(pts) = parse_landmarks_array(&val) {
                        cfg.segmentation.ai_selfie.calib_landmarks = pts;
                    }
                }
                ("segmentation", "ai_selfie", "intra_op_threads") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.segmentation.ai_selfie.intra_op_threads = n;
                    }
                }
                ("segmentation", "ai_selfie", "prefer_igpu_directml") => {
                    cfg.segmentation.ai_selfie.prefer_igpu_directml = val.eq_ignore_ascii_case("true");
                }
                ("compositor", _, "simd_acceleration") => cfg.compositor.simd_acceleration = val,
                ("compositor", _, "background_mode") => cfg.compositor.background_mode = val,
                ("compositor", _, "background_color_rgb") => {
                    if let Some(rgb) = parse_rgb_array(&val) {
                        cfg.compositor.background_color_rgb = rgb;
                    }
                }
                ("compositor", _, "background_image_path") => {
                    if !val.is_empty() {
                        cfg.compositor.background_image_path = val;
                    }
                }
                ("compositor", _, "background_blur_radius") => {
                    if let Ok(n) = val.parse::<u32>() {
                        cfg.compositor.background_blur_radius = n;
                    }
                }
                ("compositor", _, "background_zoom") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.compositor.background_zoom = n;
                    }
                }
                ("compositor", _, "background_pan_x") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.compositor.background_pan_x = n;
                    }
                }
                ("compositor", _, "background_pan_y") => {
                    if let Ok(n) = val.parse::<f32>() {
                        cfg.compositor.background_pan_y = n;
                    }
                }
                ("compositor", _, "background_mirror_h") | (_, _, "background_mirror_h") => {
                    cfg.compositor.background_mirror_h = parse_yaml_bool(&val, false);
                }
                ("compositor", _, "background_mirror_v") | (_, _, "background_mirror_v") => {
                    cfg.compositor.background_mirror_v = parse_yaml_bool(&val, false);
                }
                ("virtual_camera", _, "friendly_name") => cfg.virtual_camera.friendly_name = val,
                ("virtual_camera", _, "backend") => cfg.virtual_camera.backend = val,
                ("virtual_camera", _, "shared_memory_name") => cfg.virtual_camera.shared_memory_name = val,
                _ => {}
            }
        }
        cfg.update_computed_dimensions();
        cfg
    }

    pub fn save_to_file(&mut self, path: &Path) -> Result<(), String> {
        self.update_computed_dimensions();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let shm_escaped = self.virtual_camera.shared_memory_name.replace('\\', "\\\\");
        let bg_img_escaped = self.compositor.background_image_path.replace('\\', "/");
        let rgb = self.compositor.background_color_rgb;
        let plugins_yaml = if self.enabled_plugins.is_empty() {
            "[]".to_string()
        } else {
            let quoted: Vec<String> = self
                .enabled_plugins
                .iter()
                .map(|p| format!("\"{}\"", p.replace('"', "")))
                .collect();
            format!("[{}]", quoted.join(", "))
        };
        let lm = self.segmentation.ai_selfie.calib_landmarks;
        let lm_yaml = format!(
            "[{:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}]",
            lm[0].0, lm[0].1, lm[1].0, lm[1].1, lm[2].0, lm[2].1, lm[3].0, lm[3].1, lm[4].0, lm[4].1
        );
        let yaml = format!(
            "{header}\n\
app_icon_theme: \"{icon_theme}\"       # Options: \"cyber_lens\", \"emerald_cam\", \"neon_iris\"\n\
compute_backend: \"{compute_backend}\" # Options: \"auto\", \"cpu\", \"vulkan\", \"opencl\", \"cuda\", \"rocm\"\n\
ai_enabled: {ai_enabled}               # Options: true (AI Active), false (AI Disabled / Lightweight)\n\
enabled_plugins: {plugins_yaml}\n\
phone_camera_url: \"{phone_url}\"\n\
phone_camera_port: {phone_port}\n\
\n\
camera_input:\n\
\x20 device_index: {dev_idx}\n\
\x20 device_id: \"{dev_id}\"\n\
\x20 width: {in_w}\n\
\x20 height: {in_h}\n\
\x20 fps: {fps}\n\
\x20 bit_depth: {bit_depth}             # 64 = 64-bit Deep Color (BGRA64 / 16-bit ch), 32 = 32-bit Standard (BGRA32 / 8-bit ch)\n\
\n\
framing_output:\n\
\x20 aspect_ratio: \"{ar}\"           # Options: \"native\", \"16:9\", \"4:3\", \"1:1\", \"9:16\", \"custom\"\n\
\x20 resolution_preset: \"{res_preset}\"  # Options: \"1080p\", \"900p\", \"720p\", \"540p\", \"480p\", \"360p\", \"240p\"\n\
\x20 crop_strategy: \"{cs}\"  # Options: \"center_crop\", \"letterbox\"\n\
\x20 output_width: {out_w}\n\
\x20 output_height: {out_h}\n\
\n\
segmentation:\n\
\x20 mode: \"{mode}\"          # Options: \"passthrough\", \"green_screen\", \"ai_selfie\"\n\
\n\
\x20 green_screen:\n\
\x20   hue_min: {hmin}                 # HSV Hue lower bound [0..180]\n\
\x20   hue_max: {hmax}                 # HSV Hue upper bound [0..180]\n\
\x20   sat_min: {smin}                 # HSV Saturation lower bound [0..255]\n\
\x20   sat_max: {smax}                # HSV Saturation upper bound [0..255]\n\
\x20   val_min: {vmin}                 # HSV Value lower bound [0..255]\n\
\x20   val_max: {vmax}                # HSV Value upper bound [0..255]\n\
\x20   morph_kernel_size: {morph}        # Morphological cleanup kernel (0 = off, 3 or 5)\n\
\x20   blur_radius: {blur}              # Soft-edge feathering radius in pixels\n\
\x20   spill_suppression: {spill:.2}     # Green halo suppression on subject edges [0.0..1.0]\n\
\n\
\x20 ai_selfie:\n\
\x20   ai_enabled: {ai_enabled}\n\
\x20   model_path: \"{model}\"\n\
\x20   input_width: {ai_w}\n\
\x20   input_height: {ai_h}\n\
\x20   confidence_threshold: {ai_conf:.2}\n\
\x20   temporal_smoothing: {ai_smooth:.2}\n\
\x20   engine_mode: {ai_mode}            # 0 = Auto/Ibrido, 1 = Sagoma & Volto AI, 2 = Differenza Sfondo (Telo Virtuale)\n\
\x20   bg_diff_tolerance: {ai_tol:.2}      # Tolleranza Luce/LED & Posizione [0.10..0.95]\n\
\x20   silhouette_mode: {sil_mode}        # 0 = Volto + Busto, 1 = Solo Volto\n\
\x20   face_calibrated: {face_cal}\n\
\x20   calib_skin_cb: {cal_cb:.1}\n\
\x20   calib_skin_cr: {cal_cr:.1}\n\
\x20   calib_head_rx: {cal_rx:.3}\n\
\x20   calib_head_ry: {cal_ry:.3}\n\
\x20   calib_landmarks: {lm_yaml}\n\
\x20   intra_op_threads: {ai_threads}\n\
\x20   prefer_igpu_directml: {ai_igpu}\n\
\n\
compositor:\n\
\x20 compute_backend: \"{compute_backend}\" # Options: \"auto\", \"cpu\", \"vulkan\", \"opencl\", \"cuda\", \"rocm\"\n\
\x20 simd_acceleration: \"{simd}\"     # Options: \"auto\", \"avx512\", \"avx2\", \"scalar\"\n\
\x20 background_mode: \"{bg_mode}\" # Options: \"solid_color\", \"image\", \"blur\"\n\
\x20 background_color_rgb: [{r}, {g}, {b}]\n\
\x20 background_image_path: \"{bg_img}\"\n\
\x20 background_blur_radius: {bg_blur}\n\
\x20 background_zoom: {bg_zoom:.2}\n\
\x20 background_pan_x: {bg_px:.3}\n\
\x20 background_pan_y: {bg_py:.3}\n\
\x20 background_mirror_h: {bg_mir_h}       # Specchia lo sfondo in orizzontale (utile per app che specchiano la webcam)\n\
\x20 background_mirror_v: {bg_mir_v}       # Specchia lo sfondo in verticale\n\
\n\
virtual_camera:\n\
\x20 friendly_name: \"{vcam_name}\"\n\
\x20 backend: \"{vcam_backend}\"   # Windows 11 native Media Foundation Virtual Camera\n\
\x20 shared_memory_name: \"{shm}\"\n",
            header = PEFL_YAML_HEADER.trim_end(),
            icon_theme = self.app_icon_theme,
            compute_backend = self.compute_backend,
            ai_enabled = if self.ai_enabled { "true" } else { "false" },
            plugins_yaml = plugins_yaml,
            phone_url = self.phone_camera_url.replace('\\', "/"),
            phone_port = self.phone_camera_port,
            dev_idx = self.camera_input.device_index,
            dev_id = self.camera_input.device_id,
            in_w = self.camera_input.width,
            in_h = self.camera_input.height,
            fps = self.camera_input.fps,
            bit_depth = self.camera_input.bit_depth,
            ar = self.framing_output.aspect_ratio,
            res_preset = self.framing_output.resolution_preset,
            cs = self.framing_output.crop_strategy,
            out_w = self.framing_output.output_width,
            out_h = self.framing_output.output_height,
            mode = self.segmentation.mode,
            hmin = self.segmentation.green_screen.hue_min,
            hmax = self.segmentation.green_screen.hue_max,
            smin = self.segmentation.green_screen.sat_min,
            smax = self.segmentation.green_screen.sat_max,
            vmin = self.segmentation.green_screen.val_min,
            vmax = self.segmentation.green_screen.val_max,
            morph = self.segmentation.green_screen.morph_kernel_size,
            blur = self.segmentation.green_screen.blur_radius,
            spill = self.segmentation.green_screen.spill_suppression,
            model = self.segmentation.ai_selfie.model_path,
            ai_w = self.segmentation.ai_selfie.input_width,
            ai_h = self.segmentation.ai_selfie.input_height,
            ai_conf = self.segmentation.ai_selfie.confidence_threshold,
            ai_smooth = self.segmentation.ai_selfie.temporal_smoothing,
            ai_mode = self.segmentation.ai_selfie.engine_mode,
            ai_tol = self.segmentation.ai_selfie.bg_diff_tolerance,
            sil_mode = self.segmentation.ai_selfie.silhouette_mode,
            face_cal = if self.segmentation.ai_selfie.face_calibrated { "true" } else { "false" },
            cal_cb = self.segmentation.ai_selfie.calib_skin_cb,
            cal_cr = self.segmentation.ai_selfie.calib_skin_cr,
            cal_rx = self.segmentation.ai_selfie.calib_head_rx,
            cal_ry = self.segmentation.ai_selfie.calib_head_ry,
            lm_yaml = lm_yaml,
            ai_threads = self.segmentation.ai_selfie.intra_op_threads,
            ai_igpu = if self.segmentation.ai_selfie.prefer_igpu_directml { "true" } else { "false" },
            simd = self.compositor.simd_acceleration,
            bg_mode = self.compositor.background_mode,
            r = rgb[0],
            g = rgb[1],
            b = rgb[2],
            bg_img = bg_img_escaped,
            bg_blur = self.compositor.background_blur_radius,
            bg_zoom = self.compositor.background_zoom,
            bg_px = self.compositor.background_pan_x,
            bg_py = self.compositor.background_pan_y,
            bg_mir_h = if self.compositor.background_mirror_h { "true" } else { "false" },
            bg_mir_v = if self.compositor.background_mirror_v { "true" } else { "false" },
            vcam_name = self.virtual_camera.friendly_name,
            vcam_backend = self.virtual_camera.backend,
            shm = shm_escaped,
        );

        fs::write(path, yaml).map_err(|e| format!("Impossibile salvare {}: {}", path.display(), e))
    }
}

fn parse_yaml_bool(val: &str, default_val: bool) -> bool {
    match val.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => true,
        "false" | "off" | "no" | "0" => false,
        _ => default_val,
    }
}

fn parse_string_array(val: &str) -> Vec<String> {
    let v = val.trim();
    if v == "[]" || v.is_empty() {
        return Vec::new();
    }
    let inner = if v.starts_with('[') && v.ends_with(']') {
        &v[1..v.len() - 1]
    } else {
        v
    };
    inner
        .split(',')
        .map(|s| clean_scalar(s.trim()))
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_yaml_comment(line: &str) -> String {
    let mut in_quote = false;
    let mut quote_ch = '\0';
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        if ch == '"' || ch == '\'' {
            if !in_quote {
                in_quote = true;
                quote_ch = ch;
            } else if quote_ch == ch {
                in_quote = false;
            }
        }
        if ch == '#' && !in_quote {
            break;
        }
        out.push(ch);
    }
    out
}

fn clean_scalar(val: &str) -> String {
    let v = val.trim();
    if (v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')) {
        v[1..v.len() - 1].replace("\\\\", "\\")
    } else {
        v.to_string()
    }
}

fn parse_rgb_array(val: &str) -> Option<[u8; 3]> {
    let v = val.trim();
    if !v.starts_with('[') || !v.ends_with(']') {
        return None;
    }
    let parts: Vec<&str> = v[1..v.len() - 1].split(',').map(|s| s.trim()).collect();
    if parts.len() != 3 {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn parse_landmarks_array(val: &str) -> Option<[(f32, f32); 5]> {
    let v = val.trim();
    if !v.starts_with('[') || !v.ends_with(']') {
        return None;
    }
    let parts: Vec<&str> = v[1..v.len() - 1].split(',').map(|s| s.trim()).collect();
    if parts.len() != 10 {
        return None;
    }
    let mut out = [(0.5f32, 0.5f32); 5];
    for i in 0..5 {
        let px: f32 = parts[i * 2].parse().ok()?;
        let py: f32 = parts[i * 2 + 1].parse().ok()?;
        out[i] = (px.clamp(0.0, 1.0), py.clamp(0.0, 1.0));
    }
    Some(out)
}

pub fn resolve_repo_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("config").join("default_config.yaml").is_file() {
                return dir.to_path_buf();
            }
            if let Some(parent) = dir.parent() {
                if parent.join("config").join("default_config.yaml").is_file() {
                    return parent.to_path_buf();
                }
                if let Some(gp) = parent.parent() {
                    if gp.join("config").join("default_config.yaml").is_file() {
                        return gp.to_path_buf();
                    }
                    if let Some(ggp) = gp.parent() {
                        if ggp.join("config").join("default_config.yaml").is_file() {
                            return ggp.to_path_buf();
                        }
                    }
                }
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_yaml_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("smartcam_config_test_{}_{}.yaml", name, std::process::id()))
    }

    #[test]
    fn save_then_load_round_trips_nested_settings() {
        let mut cfg = SmartCamConfig::default();
        cfg.camera_input.device_index = 3;
        cfg.camera_input.fps = 60;
        cfg.framing_output.aspect_ratio = "16:9".to_string();
        cfg.framing_output.crop_strategy = "letterbox".to_string();
        cfg.segmentation.mode = "green_screen".to_string();
        cfg.segmentation.green_screen.hue_min = 50;
        cfg.segmentation.green_screen.blur_radius = 7;
        cfg.segmentation.ai_selfie.engine_mode = 2;
        cfg.segmentation.ai_selfie.silhouette_mode = 1;
        cfg.compositor.background_mode = "blur".to_string();
        cfg.compositor.background_blur_radius = 30;
        cfg.compositor.background_mirror_h = true;
        cfg.virtual_camera.friendly_name = "Studio Cam".to_string();

        let path = temp_yaml_path("round_trip");
        cfg.save_to_file(&path).expect("save_to_file");
        let loaded = SmartCamConfig::load_from_file(&path);
        let _ = fs::remove_file(&path);

        assert_eq!(loaded.camera_input.device_index, 3);
        assert_eq!(loaded.camera_input.fps, 60);
        assert_eq!(loaded.framing_output.aspect_ratio, "16:9");
        assert_eq!(loaded.framing_output.crop_strategy, "letterbox");
        assert_eq!(loaded.segmentation.mode, "green_screen");
        assert_eq!(loaded.segmentation.green_screen.hue_min, 50);
        assert_eq!(loaded.segmentation.green_screen.blur_radius, 7);
        assert_eq!(loaded.segmentation.ai_selfie.engine_mode, 2);
        assert_eq!(loaded.segmentation.ai_selfie.silhouette_mode, 1);
        assert_eq!(loaded.compositor.background_mode, "blur");
        assert_eq!(loaded.compositor.background_blur_radius, 30);
        assert!(loaded.compositor.background_mirror_h);
        assert_eq!(loaded.virtual_camera.friendly_name, "Studio Cam");
    }

    #[test]
    fn saved_yaml_keeps_section_indentation() {
        let mut cfg = SmartCamConfig::default();
        let path = temp_yaml_path("indentation");
        cfg.save_to_file(&path).expect("save_to_file");
        let yaml = fs::read_to_string(&path).expect("read saved yaml");
        let _ = fs::remove_file(&path);

        assert!(yaml.contains("\ncamera_input:\n  device_index: 0\n"), "{yaml}");
        assert!(yaml.contains("\nsegmentation:\n  mode: \"ai_selfie\""), "{yaml}");
        assert!(yaml.contains("\n  green_screen:\n    hue_min: 35 "), "{yaml}");
        assert!(yaml.contains("\n  ai_selfie:\n    ai_enabled: true\n"), "{yaml}");
        assert!(yaml.contains("\ncompositor:\n  compute_backend: \"auto\""), "{yaml}");
        assert!(yaml.contains("\nvirtual_camera:\n  friendly_name: \"SmartCam\"\n"), "{yaml}");
    }
}
