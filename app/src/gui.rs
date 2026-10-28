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

#![allow(dead_code)]

use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::benchmark::{
    cpu_supports_avx2, cpu_supports_avx512 as bench_cpu_supports_avx512,
    load_or_run_benchmark_on_startup, run_hardware_benchmark_suite, HardwareBenchmarkReport,
};
use crate::config::SmartCamConfig;
use crate::diagnostics::*;
use crate::engine_bridge::*;
use crate::pefl_guard::{is_pefl_accepted, save_pefl_acceptance};
use crate::plugins::{PluginRegistry, PLUGIN_ID_MULTI_PERSON, PLUGIN_ID_PHONE_CAMERA};
use crate::win32_ffi::*;

const TIMER_FRAME_ID: usize = 1;
const TRAY_ICON_ID: UINT = 1;

// Tray Menu Command IDs
const CMD_TRAY_OPEN: usize = 2001;
const CMD_TRAY_MODE_ORIG: usize = 2002;
const CMD_TRAY_MODE_BLUR: usize = 2003;
const CMD_TRAY_MODE_AI: usize = 2004;
const CMD_TRAY_MODE_GREEN: usize = 2005;
const CMD_TRAY_AUTOSTART: usize = 2006;
const CMD_TRAY_EXIT: usize = 2007;
const CMD_TRAY_PEFL: usize = 2008;
const CMD_TRAY_ICON_CYBER: usize = 2010;
const CMD_TRAY_ICON_EMERALD: usize = 2011;
const CMD_TRAY_ICON_NEON: usize = 2012;
const CMD_TRAY_PLUGINS: usize = 2015;
const CMD_TRAY_BENCHMARK: usize = 2016;
const CMD_TRAY_RUN_BENCHMARK_NOW: usize = 2017;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DropdownKind {
    AspectRatio,
    OutputResolution,
    OutputFps,
    SegmentationEngine,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum UiAction {
    // Top Header & Navigation Actions
    PrevCamera,
    NextCamera,
    RescanCameras,
    QuickTogglePhoneCamera,
    OpenPluginsModal,
    ClosePluginsModal,
    OpenPerfModal,
    ClosePerfModal,
    OpenPeflModal,
    OpenWizard,
    ToggleAdvanced,
    CloseAdvancedModal,
    MinimizeToTray,
    // Plugin Manager Modal Actions
    TogglePluginByIndex(usize),
    SetPhonePreset(u8),
    FocusPhonePortInput,
    ApplyManualPhonePort,
    SetPhoneServerPortPreset(u16),
    UnlockPhoneFirewall,
    ActivatePhoneUsbOrHotspotBridge,
    PastePhoneUrlFromClipboard,
    RescanPluginsDir,
    OpenPluginGuideDoc,
    // Guided 5-Point Face Calibration & Silhouette Mode Actions
    OpenFaceCalibModal,
    CloseFaceCalibModal,
    RetakeFaceCalibSnapshot,
    UndoFaceCalibPoint,
    ResetFaceCalibration,
    SetSilhouetteMode(i32), // 0 = Volto + Busto, 1 = Solo Volto
    ToggleSilhouetteMode,
    QuickToggleMultiPerson,
    // Performance, GPU/CPU, AVX, AI & Benchmark Modal Actions
    SelectComputeBackend(u8), // 0=Auto, 1=CPU, 2=Vulkan, 3=OpenCL, 4=CUDA, 5=ROCm
    SelectSimdMode(u8),       // 0=Auto, 1=AVX512, 2=AVX2, 3=Scalar
    SetAiEnabled(bool),
    RunHardwareBenchmarkNow,
    ApplyRecommendedBenchmark,
    ApplyBenchmarkProfile(usize),
    // PEFL v1.0 Mandatory Clause 3 Foreground Modal Actions
    PeflToggleRemember,
    PeflOpenFullLicense,
    PeflAccept,
    PeflReject,
    // Wizard Actions
    WizardNext,
    WizardPrev,
    WizardClose,
    WizardSelectEffect(u8),
    // Consumer Background Style Presets
    StyleOriginal,
    StyleBlur,
    StyleAiOffice,
    StyleGreenScreen,
    // Background Fill Sub-Mode & Unified Gallery / Crop Modal
    BgModeImage,
    BgModeBlur,
    BgModeSolidColor,
    SelectStudioPreset(u8),
    OpenCustomBgImageDialog,
    OpenBgCropModal,
    CropZoomOut,
    CropZoomIn,
    CropReset,
    CropApplyAndClose,
    ToggleBgMirrorH,
    ToggleBgMirrorV,
    ToggleSourceBitDepth,
    // Reference Background Capture (3s Real Background Capture / Virtual Green Screen)
    StartReferenceBgCapture,
    ClearReferenceBg,
    // App & Tray Icon Theme Switcher
    SetIconTheme(u8),
    // Interactive Dropdowns (Aspect Ratio, Output Resolution, Output FPS, Segmentation Engine)
    ToggleDropdown(DropdownKind),
    SelectAspectRatio(u8),      // 0 = "16:9", 1 = "4:3", 2 = "1:1", 3 = "9:16"
    SelectResolutionPreset(u8), // 0 = "1080p", 1 = "900p", 2 = "720p", 3 = "540p", 4 = "480p", 5 = "360p", 6 = "240p"
    SelectOutputFps(u32),       // 60, 30, 24, 15, 10
    SelectAiEngineMode(i32), // 0 = Auto/Ibrido, 1 = Sagoma & Volto AI, 2 = Differenza Sfondo (Telo Virtuale)
    AiBgTolDec,
    AiBgTolInc,
    // Consumer Aspect Ratio Presets (kept as aliases)
    Aspect16x9,
    Aspect4x3,
    Aspect1x1,
    Aspect9x16,
    // Green Screen 1-Click Calibration
    AutoCalibrate1Click,
    // Windows 11 Assisted Troubleshooting & Driver Actions
    UnlockWin11Privacy,
    InstallRepairDriver,
    OpenSideBySideWeb,
    ToggleAutostart,
    // Advanced Settings Adjustments
    HueMinDec,
    HueMinInc,
    HueMaxDec,
    HueMaxInc,
    SatMinDec,
    SatMinInc,
    SatMaxDec,
    SatMaxInc,
    ValMinDec,
    ValMinInc,
    ValMaxDec,
    ValMaxInc,
    MorphCycle,
    BlurDec,
    BlurInc,
    SpillDec,
    SpillInc,
    AiConfDec,
    AiConfInc,
    AiSmoothDec,
    AiSmoothInc,
    BgColorPresetCycle,
    BgBlurDec,
    BgBlurInc,
    SimdVariantCycle,
    SaveConfigYaml,
}

pub(crate) struct HitZone {
    rect: RECT,
    action: UiAction,
}

pub struct AppGuiState {
    pub repo_root: PathBuf,
    pub config_path: PathBuf,
    pub cfg: SmartCamConfig,
    pub diag: SystemDiagnostics,
    pub pipeline: SmartCamPipelineInstance,
    pub plugin_registry: PluginRegistry,
    pub benchmark_report: HardwareBenchmarkReport,
    pub raw_capture_buf: Vec<u8>,
    pub preview_buf: Vec<u8>,
    pub raw_w: u32,
    pub raw_h: u32,
    pub out_w: u32,
    pub out_h: u32,
    pub frame_counter: u64,
    pub fps_window_start: Instant,
    pub fps_window_frames: u32,
    pub current_fps: f32,
    pub show_advanced: bool,
    pub plugins_modal_open: bool,
    pub phone_port_input_focused: bool,
    pub phone_port_fresh_focus: bool,
    pub phone_port_input_text: String,
    pub perf_modal_open: bool,
    pub face_calib_modal_open: bool,
    pub face_calib_snapshot_buf: Vec<u8>,
    pub face_calib_snap_w: u32,
    pub face_calib_snap_h: u32,
    pub face_calib_step: usize, // 0..4 = 5 points to click, 5 = completed & calibrated
    pub face_calib_points: [(f32, f32); 5],
    pub face_calib_photo_rect: RECT,
    pub wizard_step: u8, // 0 = hidden, 1..=3 = active wizard step
    pub pefl_modal_open: bool,
    pub pefl_accepted_session: bool,
    pub pefl_remember_7_days: bool,
    pub minimized_to_tray: bool,
    pub on_demand_sleeping: bool,
    pub status_message: String,
    pub video_preview_rect: RECT,
    pub hit_zones: Vec<HitZone>,
    pub hover_pos: POINT,
    pub tray_balloon_shown: bool,
    // 3-second Reference Background Capture Countdown
    pub bg_capture_countdown_end: Option<Instant>,
    // Unified Background Gallery & Social-Style Interactive Crop & Zoom Modal
    pub crop_modal_open: bool,
    pub crop_dragging: bool,
    pub crop_drag_start_mouse: POINT,
    pub crop_drag_start_pan: (f32, f32),
    pub crop_canvas_rect: RECT,
    pub crop_slider_rect: RECT,
    // Native GDI Floating Dropdown Menu State
    pub open_dropdown: Option<DropdownKind>,
    pub dropdown_trigger_rects: [(DropdownKind, RECT); 4],
    pub dropdown_overlay_zones: Vec<HitZone>,
    pub dropdown_menu_rect: RECT,
    // Active loaded HICONs & cached GDI double-buffer resources
    pub current_hicon_big: HICON,
    pub current_hicon_small: HICON,
    pub backbuf_dc: HDC,
    pub backbuf_bmp: HBITMAP,
    pub backbuf_old_bmp: HGDIOBJ,
    pub backbuf_w: i32,
    pub backbuf_h: i32,
    pub font_title: HFONT,
    pub font_sub: HFONT,
    pub font_btn: HFONT,
    pub font_small: HFONT,
    pub crop_modal_scratch: Vec<u8>,
}

static mut GLOBAL_APP: *mut AppGuiState = std::ptr::null_mut();

impl AppGuiState {
    pub fn new(repo_root: PathBuf, config_path: PathBuf, start_minimized: bool) -> Self {
        let mut cfg = SmartCamConfig::load_from_file(&config_path);
        let mut diag = collect_diagnostics(&repo_root, true);

        // Select first non-IR webcam by default if current index is IR or out of range
        let best_cam =
            select_best_default_camera_index(&diag.physical_cameras, cfg.camera_input.device_index);
        if best_cam != cfg.camera_input.device_index {
            cfg.camera_input.device_index = best_cam;
            let _ = cfg.save_to_file(&config_path);
        }

        // Initialize PluginRegistry and synchronize with SmartCamConfig
        let mut plugin_registry = PluginRegistry::new(&repo_root);
        if !cfg.enabled_plugins.is_empty() {
            plugin_registry.set_enabled_plugin_ids(&cfg.enabled_plugins);
        } else {
            cfg.enabled_plugins = plugin_registry.enabled_plugin_ids();
        }
        if !cfg.phone_camera_url.trim().is_empty() {
            let _ = plugin_registry.set_phone_camera_endpoint(&cfg.phone_camera_url);
        } else {
            cfg.phone_camera_url = plugin_registry.phone_camera_endpoint();
        }
        if cfg.phone_camera_port >= 1024 {
            let actual_port = plugin_registry.set_phone_server_port(cfg.phone_camera_port);
            cfg.phone_camera_port = actual_port;
        } else {
            cfg.phone_camera_port = plugin_registry.phone_server_port();
        }

        // Clause 3 PEFL v1.0 check: if not accepted within last 7 days, force foreground modal
        // and do NOT open the hardware camera until the user explicitly approves the license!
        let pefl_already_accepted = is_pefl_accepted(&repo_root);
        let effective_start_minimized = start_minimized && pefl_already_accepted;

        let mut pipeline =
            SmartCamPipelineInstance::new(&repo_root, &cfg, &diag.active_build_variant);
        pipeline.sync_face_and_silhouette_settings(
            &cfg,
            plugin_registry.is_plugin_enabled(PLUGIN_ID_MULTI_PERSON),
        );

        // Smart Hardware Benchmark on Startup (only runs automatically once per hardware fingerprint
        // after PEFL acceptance; otherwise loads cached report or runs when PEFL is accepted)
        let (benchmark_report, auto_bench_ran) = if pefl_already_accepted {
            let (rep, ran_now) =
                load_or_run_benchmark_on_startup(&repo_root, pipeline.lib.as_ref());
            if ran_now
                && cfg.compute_backend == "auto"
                && cfg.compositor.simd_acceleration == "auto"
            {
                cfg.compute_backend = rep.recommended_backend.clone();
                cfg.compositor.compute_backend = rep.recommended_backend.clone();
                cfg.compositor.simd_acceleration = rep.recommended_simd.clone();
                cfg.ai_enabled = rep.recommended_ai_enabled;
                cfg.segmentation.ai_selfie.ai_enabled = rep.recommended_ai_enabled;
                pipeline.apply_hardware_policy(&cfg);
                let _ = cfg.save_to_file(&config_path);
            }
            (rep, ran_now)
        } else {
            // Load cached report if present, or defer actual execution until PEFL is accepted
            let cached = crate::benchmark::load_benchmark_report(&repo_root)
                .unwrap_or_else(|| run_hardware_benchmark_suite(&repo_root, pipeline.lib.as_ref()));
            (cached, false)
        };

        if !effective_start_minimized && pefl_already_accepted {
            let _ = pipeline.open_hardware_camera(
                cfg.camera_input.device_index,
                cfg.camera_input.width,
                cfg.camera_input.height,
                cfg.camera_input.fps,
            );
        }

        let wizard_flag_file = repo_root.join("config").join(".wizard_completed");
        let wizard_step = if wizard_flag_file.is_file() || effective_start_minimized {
            0
        } else {
            1
        };

        let out_w = cfg.framing_output.output_width;
        let out_h = cfg.framing_output.output_height;
        diag.autostart_enabled = is_autostart_enabled();

        let status_message = if !pefl_already_accepted {
            "⚖️ Licenza PEFL v1.0 (Clausola 3): accetta i termini d'uso in primo piano per attivare SmartCam.".to_string()
        } else if auto_bench_ran {
            format!(
                "⭐ Benchmark Hardware completato! Profilo consigliato attivo: {} ({} Pt, {:.1} FPS).",
                benchmark_report.recommended_profile_label,
                benchmark_report.recommended_score,
                benchmark_report.recommended_fps
            )
        } else if plugin_registry.is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA) {
            format!(
                "📱 Plugin Fotocamera Smartphone ATTIVO ({}) — Flusso \"SmartCam\" pronto.",
                plugin_registry.phone_camera_endpoint()
            )
        } else if pipeline.camera_open {
            format!(
                "✅ Webcam attiva: Dispositivo #{} — Flusso \"SmartCam\" pronto per Zoom, Teams, Meet e WhatsApp.",
                cfg.camera_input.device_index
            )
        } else if pipeline.camera_busy_error {
            "⚠️ Webcam occupata da un'altra app o permessi Windows 11 bloccati (Flusso Demo 1080p attivo)."
                .to_string()
        } else {
            "Flusso Demo Sintetico 1080p attivo (Nessuna webcam fisica rilevata).".to_string()
        };

        let init_fps = cfg.camera_input.fps as f32;
        let init_landmarks = cfg.segmentation.ai_selfie.calib_landmarks;
        let init_step = if cfg.segmentation.ai_selfie.face_calibrated {
            5
        } else {
            0
        };
        let init_port_text = cfg.phone_camera_port.to_string();

        Self {
            repo_root,
            config_path,
            cfg,
            diag,
            pipeline,
            plugin_registry,
            benchmark_report,
            raw_capture_buf: Vec::new(),
            preview_buf: Vec::new(),
            raw_w: 1920,
            raw_h: 1080,
            out_w,
            out_h,
            frame_counter: 0,
            fps_window_start: Instant::now(),
            fps_window_frames: 0,
            current_fps: init_fps,
            show_advanced: false,
            plugins_modal_open: false,
            phone_port_input_focused: false,
            phone_port_fresh_focus: false,
            phone_port_input_text: init_port_text,
            perf_modal_open: false,
            face_calib_modal_open: false,
            face_calib_snapshot_buf: Vec::new(),
            face_calib_snap_w: out_w.max(640),
            face_calib_snap_h: out_h.max(480),
            face_calib_step: init_step,
            face_calib_points: init_landmarks,
            face_calib_photo_rect: RECT::new(0, 0, 0, 0),
            wizard_step,
            pefl_modal_open: !pefl_already_accepted,
            pefl_accepted_session: pefl_already_accepted,
            pefl_remember_7_days: true,
            minimized_to_tray: effective_start_minimized,
            on_demand_sleeping: effective_start_minimized,
            status_message,
            video_preview_rect: RECT::new(24, 96, 1140, 522),
            hit_zones: Vec::with_capacity(96),
            hover_pos: POINT { x: -1, y: -1 },
            tray_balloon_shown: false,
            bg_capture_countdown_end: None,
            crop_modal_open: false,
            crop_dragging: false,
            crop_drag_start_mouse: POINT { x: 0, y: 0 },
            crop_drag_start_pan: (0.5, 0.5),
            crop_canvas_rect: RECT::new(0, 0, 0, 0),
            crop_slider_rect: RECT::new(0, 0, 0, 0),
            open_dropdown: None,
            dropdown_trigger_rects: [
                (DropdownKind::AspectRatio, RECT::new(0, 0, 0, 0)),
                (DropdownKind::OutputResolution, RECT::new(0, 0, 0, 0)),
                (DropdownKind::OutputFps, RECT::new(0, 0, 0, 0)),
                (DropdownKind::SegmentationEngine, RECT::new(0, 0, 0, 0)),
            ],
            dropdown_overlay_zones: Vec::with_capacity(16),
            dropdown_menu_rect: RECT::new(0, 0, 0, 0),
            current_hicon_big: std::ptr::null_mut(),
            current_hicon_small: std::ptr::null_mut(),
            backbuf_dc: std::ptr::null_mut(),
            backbuf_bmp: std::ptr::null_mut(),
            backbuf_old_bmp: std::ptr::null_mut(),
            backbuf_w: 0,
            backbuf_h: 0,
            font_title: std::ptr::null_mut(),
            font_sub: std::ptr::null_mut(),
            font_btn: std::ptr::null_mut(),
            font_small: std::ptr::null_mut(),
            crop_modal_scratch: Vec::new(),
        }
    }

    pub unsafe fn release_gdi_resources(&mut self) {
        if !self.backbuf_dc.is_null() {
            if !self.backbuf_old_bmp.is_null() {
                SelectObject(self.backbuf_dc, self.backbuf_old_bmp);
                self.backbuf_old_bmp = std::ptr::null_mut();
            }
            if !self.backbuf_bmp.is_null() {
                DeleteObject(self.backbuf_bmp);
                self.backbuf_bmp = std::ptr::null_mut();
            }
            DeleteDC(self.backbuf_dc);
            self.backbuf_dc = std::ptr::null_mut();
        }
        self.backbuf_w = 0;
        self.backbuf_h = 0;

        if !self.font_title.is_null() {
            DeleteObject(self.font_title);
            self.font_title = std::ptr::null_mut();
        }
        if !self.font_sub.is_null() {
            DeleteObject(self.font_sub);
            self.font_sub = std::ptr::null_mut();
        }
        if !self.font_btn.is_null() {
            DeleteObject(self.font_btn);
            self.font_btn = std::ptr::null_mut();
        }
        if !self.font_small.is_null() {
            DeleteObject(self.font_small);
            self.font_small = std::ptr::null_mut();
        }
        if !self.current_hicon_big.is_null() {
            DestroyIcon(self.current_hicon_big);
            self.current_hicon_big = std::ptr::null_mut();
        }
        if !self.current_hicon_small.is_null() {
            DestroyIcon(self.current_hicon_small);
            self.current_hicon_small = std::ptr::null_mut();
        }
    }

    pub fn any_modal_open(&self) -> bool {
        self.pefl_modal_open
            || self.crop_modal_open
            || self.plugins_modal_open
            || self.perf_modal_open
            || self.face_calib_modal_open
            || self.show_advanced
            || self.wizard_step > 0
    }

    pub fn close_non_pefl_modals(&mut self) {
        self.open_dropdown = None;
        self.crop_modal_open = false;
        self.plugins_modal_open = false;
        self.phone_port_input_focused = false;
        self.perf_modal_open = false;
        self.face_calib_modal_open = false;
        self.show_advanced = false;
        self.wizard_step = 0;
    }

    pub fn capture_face_calib_snapshot(&mut self) {
        let target_w = self.cfg.camera_input.width.max(320);
        let target_h = self.cfg.camera_input.height.max(240);
        let needed_raw = (target_w * target_h * 4) as usize;
        if self.raw_capture_buf.len() < needed_raw {
            self.raw_capture_buf.resize(needed_raw, 255);
        }

        if let Some((w, h)) = self.plugin_registry.read_active_camera_plugin_frame(
            &mut self.raw_capture_buf,
            target_w,
            target_h,
        ) {
            self.raw_w = w;
            self.raw_h = h;
        } else if let Some((w, h)) =
            self.pipeline
                .read_camera_frame(&mut self.raw_capture_buf, target_w, target_h)
        {
            self.raw_w = w;
            self.raw_h = h;
        } else if self.pipeline.framed_buf.is_empty() {
            self.raw_w = target_w;
            self.raw_h = target_h;
            generate_synthetic_bgra_frame(
                &mut self.raw_capture_buf,
                self.raw_w,
                self.raw_h,
                self.frame_counter,
            );
        }

        let (ow, oh) =
            self.pipeline
                .process_frame(&self.raw_capture_buf, self.raw_w, self.raw_h, &self.cfg);
        self.out_w = ow;
        self.out_h = oh;

        let expected = (ow * oh * 4) as usize;
        if self.pipeline.framed_buf.len() >= expected && ow > 0 && oh > 0 {
            self.face_calib_snapshot_buf.clear();
            self.face_calib_snapshot_buf
                .extend_from_slice(&self.pipeline.framed_buf[..expected]);
            self.face_calib_snap_w = ow;
            self.face_calib_snap_h = oh;
        } else {
            self.face_calib_snap_w = 640;
            self.face_calib_snap_h = 480;
            generate_synthetic_bgra_frame(
                &mut self.face_calib_snapshot_buf,
                self.face_calib_snap_w,
                self.face_calib_snap_h,
                self.frame_counter,
            );
        }
        self.face_calib_step = 0;
        self.face_calib_points = self.cfg.segmentation.ai_selfie.calib_landmarks;
    }

    pub fn selected_camera_entry(&self) -> Option<&CameraDeviceEntry> {
        self.diag
            .physical_cameras
            .iter()
            .find(|c| c.index == self.cfg.camera_input.device_index)
            .or_else(|| self.diag.physical_cameras.first())
    }

    pub fn is_selected_camera_ir(&self) -> bool {
        if self
            .plugin_registry
            .is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA)
        {
            return false;
        }
        self.selected_camera_entry()
            .map(|c| c.is_infrared)
            .unwrap_or(false)
    }

    pub fn active_background_short_name(&self) -> String {
        let p = self
            .cfg
            .compositor
            .background_image_path
            .to_ascii_lowercase();
        if p.contains("preset_1") || p.contains("studio_blue") {
            "Studio Blu".to_string()
        } else if p.contains("preset_2") || p.contains("warm") {
            "Ufficio Caldo".to_string()
        } else if p.contains("preset_3") || p.contains("loft") || p.contains("minimal") {
            "Loft Chiaro".to_string()
        } else if p.contains("preset_4") || p.contains("executive") || p.contains("dark") {
            "Executive Dark".to_string()
        } else {
            Path::new(&self.cfg.compositor.background_image_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Immagine PC")
                .chars()
                .take(16)
                .collect()
        }
    }

    pub fn tick_frame(&mut self) -> bool {
        // Clause 3 PEFL v1.0 guard: do NOT process or broadcast frames before license acceptance!
        if !self.pefl_accepted_session {
            self.current_fps = 0.0;
            return false;
        }
        // Modalità On-Demand ("Set & Forget")
        if self.minimized_to_tray {
            let consumer_active = self.pipeline.is_consumer_active();
            if !consumer_active {
                if self.pipeline.camera_open {
                    self.pipeline.close_hardware_camera();
                }
                self.on_demand_sleeping = true;
                self.current_fps = 0.0;
                return false;
            } else if self.on_demand_sleeping {
                self.on_demand_sleeping = false;
                let _ = self.pipeline.open_hardware_camera(
                    self.cfg.camera_input.device_index,
                    self.cfg.camera_input.width,
                    self.cfg.camera_input.height,
                    self.cfg.camera_input.fps,
                );
            }
        } else if self.on_demand_sleeping {
            self.on_demand_sleeping = false;
            let _ = self.pipeline.open_hardware_camera(
                self.cfg.camera_input.device_index,
                self.cfg.camera_input.width,
                self.cfg.camera_input.height,
                self.cfg.camera_input.fps,
            );
        }

        let target_w = self.cfg.camera_input.width.max(320);
        let target_h = self.cfg.camera_input.height.max(240);
        let needed_raw = (target_w * target_h * 4) as usize;
        if self.raw_capture_buf.len() < needed_raw {
            self.raw_capture_buf.resize(needed_raw, 255);
        }

        // 1. Check if a CameraInput plugin (such as Smartphone Camera or WebRTC/RTSP) is enabled
        if let Some((w, h)) = self.plugin_registry.read_active_camera_plugin_frame(
            &mut self.raw_capture_buf,
            target_w,
            target_h,
        ) {
            self.raw_w = w;
            self.raw_h = h;
        } else if let Some((w, h)) =
            self.pipeline
                .read_camera_frame(&mut self.raw_capture_buf, target_w, target_h)
        {
            self.raw_w = w;
            self.raw_h = h;
        } else {
            self.raw_w = target_w;
            self.raw_h = target_h;
            generate_synthetic_bgra_frame(
                &mut self.raw_capture_buf,
                self.raw_w,
                self.raw_h,
                self.frame_counter,
            );
        }

        // Check if 3-second Reference Background Capture Countdown has completed BEFORE blending
        if let Some(deadline) = self.bg_capture_countdown_end {
            if Instant::now() >= deadline {
                self.bg_capture_countdown_end = None;
                let (ow_calc, oh_calc) = self.pipeline.process_frame(
                    &self.raw_capture_buf,
                    self.raw_w,
                    self.raw_h,
                    &self.cfg,
                );
                let framed_snapshot = self.pipeline.framed_buf.clone();
                if !framed_snapshot.is_empty() && ow_calc > 0 && oh_calc > 0 {
                    self.pipeline
                        .set_reference_background(&framed_snapshot, ow_calc, oh_calc);
                    if self.cfg.segmentation.mode == "passthrough"
                        || self.cfg.segmentation.mode == "green_screen"
                    {
                        self.cfg.segmentation.mode = "ai_selfie".to_string();
                    }
                    if self.cfg.segmentation.ai_selfie.engine_mode == 1 {
                        self.cfg.segmentation.ai_selfie.engine_mode = 0;
                    }
                    self.pipeline.update_greenscreen_config(&self.cfg);
                    let _ = self.cfg.save_to_file(&self.config_path);
                    self.status_message =
                        "✅ Sfondo Reale Catturato (Telo Virtuale attivo)! ⚠️ Tieni webcam/notebook su una superficie statica e ferma."
                            .to_string();
                }
            }
        }

        let (ow, oh) =
            self.pipeline
                .process_frame(&self.raw_capture_buf, self.raw_w, self.raw_h, &self.cfg);
        self.out_w = ow;
        self.out_h = oh;

        // Apply active VideoFilter plugins (e.g., Studio Color Enhancer) to Virtual Camera stream
        self.plugin_registry.apply_active_video_filter_plugins(
            &mut self.pipeline.out_buf,
            Some(&mut self.pipeline.mask_buf),
            ow,
            oh,
            false,
        );

        let fps = self.cfg.camera_input.fps;
        let out_ptr = self.pipeline.out_buf.as_ptr();
        let out_len = self.pipeline.out_buf.len();
        let out_slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        self.pipeline.push_frame_to_sink(out_slice, ow, oh, fps);

        // Prepare GUI preview buffer (includes preview-only overlays like Teleprompter when enabled)
        self.preview_buf.resize(self.pipeline.out_buf.len(), 255);
        self.preview_buf.copy_from_slice(&self.pipeline.out_buf);
        if self
            .plugin_registry
            .is_plugin_enabled(crate::plugins::PLUGIN_ID_TELEPROMPTER)
        {
            self.plugin_registry.apply_active_video_filter_plugins(
                &mut self.preview_buf,
                None,
                ow,
                oh,
                true,
            );
        }

        self.frame_counter += 1;
        self.fps_window_frames += 1;
        let elapsed = self.fps_window_start.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            self.current_fps = (self.fps_window_frames as f32) / elapsed;
            self.fps_window_frames = 0;
            self.fps_window_start = Instant::now();
        }
        true
    }

    pub fn apply_and_save_config(&mut self, msg: &str) {
        self.cfg.update_computed_dimensions();
        self.out_w = self.cfg.framing_output.output_width;
        self.out_h = self.cfg.framing_output.output_height;
        self.pipeline.update_greenscreen_config(&self.cfg);
        let multi_active = self
            .plugin_registry
            .is_plugin_enabled(PLUGIN_ID_MULTI_PERSON);
        self.pipeline
            .sync_face_and_silhouette_settings(&self.cfg, multi_active);
        self.pipeline.rebuild_cropped_background(
            self.out_w.max(16),
            self.out_h.max(16),
            self.cfg.compositor.background_zoom,
            self.cfg.compositor.background_pan_x,
            self.cfg.compositor.background_pan_y,
            self.cfg.compositor.background_mirror_h,
            self.cfg.compositor.background_mirror_v,
        );
        let _ = self.cfg.save_to_file(&self.config_path);
        self.status_message = msg.to_string();
    }

    pub fn sync_and_save_plugins(&mut self, msg: &str) {
        self.cfg.enabled_plugins = self.plugin_registry.enabled_plugin_ids();
        self.cfg.phone_camera_url = self.plugin_registry.phone_camera_endpoint();
        self.cfg.phone_camera_port = self.plugin_registry.phone_server_port();
        let multi_active = self
            .plugin_registry
            .is_plugin_enabled(PLUGIN_ID_MULTI_PERSON);
        self.pipeline
            .sync_face_and_silhouette_settings(&self.cfg, multi_active);
        let _ = self.plugin_registry.save_state(&self.repo_root);
        let _ = self.cfg.save_to_file(&self.config_path);
        self.status_message = msg.to_string();
    }

    pub fn switch_camera(&mut self, new_index: u32) {
        // If user switches physical camera, disable phone camera plugin so hardware webcam is shown
        if self
            .plugin_registry
            .is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA)
        {
            self.plugin_registry
                .set_plugin_enabled(PLUGIN_ID_PHONE_CAMERA, false);
            self.cfg.enabled_plugins = self.plugin_registry.enabled_plugin_ids();
            let _ = self.plugin_registry.save_state(&self.repo_root);
        }
        self.cfg.camera_input.device_index = new_index;
        self.pipeline.clear_reference_background();
        let opened = self.pipeline.open_hardware_camera(
            self.cfg.camera_input.device_index,
            self.cfg.camera_input.width,
            self.cfg.camera_input.height,
            self.cfg.camera_input.fps,
        );
        let _ = self.cfg.save_to_file(&self.config_path);
        if self.is_selected_camera_ir() {
            self.status_message =
                "⚠️ Hai selezionato il Sensore a Infrarossi (IR). L'immagine potrebbe essere B/N o lampeggiare."
                    .to_string();
        } else if opened {
            self.status_message = format!("📷 Fotocamera #{} selezionata e attiva.", new_index);
        } else {
            self.status_message =
                "⚠️ Webcam occupata da un'altra app o permessi bloccati (Flusso Demo attivo)."
                    .to_string();
        }
    }

    pub fn handle_action(&mut self, hwnd: HWND, action: UiAction) {
        match action {
            UiAction::PrevCamera => {
                if !self.diag.physical_cameras.is_empty() {
                    let cur_pos = self
                        .diag
                        .physical_cameras
                        .iter()
                        .position(|c| c.index == self.cfg.camera_input.device_index)
                        .unwrap_or(0);
                    let next_pos = if cur_pos == 0 {
                        self.diag.physical_cameras.len() - 1
                    } else {
                        cur_pos - 1
                    };
                    let new_idx = self.diag.physical_cameras[next_pos].index;
                    self.switch_camera(new_idx);
                }
            }
            UiAction::NextCamera => {
                if !self.diag.physical_cameras.is_empty() {
                    let cur_pos = self
                        .diag
                        .physical_cameras
                        .iter()
                        .position(|c| c.index == self.cfg.camera_input.device_index)
                        .unwrap_or(0);
                    let next_pos = (cur_pos + 1) % self.diag.physical_cameras.len();
                    let new_idx = self.diag.physical_cameras[next_pos].index;
                    self.switch_camera(new_idx);
                }
            }
            UiAction::RescanCameras => {
                self.diag = collect_diagnostics(&self.repo_root, true);
                let best = select_best_default_camera_index(
                    &self.diag.physical_cameras,
                    self.cfg.camera_input.device_index,
                );
                self.switch_camera(best);
                self.status_message = format!(
                    "🔄 Rilevate {} fotocamere (scelta automatica webcam a colori).",
                    self.diag.physical_cameras.len()
                );
            }
            UiAction::QuickTogglePhoneCamera => {
                let now_on = self
                    .plugin_registry
                    .toggle_plugin(PLUGIN_ID_PHONE_CAMERA)
                    .unwrap_or(false);
                let msg = if now_on {
                    format!(
                        "📱 Plugin Fotocamera Smartphone ATTIVATO! Endpoint: {} (clicca '🧩 Plugin' per cambiare Wi-Fi/USB).",
                        self.plugin_registry.phone_camera_endpoint()
                    )
                } else {
                    "📷 Ritorno alla Webcam Fisica del PC.".to_string()
                };
                self.sync_and_save_plugins(&msg);
            }
            UiAction::OpenPluginsModal => {
                self.close_non_pefl_modals();
                self.plugins_modal_open = true;
                self.phone_port_input_focused = false;
                self.phone_port_input_text = self.plugin_registry.phone_server_port().to_string();
                self.status_message =
                    "🧩 Gestione Plugin aperta: clicca sul campo Porta per digitare manualmente la porta o premi Default (8080)."
                        .to_string();
            }
            UiAction::ClosePluginsModal => {
                self.plugins_modal_open = false;
                self.phone_port_input_focused = false;
                let summary = self.plugin_registry.summary_line_it();
                self.sync_and_save_plugins(&format!(
                    "✅ Configurazione Plugin salvata — {}",
                    summary
                ));
            }
            UiAction::TogglePluginByIndex(idx) => {
                if let Some(pid) = self
                    .plugin_registry
                    .plugins()
                    .get(idx)
                    .map(|p| p.id.clone())
                {
                    let new_state = self.plugin_registry.toggle_plugin(&pid).unwrap_or(false);
                    let pname = self
                        .plugin_registry
                        .get_plugin(&pid)
                        .map(|p| p.name.clone())
                        .unwrap_or(pid);
                    let msg = format!(
                        "🧩 Plugin \"{}\" {}",
                        pname,
                        if new_state {
                            "ATTIVATO ✅"
                        } else {
                            "DISATTIVATO ⚪"
                        }
                    );
                    self.sync_and_save_plugins(&msg);
                }
            }
            UiAction::SetPhonePreset(preset_id) => {
                let port = self.plugin_registry.phone_server_port();
                let lan_ip = self.plugin_registry.local_lan_ip();
                let (url, desc) = match preset_id {
                    1 => (
                        "http://127.0.0.1:4747/video".to_string(),
                        "USB Tethering / ADB (DroidCam / Iriun 127.0.0.1:4747)".to_string(),
                    ),
                    2 => (
                        format!("rtsp://{}:8554/live", lan_ip),
                        format!("Stream RTSP Bassa Latenza (rtsp://{}:8554/live)", lan_ip),
                    ),
                    _ => (
                        format!("http://{}:{}/video", lan_ip, port),
                        format!("Wi-Fi Smartphone Server (http://{}:{})", lan_ip, port),
                    ),
                };
                let _ = self.plugin_registry.set_phone_camera_endpoint(&url);
                self.plugin_registry
                    .set_plugin_enabled(PLUGIN_ID_PHONE_CAMERA, true);
                self.sync_and_save_plugins(&format!(
                    "📱 Fotocamera Smartphone impostata su: {}",
                    desc
                ));
            }
            UiAction::FocusPhonePortInput => {
                self.phone_port_input_focused = true;
                self.phone_port_fresh_focus = true;
                self.status_message =
                    "⌨️ Digita con la tastiera la porta desiderata (es. 8080 o 9090) e premi Invio o '✅ Applica Porta'.".to_string();
            }
            UiAction::ApplyManualPhonePort => {
                self.phone_port_input_focused = false;
                self.phone_port_fresh_focus = false;
                let parsed = self
                    .phone_port_input_text
                    .trim()
                    .parse::<u16>()
                    .unwrap_or(8080)
                    .clamp(1024, 65535);
                let actual = self.plugin_registry.set_phone_server_port(parsed);
                self.cfg.phone_camera_port = actual;
                self.phone_port_input_text = actual.to_string();
                let srv_url = self.plugin_registry.phone_server_url();
                self.sync_and_save_plugins(&format!(
                    "🌐 Porta Server Smartphone impostata su {} (IN ASCOLTO) — Apri sul telefono: {}",
                    actual, srv_url
                ));
            }
            UiAction::SetPhoneServerPortPreset(port) => {
                self.phone_port_input_focused = false;
                let actual = self
                    .plugin_registry
                    .set_phone_server_port(port.clamp(1024, 65535));
                self.cfg.phone_camera_port = actual;
                self.phone_port_input_text = actual.to_string();
                let srv_url = self.plugin_registry.phone_server_url();
                self.sync_and_save_plugins(&format!(
                    "🌐 Porta Server Smartphone ripristinata a {} (Default) — Apri sul telefono: {}",
                    actual, srv_url
                ));
            }
            UiAction::UnlockPhoneFirewall => {
                match self.plugin_registry.unlock_windows_firewall_for_phone() {
                    Ok(msg) => {
                        self.status_message = msg;
                    }
                    Err(err) => {
                        self.status_message = format!("⚠️ Firewall Windows: {}", err);
                    }
                }
            }
            UiAction::ActivatePhoneUsbOrHotspotBridge => {
                let msg = self.plugin_registry.activate_phone_usb_or_hotspot_bridge();
                self.plugin_registry
                    .set_plugin_enabled(PLUGIN_ID_PHONE_CAMERA, true);
                self.sync_and_save_plugins(&msg);
            }
            UiAction::PastePhoneUrlFromClipboard => {
                if let Some(clip) = read_clipboard_text(hwnd) {
                    let trimmed = clip.lines().next().unwrap_or("").trim();
                    if !trimmed.is_empty() {
                        let clean_digits = trimmed.trim_start_matches(':');
                        if let Ok(port_only) = clean_digits.parse::<u16>() {
                            let actual = self
                                .plugin_registry
                                .set_phone_server_port(port_only.clamp(1024, 65535));
                            self.cfg.phone_camera_port = actual;
                            self.plugin_registry
                                .set_plugin_enabled(PLUGIN_ID_PHONE_CAMERA, true);
                            let srv_url = self.plugin_registry.phone_server_url();
                            self.sync_and_save_plugins(&format!(
                                "📋 Porta Smartphone {} incollata dagli Appunti! Apri sul telefono: {}",
                                actual, srv_url
                            ));
                        } else {
                            if let Some(extracted_port) = extract_port_from_url_str(trimmed) {
                                let actual =
                                    self.plugin_registry.set_phone_server_port(extracted_port);
                                self.cfg.phone_camera_port = actual;
                            }
                            let _ = self.plugin_registry.set_phone_camera_endpoint(trimmed);
                            self.plugin_registry
                                .set_plugin_enabled(PLUGIN_ID_PHONE_CAMERA, true);
                            self.sync_and_save_plugins(&format!(
                                "📋 Indirizzo/Porta Fotocamera Smartphone incollato dagli Appunti: {} (Porta Server: {})",
                                trimmed,
                                self.plugin_registry.phone_server_port()
                            ));
                        }
                    } else {
                        self.status_message =
                            "⚠️ Gli Appunti non contengono una porta o un indirizzo URL valido."
                                .to_string();
                    }
                } else {
                    self.status_message =
                        "⚠️ Nessun testo trovato negli Appunti di Windows (copia una porta es. 8080 o un URL)."
                            .to_string();
                }
            }
            UiAction::OpenFaceCalibModal => {
                self.close_non_pefl_modals();
                self.face_calib_modal_open = true;
                self.capture_face_calib_snapshot();
                self.status_message =
                    "🎯 Acquisizione & Calibrazione Volto Guidata (5 Punti): PASSO 1 di 5 — clicca sull'OCCHIO DESTRO nella foto."
                        .to_string();
            }
            UiAction::CloseFaceCalibModal => {
                self.face_calib_modal_open = false;
                let sil_lbl = if self.cfg.segmentation.ai_selfie.silhouette_mode == 1 {
                    "🙂 Solo Volto"
                } else {
                    "👤 Volto + Busto"
                };
                let cal_lbl = if self.cfg.segmentation.ai_selfie.face_calibrated {
                    format!(
                        "✓ Volto Calibrato (Cb={:.0}, Cr={:.0})",
                        self.cfg.segmentation.ai_selfie.calib_skin_cb,
                        self.cfg.segmentation.ai_selfie.calib_skin_cr
                    )
                } else {
                    "Auto Default".to_string()
                };
                self.apply_and_save_config(&format!(
                    "✅ Impostazioni Volto & Sagoma applicate: {} — {}",
                    sil_lbl, cal_lbl
                ));
            }
            UiAction::RetakeFaceCalibSnapshot => {
                self.capture_face_calib_snapshot();
                self.status_message =
                    "📸 Nuova foto scattata dalla webcam! PASSO 1 di 5: clicca sull'OCCHIO DESTRO."
                        .to_string();
            }
            UiAction::UndoFaceCalibPoint => {
                if self.face_calib_step > 0 {
                    self.face_calib_step -= 1;
                    self.status_message = format!(
                        "↩️ Punto corretto: ripeti il PASSO {} di 5.",
                        self.face_calib_step + 1
                    );
                } else {
                    self.status_message = "ℹ️ Sei già al Passo 1 di 5 (Occhio Destro).".to_string();
                }
            }
            UiAction::ResetFaceCalibration => {
                self.cfg.segmentation.ai_selfie.face_calibrated = false;
                self.face_calib_step = 0;
                self.apply_and_save_config(
                    "🗑️ Calibrazione manuale volto disattivata — Ripristinato tracciamento automatico.",
                );
            }
            UiAction::SetSilhouetteMode(mode) => {
                let m = mode.clamp(0, 1);
                self.cfg.segmentation.ai_selfie.silhouette_mode = m;
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                let lbl = if m == 1 {
                    "🙂 Mostra: Solo Volto (Esclude Busto e Spalle)"
                } else {
                    "👤 Mostra: Volto + Busto (Sagoma Completa)"
                };
                self.apply_and_save_config(&format!("✅ Modalità Sagoma impostata su: {}", lbl));
            }
            UiAction::ToggleSilhouetteMode => {
                let next = if self.cfg.segmentation.ai_selfie.silhouette_mode == 0 {
                    1
                } else {
                    0
                };
                self.handle_action(hwnd, UiAction::SetSilhouetteMode(next));
            }
            UiAction::QuickToggleMultiPerson => {
                let now_on = self
                    .plugin_registry
                    .toggle_plugin(PLUGIN_ID_MULTI_PERSON)
                    .unwrap_or(false);
                let msg = if now_on {
                    "👥 Plugin Tracking Persone Multiple ATTIVATO! Tutti i volti nell'inquadratura sono inclusi."
                        .to_string()
                } else {
                    "👤 Tracking Singola Persona attivo (Focus sul soggetto principale)."
                        .to_string()
                };
                self.sync_and_save_plugins(&msg);
            }
            UiAction::RescanPluginsDir => {
                let plugins_dir = self.repo_root.join("plugins");
                let count = self.plugin_registry.scan_external_plugins_dir(&plugins_dir);
                self.status_message = format!(
                    "🔄 Cartella plugins/ scansionata: {} nuovi plugin esterni (.dll) caricati ({} totali).",
                    count,
                    self.plugin_registry.plugins().len()
                );
            }
            UiAction::OpenPluginGuideDoc => {
                let doc = self.repo_root.join("how-to-make-a-plugin.md");
                let target = if doc.is_file() {
                    doc.to_string_lossy().to_string()
                } else {
                    self.repo_root.to_string_lossy().to_string()
                };
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &target])
                    .spawn();
                self.status_message =
                    "📖 Aperta guida SDK \"how-to-make-a-plugin.md\" per creare plugin SmartCam."
                        .to_string();
            }
            UiAction::OpenPerfModal => {
                self.close_non_pefl_modals();
                self.perf_modal_open = true;
                self.status_message =
                    "⚡ Centro Prestazioni: seleziona CPU/GPU (Vulkan, OpenCL, CUDA, ROCm), AVX, AI o avvia il Benchmark."
                        .to_string();
            }
            UiAction::ClosePerfModal => {
                self.perf_modal_open = false;
                self.status_message = format!(
                    "✅ Configurazione Hardware attiva: {} | AI: {}",
                    self.pipeline.simd_backend_label,
                    if self.cfg.ai_enabled {
                        "ATTIVA"
                    } else {
                        "DISATTIVATA (Ultra-Leggera)"
                    }
                );
            }
            UiAction::SelectComputeBackend(b_id) => {
                if b_id >= 2 {
                    let avail = self
                        .benchmark_report
                        .devices
                        .iter()
                        .any(|d| d.backend_id == b_id as u32 && d.available);
                    if !avail {
                        let b_disp = crate::benchmark::compute_backend_id_to_display(b_id as u32);
                        self.status_message = format!(
                            "⚠️ Il backend GPU \"{}\" non è supportato dall'hardware di questo PC.",
                            b_disp
                        );
                        return;
                    }
                }
                let b_key = crate::benchmark::compute_backend_id_to_str(b_id as u32);
                let b_disp = crate::benchmark::compute_backend_id_to_display(b_id as u32);
                self.cfg.compute_backend = b_key.to_string();
                self.cfg.compositor.compute_backend = b_key.to_string();
                self.pipeline.apply_hardware_policy(&self.cfg);
                let _ = self.cfg.save_to_file(&self.config_path);
                self.status_message = format!(
                    "⚡ Dispositivo di Calcolo impostato su: {} ({})",
                    b_disp, self.pipeline.simd_backend_label
                );
            }
            UiAction::SelectSimdMode(s_id) => {
                let s_key = crate::benchmark::simd_policy_id_to_str(s_id as u32);
                let s_disp = crate::benchmark::simd_policy_id_to_display(s_id as u32);
                self.cfg.compositor.simd_acceleration = s_key.to_string();
                self.pipeline.apply_hardware_policy(&self.cfg);
                let _ = self.cfg.save_to_file(&self.config_path);
                self.status_message = format!(
                    "🚀 Istruzioni Vettoriali CPU (SIMD) impostate su: {} ({})",
                    s_disp, self.pipeline.simd_backend_label
                );
            }
            UiAction::SetAiEnabled(en) => {
                self.cfg.ai_enabled = en;
                self.cfg.segmentation.ai_selfie.ai_enabled = en;
                self.pipeline.apply_hardware_policy(&self.cfg);
                let _ = self.cfg.save_to_file(&self.config_path);
                self.status_message = if en {
                    "🤖 Intelligenza Artificiale ATTIVATA (Massima precisione scontorno volto/capelli)."
                        .to_string()
                } else {
                    "🍃 Intelligenza Artificiale DISATTIVATA (Modalità Ultra-Leggera <0.2ms / Differenza Sfondo & Chroma)."
                        .to_string()
                };
            }
            UiAction::RunHardwareBenchmarkNow => {
                self.status_message =
                    "⏳ Esecuzione Benchmark Hardware (CPU AVX-512/AVX2/Scalar, Vulkan, OpenCL, CUDA, ROCm) in corso..."
                        .to_string();
                let report =
                    run_hardware_benchmark_suite(&self.repo_root, self.pipeline.lib.as_ref());
                // Restore current user hardware policy on the live engine after benchmarking
                self.pipeline.apply_hardware_policy(&self.cfg);
                self.status_message = format!(
                    "✅ Benchmark completato! ⭐ Consigliato: {} ({} Pt, {:.1} FPS, ~{:.1}% CPU).",
                    report.recommended_profile_label,
                    report.recommended_score,
                    report.recommended_fps,
                    report.recommended_cpu_load_pct
                );
                self.benchmark_report = report;
            }
            UiAction::ApplyRecommendedBenchmark => {
                let rec_back = self.benchmark_report.recommended_backend.clone();
                let rec_simd = self.benchmark_report.recommended_simd.clone();
                let rec_ai = self.benchmark_report.recommended_ai_enabled;
                let rec_lbl = self.benchmark_report.recommended_profile_label.clone();
                let rec_score = self.benchmark_report.recommended_score;

                self.cfg.compute_backend = rec_back.clone();
                self.cfg.compositor.compute_backend = rec_back;
                self.cfg.compositor.simd_acceleration = rec_simd;
                self.cfg.ai_enabled = rec_ai;
                self.cfg.segmentation.ai_selfie.ai_enabled = rec_ai;
                self.pipeline.apply_hardware_policy(&self.cfg);
                let _ = self.cfg.save_to_file(&self.config_path);
                self.status_message = format!(
                    "⭐ Applicata Configurazione Consigliata: {} ({} Pt) — {}!",
                    rec_lbl, rec_score, self.pipeline.simd_backend_label
                );
            }
            UiAction::ApplyBenchmarkProfile(idx) => {
                if let Some(prof) = self.benchmark_report.profiles.get(idx).cloned() {
                    if prof.available {
                        self.cfg.compute_backend = prof.compute_backend.clone();
                        self.cfg.compositor.compute_backend = prof.compute_backend.clone();
                        self.cfg.compositor.simd_acceleration = prof.simd_mode.clone();
                        self.cfg.ai_enabled = prof.ai_enabled;
                        self.cfg.segmentation.ai_selfie.ai_enabled = prof.ai_enabled;
                        self.pipeline.apply_hardware_policy(&self.cfg);
                        let _ = self.cfg.save_to_file(&self.config_path);
                        self.status_message = format!(
                            "✅ Profilo \"{}\" applicato ({} Pt, {:.1} FPS, ~{:.1}% CPU)!",
                            prof.label, prof.score, prof.estimated_fps, prof.estimated_cpu_load_pct
                        );
                    } else {
                        self.status_message = format!(
                            "⚠️ Il profilo \"{}\" non è disponibile su questo PC ({}).",
                            prof.label, prof.availability_label
                        );
                    }
                }
            }
            UiAction::OpenPeflModal => {
                self.close_non_pefl_modals();
                self.pefl_modal_open = true;
                self.status_message =
                    "📜 Licenza PEFL v1.0 (Clausola 3) — Personal and Educational Free License."
                        .to_string();
            }
            UiAction::PeflToggleRemember => {
                self.pefl_remember_7_days = !self.pefl_remember_7_days;
            }
            UiAction::PeflOpenFullLicense => {
                let lic_it = self.repo_root.join("LICENSE.it");
                let lic_en = self.repo_root.join("LICENSE");
                let target = if lic_it.is_file() {
                    lic_it.to_string_lossy().to_string()
                } else if lic_en.is_file() {
                    lic_en.to_string_lossy().to_string()
                } else {
                    "https://github.com/RubensRainelli/pefl-license-spec".to_string()
                };
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &target])
                    .spawn();
                self.status_message =
                    "📜 Aperto documento integrale Licenza PEFL v1.0.".to_string();
            }
            UiAction::PeflAccept => {
                save_pefl_acceptance(&self.repo_root, self.pefl_remember_7_days);
                self.pefl_accepted_session = true;
                self.pefl_modal_open = false;

                // Smart One-Time Auto-Benchmark on First Launch if not yet benchmarked on this PC
                let (rep, ran_now) =
                    load_or_run_benchmark_on_startup(&self.repo_root, self.pipeline.lib.as_ref());
                if ran_now {
                    self.cfg.compute_backend = rep.recommended_backend.clone();
                    self.cfg.compositor.compute_backend = rep.recommended_backend.clone();
                    self.cfg.compositor.simd_acceleration = rep.recommended_simd.clone();
                    self.cfg.ai_enabled = rep.recommended_ai_enabled;
                    self.cfg.segmentation.ai_selfie.ai_enabled = rep.recommended_ai_enabled;
                    self.pipeline.apply_hardware_policy(&self.cfg);
                    let _ = self.cfg.save_to_file(&self.config_path);
                }
                self.benchmark_report = rep;

                if !self.pipeline.camera_open && !self.minimized_to_tray {
                    let _ = self.pipeline.open_hardware_camera(
                        self.cfg.camera_input.device_index,
                        self.cfg.camera_input.width,
                        self.cfg.camera_input.height,
                        self.cfg.camera_input.fps,
                    );
                }
                self.status_message = if ran_now {
                    format!(
                        "✅ PEFL v1.0 accettata & Benchmark completato! ⭐ Profilo attivo: {} ({} Pt).",
                        self.benchmark_report.recommended_profile_label,
                        self.benchmark_report.recommended_score
                    )
                } else if self.pefl_remember_7_days {
                    "✅ Licenza PEFL v1.0 accettata (memorizzata per 7 giorni). Flusso \"SmartCam\" attivo!"
                        .to_string()
                } else {
                    "✅ Licenza PEFL v1.0 accettata per questa sessione. Flusso \"SmartCam\" attivo!"
                        .to_string()
                };
            }
            UiAction::PeflReject => {
                save_pefl_acceptance(&self.repo_root, false);
                self.pefl_accepted_session = false;
                unsafe {
                    remove_tray_icon(hwnd);
                    DestroyWindow(hwnd);
                }
            }
            UiAction::OpenWizard => {
                self.close_non_pefl_modals();
                self.wizard_step = 1;
                self.status_message =
                    "🧭 Guida Rapida di Primo Avvio aperta (Passo 1 di 3).".to_string();
            }
            UiAction::WizardNext => {
                if self.wizard_step < 3 {
                    self.wizard_step += 1;
                } else {
                    self.finish_wizard();
                }
            }
            UiAction::WizardPrev => {
                if self.wizard_step > 1 {
                    self.wizard_step -= 1;
                }
            }
            UiAction::WizardClose => {
                self.finish_wizard();
            }
            UiAction::WizardSelectEffect(eff) => match eff {
                0 => self.handle_action(hwnd, UiAction::StyleOriginal),
                1 => self.handle_action(hwnd, UiAction::StyleBlur),
                2 => self.handle_action(hwnd, UiAction::StyleAiOffice),
                _ => self.handle_action(hwnd, UiAction::StyleGreenScreen),
            },
            UiAction::ToggleAdvanced => {
                let target = !self.show_advanced;
                self.close_non_pefl_modals();
                self.show_advanced = target;
                self.status_message = if self.show_advanced {
                    "⚙️ Calibrazione Avanzata aperta (parametri HSV Telo Verde, Soglie AI, Morfologia e Bokeh)."
                        .to_string()
                } else {
                    "Vista Studio principale attiva.".to_string()
                };
            }
            UiAction::CloseAdvancedModal => {
                self.show_advanced = false;
                self.apply_and_save_config("✅ Parametri di Calibrazione Avanzata salvati.");
            }
            UiAction::MinimizeToTray => {
                self.open_dropdown = None;
                minimize_window_to_tray(hwnd, self);
            }
            UiAction::StyleOriginal => {
                self.cfg.segmentation.mode = "passthrough".to_string();
                self.apply_and_save_config(
                    "🎥 Effetto attivo: Video Originale (Senza ritaglio sfondo).",
                );
            }
            UiAction::StyleBlur => {
                self.cfg.segmentation.mode = "ai_selfie".to_string();
                self.cfg.compositor.background_mode = "blur".to_string();
                self.apply_and_save_config(
                    "💧 Effetto attivo: Sfoca Sfondo Dinamico (Tracciamento Volto/Busto in tempo reale).",
                );
            }
            UiAction::StyleAiOffice => {
                self.cfg.segmentation.mode = "ai_selfie".to_string();
                if self.cfg.compositor.background_mode == "blur" {
                    self.cfg.compositor.background_mode = "image".to_string();
                }
                if self.cfg.compositor.background_image_path.trim().is_empty() {
                    self.cfg.compositor.background_image_path =
                        "assets/backgrounds/preset_1_studio_blue.jpg".to_string();
                }
                let p = self.cfg.compositor.background_image_path.clone();
                let _ = self.pipeline.load_or_generate_background_image(&p);
                self.apply_and_save_config(
                    "🤖 Effetto attivo: Sfondo Virtuale (Clicca '🖼️ Scegli / Ritaglia Sfondo...' per Galleria e Zoom!).",
                );
            }
            UiAction::StyleGreenScreen => {
                self.cfg.segmentation.mode = "green_screen".to_string();
                if self.cfg.compositor.background_mode == "blur" {
                    self.cfg.compositor.background_mode = "image".to_string();
                }
                self.apply_and_save_config(
                    "🟩 Effetto attivo: Telo Verde (Green Screen). Clicca sul video per calibrare col contagocce!",
                );
            }
            UiAction::BgModeImage => {
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                self.cfg.compositor.background_mode = "image".to_string();
                if self.cfg.compositor.background_image_path.trim().is_empty() {
                    self.cfg.compositor.background_image_path =
                        "assets/backgrounds/preset_1_studio_blue.jpg".to_string();
                }
                let p = self.cfg.compositor.background_image_path.clone();
                let _ = self.pipeline.load_or_generate_background_image(&p);
                self.apply_and_save_config(
                    "🖼️ Riempimento Sfondo: Immagine / Preset Studio attivo.",
                );
            }
            UiAction::BgModeBlur => {
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                self.cfg.compositor.background_mode = "blur".to_string();
                self.apply_and_save_config("💧 Riempimento Sfondo: Sfocatura Bokeh attiva.");
            }
            UiAction::BgModeSolidColor => {
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                self.cfg.compositor.background_mode = "solid_color".to_string();
                self.apply_and_save_config("🎨 Riempimento Sfondo: Tinta Unita RGB attiva.");
            }
            UiAction::SelectStudioPreset(idx) => {
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                self.cfg.compositor.background_mode = "image".to_string();
                let rel_path = match idx {
                    1 => "assets/backgrounds/preset_1_studio_blue.jpg",
                    2 => "assets/backgrounds/preset_2_warm_office.jpg",
                    3 => "assets/backgrounds/preset_3_minimal_loft.jpg",
                    _ => "assets/backgrounds/preset_4_dark_executive.jpg",
                };
                self.cfg.compositor.background_image_path = rel_path.to_string();
                self.cfg.compositor.background_zoom = 1.0;
                self.cfg.compositor.background_pan_x = 0.5;
                self.cfg.compositor.background_pan_y = 0.5;
                let _ = self.pipeline.load_or_generate_background_image(rel_path);
                let name = match idx {
                    1 => "Studio Blu",
                    2 => "Ufficio Caldo",
                    3 => "Loft Chiaro",
                    _ => "Executive Dark",
                };
                self.apply_and_save_config(&format!(
                    "🖼️ Sfondo selezionato: {} — Trascina l'immagine nel riquadro per inquadrarla o usa lo Zoom!",
                    name
                ));
            }
            UiAction::OpenCustomBgImageDialog => {
                let init_dir = self.repo_root.join("assets").join("backgrounds");
                if let Some(chosen_path) = open_background_file_dialog(
                    hwnd,
                    if init_dir.is_dir() {
                        Some(&init_dir)
                    } else {
                        Some(&self.repo_root)
                    },
                ) {
                    if self.cfg.segmentation.mode == "passthrough" {
                        self.cfg.segmentation.mode = "ai_selfie".to_string();
                    }
                    self.cfg.compositor.background_mode = "image".to_string();
                    let path_str = chosen_path.to_string_lossy().to_string();
                    self.cfg.compositor.background_image_path = path_str.clone();
                    self.cfg.compositor.background_zoom = 1.0;
                    self.cfg.compositor.background_pan_x = 0.5;
                    self.cfg.compositor.background_pan_y = 0.5;
                    let _ = self.pipeline.load_or_generate_background_image(&path_str);
                    self.close_non_pefl_modals();
                    self.crop_modal_open = true;
                    let fname = chosen_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("immagine");
                    self.apply_and_save_config(&format!(
                        "📂 Caricata \"{}\" — Trascina l'immagine e usa lo Zoom per ritagliare lo sfondo!",
                        fname
                    ));
                }
            }
            UiAction::OpenBgCropModal => {
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                self.cfg.compositor.background_mode = "image".to_string();
                if self.pipeline.bg_source_image.is_empty() {
                    let p = self.cfg.compositor.background_image_path.clone();
                    let _ = self.pipeline.load_or_generate_background_image(&p);
                }
                self.close_non_pefl_modals();
                self.crop_modal_open = true;
                self.apply_and_save_config(
                    "🖼️ Galleria Sfondi & Ritaglio: scegli una miniatura Studio, carica una foto dal PC o inquadra col mouse!",
                );
            }
            UiAction::CropZoomOut => {
                self.cfg.compositor.background_zoom =
                    (self.cfg.compositor.background_zoom - 0.15).clamp(1.0, 4.0);
                self.apply_and_save_config(&format!(
                    "🔍 Zoom Sfondo: {:.0}%",
                    self.cfg.compositor.background_zoom * 100.0
                ));
            }
            UiAction::CropZoomIn => {
                self.cfg.compositor.background_zoom =
                    (self.cfg.compositor.background_zoom + 0.15).clamp(1.0, 4.0);
                self.apply_and_save_config(&format!(
                    "🔍 Zoom Sfondo: {:.0}%",
                    self.cfg.compositor.background_zoom * 100.0
                ));
            }
            UiAction::CropReset => {
                self.cfg.compositor.background_zoom = 1.0;
                self.cfg.compositor.background_pan_x = 0.5;
                self.cfg.compositor.background_pan_y = 0.5;
                self.cfg.compositor.background_mirror_h = false;
                self.cfg.compositor.background_mirror_v = false;
                self.apply_and_save_config(
                    "🔄 Ritaglio, Zoom e Specchiatura sfondo ripristinati (100% centrato).",
                );
            }
            UiAction::CropApplyAndClose => {
                self.crop_modal_open = false;
                self.crop_dragging = false;
                let bg_name = self.active_background_short_name();
                let mir_tag = match (
                    self.cfg.compositor.background_mirror_h,
                    self.cfg.compositor.background_mirror_v,
                ) {
                    (true, true) => " • Specchiato Oriz.+Vert.",
                    (true, false) => " • Specchiato Orizzontale",
                    (false, true) => " • Specchiato Verticale",
                    (false, false) => "",
                };
                self.apply_and_save_config(&format!(
                    "✅ Sfondo \"{}\" applicato (Zoom {:.0}%, formato {}{}).",
                    bg_name,
                    self.cfg.compositor.background_zoom * 100.0,
                    self.cfg.framing_output.aspect_ratio,
                    mir_tag
                ));
            }
            UiAction::ToggleBgMirrorH => {
                self.cfg.compositor.background_mirror_h = !self.cfg.compositor.background_mirror_h;
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                let msg = if self.cfg.compositor.background_mirror_h {
                    "↔️ Specchia Sfondo Orizzontale: ATTIVO (compensa i programmi che specchiano la telecamera di default)."
                } else {
                    "↔️ Specchia Sfondo Orizzontale: DISATTIVATO (orientamento originale)."
                };
                self.apply_and_save_config(msg);
            }
            UiAction::ToggleBgMirrorV => {
                self.cfg.compositor.background_mirror_v = !self.cfg.compositor.background_mirror_v;
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                let msg = if self.cfg.compositor.background_mirror_v {
                    "↕️ Specchia Sfondo Verticale: ATTIVO."
                } else {
                    "↕️ Specchia Sfondo Verticale: DISATTIVATO."
                };
                self.apply_and_save_config(msg);
            }
            UiAction::ToggleSourceBitDepth => {
                self.cfg.camera_input.bit_depth = if self.cfg.camera_input.bit_depth == 64 {
                    32
                } else {
                    64
                };
                let msg = if self.cfg.camera_input.bit_depth == 64 {
                    "🎨 Sorgente Video impostata a 64-bit (BGRA64 Deep Color 16-bit/canale • Architettura nativa x86_64)."
                } else {
                    "🎨 Sorgente Video impostata a 32-bit (BGRA32 Standard 8-bit/canale • Architettura nativa x86_64)."
                };
                self.apply_and_save_config(msg);
            }
            UiAction::StartReferenceBgCapture => {
                self.open_dropdown = None;
                if self.cfg.segmentation.mode == "passthrough"
                    || self.cfg.segmentation.mode == "green_screen"
                {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                if self.cfg.segmentation.ai_selfie.engine_mode == 1 {
                    self.cfg.segmentation.ai_selfie.engine_mode = 0;
                }
                self.pipeline.update_greenscreen_config(&self.cfg);
                let _ = self.cfg.save_to_file(&self.config_path);
                self.bg_capture_countdown_end = Some(Instant::now() + Duration::from_secs(3));
                self.status_message =
                    "⚠️ Avviso Telo Virtuale: assicurati che webcam/notebook siano su una superficie statica e ferma! Spostati per 3s..."
                        .to_string();
            }
            UiAction::ClearReferenceBg => {
                self.bg_capture_countdown_end = None;
                self.pipeline.clear_reference_background();
                self.status_message =
                    "🗑️ Sfondo reale catturato rimosso (Tracciamento dinamico volto/busto attivo)."
                        .to_string();
            }
            UiAction::SetIconTheme(theme_id) => {
                let (theme_key, theme_label) = match theme_id {
                    1 => ("emerald_cam", "Emerald Cam (Verde Smeraldo)"),
                    2 => ("neon_iris", "Neon Iris (Viola/Magenta)"),
                    _ => ("cyber_lens", "Cyber Lens (Ciano/Blu)"),
                };
                self.cfg.app_icon_theme = theme_key.to_string();
                let _ = self.cfg.save_to_file(&self.config_path);
                unsafe {
                    apply_window_and_tray_icon(hwnd, self);
                }
                self.status_message =
                    format!("🎨 Icona App & System Tray cambiata in: {}", theme_label);
            }
            UiAction::ToggleDropdown(kind) => {
                if self.open_dropdown == Some(kind) {
                    self.open_dropdown = None;
                } else {
                    self.open_dropdown = Some(kind);
                }
            }
            UiAction::SelectAspectRatio(idx) => {
                self.open_dropdown = None;
                let (ar_code, ar_label) = match idx {
                    0 => ("16:9", "16:9 (Widescreen)"),
                    1 => ("4:3", "4:3 (Standard)"),
                    2 => ("1:1", "1:1 (Quadrato)"),
                    _ => ("9:16", "9:16 (Verticale)"),
                };
                self.cfg.framing_output.aspect_ratio = ar_code.to_string();
                self.pipeline.clear_reference_background();
                self.cfg.update_computed_dimensions();
                let (ow, oh) = (
                    self.cfg.framing_output.output_width,
                    self.cfg.framing_output.output_height,
                );
                self.apply_and_save_config(&format!(
                    "📐 Formato impostato su: {} — Output {} ({}x{}).",
                    ar_label, self.cfg.framing_output.resolution_preset, ow, oh
                ));
            }
            UiAction::SelectResolutionPreset(idx) => {
                self.open_dropdown = None;
                let (res_code, res_desc) = match idx {
                    0 => ("1080p", "1080p (Full HD)"),
                    1 => ("900p", "900p (HD+)"),
                    2 => ("720p", "720p (HD)"),
                    3 => ("540p", "540p (qHD)"),
                    4 => ("480p", "480p (SD)"),
                    5 => ("360p", "360p (Risparmio Banda)"),
                    _ => ("240p", "240p (Banda Estrema)"),
                };
                self.cfg.framing_output.resolution_preset = res_code.to_string();
                self.pipeline.clear_reference_background();
                self.cfg.update_computed_dimensions();
                let (ow, oh) = (
                    self.cfg.framing_output.output_width,
                    self.cfg.framing_output.output_height,
                );
                self.apply_and_save_config(&format!(
                    "🎞️ Definizione Output impostata su: {} — {}x{} ({}).",
                    res_desc, ow, oh, self.cfg.framing_output.aspect_ratio
                ));
            }
            UiAction::SelectOutputFps(fps_val) => {
                self.open_dropdown = None;
                let clamped_fps = fps_val.clamp(5, 120);
                self.cfg.camera_input.fps = clamped_fps;
                let interval_ms = (1000 / clamped_fps).max(15);
                unsafe {
                    SetTimer(hwnd, TIMER_FRAME_ID, interval_ms, std::ptr::null());
                }
                let fps_desc = match clamped_fps {
                    60 => "60 FPS (Massima Fluidità)",
                    30 => "30 FPS (Standard)",
                    25 => "25 FPS (PAL Standard)",
                    24 => "24 FPS (Cinema / Risparmio)",
                    20 => "20 FPS (Eco)",
                    15 => "15 FPS (Bassa Banda)",
                    10 => "10 FPS (Connessione Estrema)",
                    _ => "Personalizzato",
                };
                self.apply_and_save_config(&format!(
                    "⏱️ Framerate Output/Acquisizione impostato su: {} (Timer {} ms).",
                    fps_desc, interval_ms
                ));
            }
            UiAction::SelectAiEngineMode(mode_id) => {
                self.open_dropdown = None;
                let m = mode_id.clamp(0, 2);
                self.cfg.segmentation.ai_selfie.engine_mode = m;
                if self.cfg.segmentation.mode == "passthrough" {
                    self.cfg.segmentation.mode = "ai_selfie".to_string();
                }
                let m_desc = match m {
                    1 => "Sagoma & Volto (Senza Cattura Sfondo)",
                    2 => "Differenza Sfondo (Telo Verde Virtuale Anti-LED) — ⚠️ Tieni webcam/notebook su superficie statica!",
                    _ => "Auto (Sagoma AI + Sfondo Catturato)",
                };
                self.apply_and_save_config(&format!(
                    "🧠 Motore Scontorno impostato su: {}",
                    m_desc
                ));
            }
            UiAction::AiBgTolDec => {
                self.cfg.segmentation.ai_selfie.bg_diff_tolerance =
                    (self.cfg.segmentation.ai_selfie.bg_diff_tolerance - 0.05).clamp(0.10, 0.95);
                let pct = (self.cfg.segmentation.ai_selfie.bg_diff_tolerance * 100.0).round();
                self.apply_and_save_config(&format!(
                    "🎚️ Tolleranza Luce/LED & Posizione: {:.0}%",
                    pct
                ));
            }
            UiAction::AiBgTolInc => {
                self.cfg.segmentation.ai_selfie.bg_diff_tolerance =
                    (self.cfg.segmentation.ai_selfie.bg_diff_tolerance + 0.05).clamp(0.10, 0.95);
                let pct = (self.cfg.segmentation.ai_selfie.bg_diff_tolerance * 100.0).round();
                self.apply_and_save_config(&format!(
                    "🎚️ Tolleranza Luce/LED & Posizione: {:.0}%",
                    pct
                ));
            }
            UiAction::Aspect16x9 => self.handle_action(hwnd, UiAction::SelectAspectRatio(0)),
            UiAction::Aspect4x3 => self.handle_action(hwnd, UiAction::SelectAspectRatio(1)),
            UiAction::Aspect1x1 => self.handle_action(hwnd, UiAction::SelectAspectRatio(2)),
            UiAction::Aspect9x16 => self.handle_action(hwnd, UiAction::SelectAspectRatio(3)),
            UiAction::AutoCalibrate1Click => {
                self.cfg.segmentation.mode = "green_screen".to_string();
                let (w, h) = (self.out_w, self.out_h);
                let frame_clone = self.pipeline.framed_buf.clone();
                let (hmin, hmax, smin, vmin) = self.pipeline.auto_calibrate_greenscreen(
                    &frame_clone,
                    w,
                    h,
                    -1,
                    -1,
                    &mut self.cfg,
                );
                let msg = format!(
                    "🪄 Auto-Calibrazione Telo Verde completata! Hue=[{}..{}], SatMin={}, ValMin={}",
                    hmin, hmax, smin, vmin
                );
                self.apply_and_save_config(&msg);
            }
            UiAction::UnlockWin11Privacy => {
                open_windows11_camera_privacy_settings();
                self.status_message =
                    "🔓 Aperte Impostazioni Privacy Fotocamera di Windows 11.".to_string();
            }
            UiAction::InstallRepairDriver => {
                self.status_message =
                    "🛡️ Richiesta autorizzazione UAC per registrare il driver SmartCam..."
                        .to_string();
                let (ok, msg) =
                    invoke_driver_install(&self.repo_root, &self.diag.active_build_variant);
                self.diag = collect_diagnostics(&self.repo_root, true);
                self.status_message = if ok {
                    "✅ Driver \"SmartCam\" registrato correttamente in Windows 11!".to_string()
                } else {
                    format!(
                        "⚠️ Registrazione driver: {}",
                        msg.lines().last().unwrap_or("annullata")
                    )
                };
            }
            UiAction::OpenSideBySideWeb => {
                let url = open_web_testbench(&self.repo_root);
                self.status_message = format!("🌐 Aperto Test Side-by-Side nel browser: {}", url);
            }
            UiAction::ToggleAutostart => {
                let target = !self.diag.autostart_enabled;
                if set_autostart_enabled(target) {
                    self.diag.autostart_enabled = target;
                    self.status_message = if target {
                        "🚀 Avvio automatico con Windows attivato (in System Tray).".to_string()
                    } else {
                        "Avvio automatico con Windows disattivato.".to_string()
                    };
                }
            }
            // Advanced Settings Fine-Tuning
            UiAction::HueMinDec => {
                self.cfg.segmentation.green_screen.hue_min =
                    self.cfg.segmentation.green_screen.hue_min.saturating_sub(2);
                self.apply_and_save_config("Aggiornato Hue Min.");
            }
            UiAction::HueMinInc => {
                let max = self.cfg.segmentation.green_screen.hue_max;
                self.cfg.segmentation.green_screen.hue_min =
                    (self.cfg.segmentation.green_screen.hue_min + 2).min(max);
                self.apply_and_save_config("Aggiornato Hue Min.");
            }
            UiAction::HueMaxDec => {
                let min = self.cfg.segmentation.green_screen.hue_min;
                self.cfg.segmentation.green_screen.hue_max = self
                    .cfg
                    .segmentation
                    .green_screen
                    .hue_max
                    .saturating_sub(2)
                    .max(min);
                self.apply_and_save_config("Aggiornato Hue Max.");
            }
            UiAction::HueMaxInc => {
                self.cfg.segmentation.green_screen.hue_max =
                    (self.cfg.segmentation.green_screen.hue_max + 2).min(180);
                self.apply_and_save_config("Aggiornato Hue Max.");
            }
            UiAction::SatMinDec => {
                self.cfg.segmentation.green_screen.sat_min =
                    self.cfg.segmentation.green_screen.sat_min.saturating_sub(5);
                self.apply_and_save_config("Aggiornato Sat Min.");
            }
            UiAction::SatMinInc => {
                self.cfg.segmentation.green_screen.sat_min =
                    self.cfg.segmentation.green_screen.sat_min.saturating_add(5);
                self.apply_and_save_config("Aggiornato Sat Min.");
            }
            UiAction::SatMaxDec => {
                self.cfg.segmentation.green_screen.sat_max = self
                    .cfg
                    .segmentation
                    .green_screen
                    .sat_max
                    .saturating_sub(5)
                    .max(50);
                self.apply_and_save_config("Aggiornato Sat Max.");
            }
            UiAction::SatMaxInc => {
                self.cfg.segmentation.green_screen.sat_max =
                    self.cfg.segmentation.green_screen.sat_max.saturating_add(5);
                self.apply_and_save_config("Aggiornato Sat Max.");
            }
            UiAction::ValMinDec => {
                self.cfg.segmentation.green_screen.val_min =
                    self.cfg.segmentation.green_screen.val_min.saturating_sub(5);
                self.apply_and_save_config("Aggiornato Val Min.");
            }
            UiAction::ValMinInc => {
                self.cfg.segmentation.green_screen.val_min =
                    self.cfg.segmentation.green_screen.val_min.saturating_add(5);
                self.apply_and_save_config("Aggiornato Val Min.");
            }
            UiAction::ValMaxDec => {
                self.cfg.segmentation.green_screen.val_max = self
                    .cfg
                    .segmentation
                    .green_screen
                    .val_max
                    .saturating_sub(5)
                    .max(50);
                self.apply_and_save_config("Aggiornato Val Max.");
            }
            UiAction::ValMaxInc => {
                self.cfg.segmentation.green_screen.val_max =
                    self.cfg.segmentation.green_screen.val_max.saturating_add(5);
                self.apply_and_save_config("Aggiornato Val Max.");
            }
            UiAction::MorphCycle => {
                self.cfg.segmentation.green_screen.morph_kernel_size =
                    match self.cfg.segmentation.green_screen.morph_kernel_size {
                        0 => 3,
                        3 => 5,
                        _ => 0,
                    };
                self.apply_and_save_config("Aggiornato Kernel Morfologia.");
            }
            UiAction::BlurDec => {
                self.cfg.segmentation.green_screen.blur_radius = self
                    .cfg
                    .segmentation
                    .green_screen
                    .blur_radius
                    .saturating_sub(1);
                self.apply_and_save_config("Aggiornata Morbidezza Bordi (Blur).");
            }
            UiAction::BlurInc => {
                self.cfg.segmentation.green_screen.blur_radius =
                    (self.cfg.segmentation.green_screen.blur_radius + 1).min(10);
                self.apply_and_save_config("Aggiornata Morbidezza Bordi (Blur).");
            }
            UiAction::SpillDec => {
                self.cfg.segmentation.green_screen.spill_suppression =
                    (self.cfg.segmentation.green_screen.spill_suppression - 0.05).clamp(0.0, 1.0);
                self.apply_and_save_config("Aggiornata Riduzione Alone Verde (Spill).");
            }
            UiAction::SpillInc => {
                self.cfg.segmentation.green_screen.spill_suppression =
                    (self.cfg.segmentation.green_screen.spill_suppression + 0.05).clamp(0.0, 1.0);
                self.apply_and_save_config("Aggiornata Riduzione Alone Verde (Spill).");
            }
            UiAction::AiConfDec => {
                self.cfg.segmentation.ai_selfie.confidence_threshold =
                    (self.cfg.segmentation.ai_selfie.confidence_threshold - 0.05).clamp(0.10, 0.90);
                self.apply_and_save_config("Aggiornata Soglia Confidenza AI.");
            }
            UiAction::AiConfInc => {
                self.cfg.segmentation.ai_selfie.confidence_threshold =
                    (self.cfg.segmentation.ai_selfie.confidence_threshold + 0.05).clamp(0.10, 0.90);
                self.apply_and_save_config("Aggiornata Soglia Confidenza AI.");
            }
            UiAction::AiSmoothDec => {
                self.cfg.segmentation.ai_selfie.temporal_smoothing =
                    (self.cfg.segmentation.ai_selfie.temporal_smoothing - 0.05).clamp(0.0, 0.85);
                self.apply_and_save_config("Aggiornato Filtro Temporale EMA AI.");
            }
            UiAction::AiSmoothInc => {
                self.cfg.segmentation.ai_selfie.temporal_smoothing =
                    (self.cfg.segmentation.ai_selfie.temporal_smoothing + 0.05).clamp(0.0, 0.85);
                self.apply_and_save_config("Aggiornato Filtro Temporale EMA AI.");
            }
            UiAction::BgColorPresetCycle => {
                self.cfg.compositor.background_mode = "solid_color".to_string();
                let presets: [[u8; 3]; 5] = [
                    [24, 34, 56], // Blu Ufficio Notte
                    [30, 64, 52], // Verde Studio Elegante
                    [48, 38, 68], // Viola Podcast
                    [65, 72, 82], // Grigio Neutro Professionale
                    [15, 23, 42], // Ardesia Scuro
                ];
                let cur = self.cfg.compositor.background_color_rgb;
                let idx = presets.iter().position(|&c| c == cur).unwrap_or(0);
                self.cfg.compositor.background_color_rgb = presets[(idx + 1) % presets.len()];
                self.apply_and_save_config("🎨 Cambiato Colore Sfondo Virtuale RGB.");
            }
            UiAction::BgBlurDec => {
                self.cfg.compositor.background_blur_radius = self
                    .cfg
                    .compositor
                    .background_blur_radius
                    .saturating_sub(2)
                    .max(3);
                self.apply_and_save_config("Aggiornata Intensità Sfocatura Sfondo.");
            }
            UiAction::BgBlurInc => {
                self.cfg.compositor.background_blur_radius =
                    (self.cfg.compositor.background_blur_radius + 2).min(45);
                self.apply_and_save_config("Aggiornata Intensità Sfocatura Sfondo.");
            }
            UiAction::SimdVariantCycle => {
                let next_simd = match self.cfg.compositor.simd_acceleration.as_str() {
                    "auto" => "avx512",
                    "avx512" => "avx2",
                    "avx2" => "scalar",
                    _ => "auto",
                };
                self.cfg.compositor.simd_acceleration = next_simd.to_string();
                self.pipeline.apply_hardware_policy(&self.cfg);
                let label = self.pipeline.simd_backend_label.clone();
                self.apply_and_save_config(&format!("⚡ Motore SIMD impostato su: {}", label));
            }
            UiAction::SaveConfigYaml => {
                self.apply_and_save_config(
                    "💾 Configurazione salvata in config/default_config.yaml!",
                );
            }
        }
    }

    pub fn finish_wizard(&mut self) {
        self.wizard_step = 0;
        let wizard_flag = self.repo_root.join("config").join(".wizard_completed");
        let _ = fs::write(wizard_flag, "1");
        self.status_message =
            "✅ Guida completata! Seleziona \"SmartCam\" nelle impostazioni video di Zoom, Teams, Meet o WhatsApp."
                .to_string();
    }
}

fn extract_port_from_url_str(s: &str) -> Option<u16> {
    let after_scheme = if let Some((_, rest)) = s.split_once("://") {
        rest
    } else {
        s
    };
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    let (_, port_part) = host_port.rsplit_once(':')?;
    let digits: String = port_part
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let p: u16 = digits.parse().ok()?;
    if p >= 1024 {
        Some(p)
    } else {
        None
    }
}

// ============================================================================
// Multi-Icon Theme Loader (Resource ID 101/102/103 + .ico File + Procedural Fallback)
// ============================================================================

unsafe fn create_procedural_theme_icon(theme: &str, size: i32) -> HICON {
    let (ring_r, ring_g, ring_b) = match theme {
        "emerald_cam" => (16u8, 185u8, 129u8),
        "neon_iris" => (217u8, 70u8, 239u8),
        _ => (56u8, 189u8, 248u8),
    };

    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let c = (size as f32 - 1.0) * 0.5;
    let r_outer = c * 0.92;
    let r_ring = c * 0.68;
    let r_core = c * 0.36;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let dist = (dx * dx + dy * dy).sqrt();
            let off = ((y * size + x) * 4) as usize;
            if dist > r_outer {
                pixels[off] = 0;
                pixels[off + 1] = 0;
                pixels[off + 2] = 0;
                pixels[off + 3] = 0;
            } else if dist > r_ring {
                pixels[off] = 38;
                pixels[off + 1] = 24;
                pixels[off + 2] = 15;
                pixels[off + 3] = 255;
            } else if dist > r_core {
                pixels[off] = ring_b;
                pixels[off + 1] = ring_g;
                pixels[off + 2] = ring_r;
                pixels[off + 3] = 255;
            } else {
                pixels[off] = 250;
                pixels[off + 1] = 245;
                pixels[off + 2] = 240;
                pixels[off + 3] = 255;
            }
        }
    }

    let hbm_color = CreateBitmap(size, size, 1, 32, pixels.as_ptr() as *const c_void);
    if hbm_color.is_null() {
        return LoadIconW(NULL, IDI_APPLICATION);
    }

    let mask_bytes = vec![0u8; (((size + 15) / 16) * 2 * size) as usize];
    let hbm_mask = CreateBitmap(size, size, 1, 1, mask_bytes.as_ptr() as *const c_void);

    let mut ii = ICONINFO {
        fIcon: TRUE,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbm_mask,
        hbmColor: hbm_color,
    };
    let hicon = CreateIconIndirect(&mut ii);
    DeleteObject(hbm_color);
    if !hbm_mask.is_null() {
        DeleteObject(hbm_mask);
    }
    if hicon.is_null() {
        LoadIconW(NULL, IDI_APPLICATION)
    } else {
        hicon
    }
}

const fn make_int_resource(id: u16) -> *const u16 {
    id as usize as *const u16
}

pub unsafe fn load_theme_icon(repo_root: &Path, theme: &str, size: i32) -> HICON {
    let (res_id, filename) = match theme {
        "emerald_cam" => (102u16, "smartcam_emerald_cam.ico"),
        "neon_iris" => (103u16, "smartcam_neon_iris.ico"),
        _ => (101u16, "smartcam_cyber_lens.ico"),
    };

    let hinst = GetModuleHandleW(std::ptr::null());
    let h_res = LoadImageW(hinst, make_int_resource(res_id), IMAGE_ICON, size, size, 0);
    if !h_res.is_null() {
        return h_res;
    }

    let ico_path = repo_root.join("assets").join("icons").join(filename);
    if ico_path.is_file() {
        let wpath = to_wstring(&ico_path.to_string_lossy());
        let h_file = LoadImageW(
            NULL,
            wpath.as_ptr(),
            IMAGE_ICON,
            size,
            size,
            LR_LOADFROMFILE,
        );
        if !h_file.is_null() {
            return h_file;
        }
    }

    create_procedural_theme_icon(theme, size)
}

unsafe fn apply_window_and_tray_icon(hwnd: HWND, app: &mut AppGuiState) {
    let old_big = app.current_hicon_big;
    let old_small = app.current_hicon_small;

    let big = load_theme_icon(&app.repo_root, &app.cfg.app_icon_theme, 32);
    let small = load_theme_icon(&app.repo_root, &app.cfg.app_icon_theme, 16);

    SendMessageW(hwnd, WM_SETICON, ICON_BIG, big as LPARAM);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small as LPARAM);

    app.current_hicon_big = big;
    app.current_hicon_small = small;

    if !old_big.is_null() && old_big != big {
        DestroyIcon(old_big);
    }
    if !old_small.is_null() && old_small != small {
        DestroyIcon(old_small);
    }

    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as DWORD;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    nid.uFlags = NIF_ICON | NIF_TIP;
    nid.hIcon = small;
    let tip = to_wstring("SmartCam — Attiva vicino all'orologio");
    for (i, &ch) in tip.iter().take(127).enumerate() {
        nid.szTip[i] = ch;
    }
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

// ============================================================================
// System Tray Icon & Balloon Helpers
// ============================================================================

unsafe fn register_tray_icon(hwnd: HWND, app: &mut AppGuiState) {
    let old_big = app.current_hicon_big;
    let old_small = app.current_hicon_small;
    let small = load_theme_icon(&app.repo_root, &app.cfg.app_icon_theme, 16);
    let big = load_theme_icon(&app.repo_root, &app.cfg.app_icon_theme, 32);
    SendMessageW(hwnd, WM_SETICON, ICON_BIG, big as LPARAM);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small as LPARAM);
    app.current_hicon_big = big;
    app.current_hicon_small = small;
    if !old_big.is_null() && old_big != big {
        DestroyIcon(old_big);
    }
    if !old_small.is_null() && old_small != small {
        DestroyIcon(old_small);
    }

    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as DWORD;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAYICON;
    nid.hIcon = small;
    let tip = to_wstring("SmartCam — Attiva vicino all'orologio");
    for (i, &ch) in tip.iter().take(127).enumerate() {
        nid.szTip[i] = ch;
    }
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as DWORD;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

fn minimize_window_to_tray(hwnd: HWND, app: &mut AppGuiState) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        app.minimized_to_tray = true;
        if !app.tray_balloon_shown {
            app.tray_balloon_shown = true;
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as DWORD;
            nid.hWnd = hwnd;
            nid.uID = TRAY_ICON_ID;
            nid.uFlags = NIF_INFO;
            nid.dwInfoFlags = NIIF_INFO;
            let title = to_wstring("SmartCam è attiva vicino all'orologio");
            for (i, &c) in title.iter().take(63).enumerate() {
                nid.szInfoTitle[i] = c;
            }
            let info = to_wstring(
                "Modalità On-Demand attiva: quando nessuna app video usa \"SmartCam\", consuma 0% CPU e spegne il LED.",
            );
            for (i, &c) in info.iter().take(255).enumerate() {
                nid.szInfo[i] = c;
            }
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }
}

unsafe fn show_tray_context_menu(hwnd: HWND, app: &mut AppGuiState) {
    let hmenu = CreatePopupMenu();
    if hmenu.is_null() {
        return;
    }

    let t_open = to_wstring("📷 Apri Studio SmartCam");
    AppendMenuW(hmenu, MF_STRING, CMD_TRAY_OPEN, t_open.as_ptr());
    let t_plug = to_wstring("🧩 Gestione Plugin (Fotocamera Smartphone & Filtri)...");
    AppendMenuW(hmenu, MF_STRING, CMD_TRAY_PLUGINS, t_plug.as_ptr());
    let t_bench = to_wstring("⚡ Centro Prestazioni, GPU/CPU, AVX & Benchmark...");
    AppendMenuW(hmenu, MF_STRING, CMD_TRAY_BENCHMARK, t_bench.as_ptr());
    let t_run_bench = to_wstring("🚀 Riesegui Benchmark Hardware Ora");
    AppendMenuW(
        hmenu,
        MF_STRING,
        CMD_TRAY_RUN_BENCHMARK_NOW,
        t_run_bench.as_ptr(),
    );
    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());

    let is_orig = app.cfg.segmentation.mode == "passthrough";
    let is_blur =
        app.cfg.segmentation.mode == "ai_selfie" && app.cfg.compositor.background_mode == "blur";
    let is_ai =
        app.cfg.segmentation.mode == "ai_selfie" && app.cfg.compositor.background_mode != "blur";
    let is_gs = app.cfg.segmentation.mode == "green_screen";

    let t_orig = to_wstring("🎥 Modalità: Originale");
    AppendMenuW(
        hmenu,
        MF_STRING | if is_orig { MF_CHECKED } else { 0 },
        CMD_TRAY_MODE_ORIG,
        t_orig.as_ptr(),
    );
    let t_blur = to_wstring("💧 Modalità: Sfoca Sfondo");
    AppendMenuW(
        hmenu,
        MF_STRING | if is_blur { MF_CHECKED } else { 0 },
        CMD_TRAY_MODE_BLUR,
        t_blur.as_ptr(),
    );
    let t_ai = to_wstring("🤖 Modalità: Sfondo Virtuale");
    AppendMenuW(
        hmenu,
        MF_STRING | if is_ai { MF_CHECKED } else { 0 },
        CMD_TRAY_MODE_AI,
        t_ai.as_ptr(),
    );
    let t_gs = to_wstring("🟩 Modalità: Telo Verde (Green Screen)");
    AppendMenuW(
        hmenu,
        MF_STRING | if is_gs { MF_CHECKED } else { 0 },
        CMD_TRAY_MODE_GREEN,
        t_gs.as_ptr(),
    );

    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
    let ic = app.cfg.app_icon_theme.as_str();
    let t_ic1 = to_wstring("🎨 Icona: Cyber Lens (Ciano)");
    AppendMenuW(
        hmenu,
        MF_STRING | if ic == "cyber_lens" { MF_CHECKED } else { 0 },
        CMD_TRAY_ICON_CYBER,
        t_ic1.as_ptr(),
    );
    let t_ic2 = to_wstring("🎨 Icona: Emerald Cam (Smeraldo)");
    AppendMenuW(
        hmenu,
        MF_STRING | if ic == "emerald_cam" { MF_CHECKED } else { 0 },
        CMD_TRAY_ICON_EMERALD,
        t_ic2.as_ptr(),
    );
    let t_ic3 = to_wstring("🎨 Icona: Neon Iris (Viola/Neon)");
    AppendMenuW(
        hmenu,
        MF_STRING | if ic == "neon_iris" { MF_CHECKED } else { 0 },
        CMD_TRAY_ICON_NEON,
        t_ic3.as_ptr(),
    );

    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
    let t_auto = to_wstring("🚀 Avvia automaticamente con Windows");
    AppendMenuW(
        hmenu,
        MF_STRING
            | if app.diag.autostart_enabled {
                MF_CHECKED
            } else {
                0
            },
        CMD_TRAY_AUTOSTART,
        t_auto.as_ptr(),
    );
    let t_pefl = to_wstring("📜 Licenza PEFL v1.0 (Clausola 3)...");
    AppendMenuW(hmenu, MF_STRING, CMD_TRAY_PEFL, t_pefl.as_ptr());

    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
    let t_exit = to_wstring("❌ Esci Completamente da SmartCam");
    AppendMenuW(hmenu, MF_STRING, CMD_TRAY_EXIT, t_exit.as_ptr());

    let mut pt: POINT = Default::default();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        hmenu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    ) as usize;
    DestroyMenu(hmenu);

    match cmd {
        CMD_TRAY_OPEN => {
            app.minimized_to_tray = false;
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
        }
        CMD_TRAY_PLUGINS => {
            app.minimized_to_tray = false;
            app.handle_action(hwnd, UiAction::OpenPluginsModal);
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
        }
        CMD_TRAY_BENCHMARK => {
            app.minimized_to_tray = false;
            app.handle_action(hwnd, UiAction::OpenPerfModal);
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
        }
        CMD_TRAY_RUN_BENCHMARK_NOW => {
            app.minimized_to_tray = false;
            app.handle_action(hwnd, UiAction::OpenPerfModal);
            app.handle_action(hwnd, UiAction::RunHardwareBenchmarkNow);
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
        }
        CMD_TRAY_PEFL => {
            app.minimized_to_tray = false;
            app.pefl_modal_open = true;
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
        }
        CMD_TRAY_MODE_ORIG => app.handle_action(hwnd, UiAction::StyleOriginal),
        CMD_TRAY_MODE_BLUR => app.handle_action(hwnd, UiAction::StyleBlur),
        CMD_TRAY_MODE_AI => app.handle_action(hwnd, UiAction::StyleAiOffice),
        CMD_TRAY_MODE_GREEN => app.handle_action(hwnd, UiAction::StyleGreenScreen),
        CMD_TRAY_ICON_CYBER => app.handle_action(hwnd, UiAction::SetIconTheme(0)),
        CMD_TRAY_ICON_EMERALD => app.handle_action(hwnd, UiAction::SetIconTheme(1)),
        CMD_TRAY_ICON_NEON => app.handle_action(hwnd, UiAction::SetIconTheme(2)),
        CMD_TRAY_AUTOSTART => app.handle_action(hwnd, UiAction::ToggleAutostart),
        CMD_TRAY_EXIT => {
            remove_tray_icon(hwnd);
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

// ============================================================================
// Custom Double-Buffered Win32 GDI Painting
// ============================================================================

unsafe fn draw_button(
    hdc: HDC,
    rc: RECT,
    label: &str,
    active: bool,
    accent_color: DWORD,
    hover: bool,
    hfont: HFONT,
) {
    let fill_col = if active {
        accent_color
    } else if hover {
        rgb(51, 65, 85)
    } else {
        rgb(30, 41, 59)
    };
    let border_col = if active {
        rgb(125, 211, 252)
    } else if hover {
        rgb(100, 116, 139)
    } else {
        rgb(51, 65, 85)
    };

    let brush = CreateSolidBrush(fill_col);
    FillRect(hdc, &rc, brush);
    DeleteObject(brush);

    let b_brush = CreateSolidBrush(border_col);
    FrameRect(hdc, &rc, b_brush);
    DeleteObject(b_brush);

    let old_font = SelectObject(hdc, hfont);
    SetTextColor(
        hdc,
        if active {
            rgb(255, 255, 255)
        } else {
            rgb(226, 232, 240)
        },
    );
    let mut text_rc = rc;
    let wlabel = to_wstring(label);
    DrawTextW(
        hdc,
        wlabel.as_ptr(),
        -1,
        &mut text_rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old_font);
}

unsafe fn draw_text_left(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    color: DWORD,
    hfont: HFONT,
) {
    let old = SelectObject(hdc, hfont);
    SetTextColor(hdc, color);
    let mut rc = RECT::new(x, y, x + w, y + h);
    let wt = to_wstring(text);
    DrawTextW(
        hdc,
        wt.as_ptr(),
        -1,
        &mut rc,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old);
}

unsafe fn draw_text_center(hdc: HDC, rc: RECT, text: &str, color: DWORD, hfont: HFONT) {
    let old = SelectObject(hdc, hfont);
    SetTextColor(hdc, color);
    let mut r = rc;
    let wt = to_wstring(text);
    DrawTextW(
        hdc,
        wt.as_ptr(),
        -1,
        &mut r,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old);
}

unsafe fn register_button(
    hdc: HDC,
    app: &mut AppGuiState,
    rc: RECT,
    label: &str,
    action: UiAction,
    active: bool,
    accent: DWORD,
    hfont: HFONT,
) {
    let hover = rc.contains(app.hover_pos.x, app.hover_pos.y);
    draw_button(hdc, rc, label, active, accent, hover, hfont);
    app.hit_zones.push(HitZone { rect: rc, action });
}

unsafe fn draw_multiline_text(hdc: HDC, mut rc: RECT, text: &str, color: DWORD, hfont: HFONT) {
    let old = SelectObject(hdc, hfont);
    SetTextColor(hdc, color);
    let wt = to_wstring(text);
    DrawTextW(
        hdc,
        wt.as_ptr(),
        -1,
        &mut rc,
        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
    );
    SelectObject(hdc, old);
}

unsafe fn paint_main_window(hwnd: HWND, app: &mut AppGuiState) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc_win = BeginPaint(hwnd, &mut ps);

    let mut client_rc: RECT = Default::default();
    GetClientRect(hwnd, &mut client_rc);
    let win_w = client_rc.width().max(100);
    let win_h = client_rc.height().max(100);

    if app.backbuf_dc.is_null()
        || app.backbuf_bmp.is_null()
        || app.backbuf_w != win_w
        || app.backbuf_h != win_h
    {
        if !app.backbuf_dc.is_null() {
            if !app.backbuf_old_bmp.is_null() {
                SelectObject(app.backbuf_dc, app.backbuf_old_bmp);
                app.backbuf_old_bmp = std::ptr::null_mut();
            }
            if !app.backbuf_bmp.is_null() {
                DeleteObject(app.backbuf_bmp);
                app.backbuf_bmp = std::ptr::null_mut();
            }
            DeleteDC(app.backbuf_dc);
        }
        app.backbuf_dc = CreateCompatibleDC(hdc_win);
        app.backbuf_bmp = CreateCompatibleBitmap(hdc_win, win_w, win_h);
        app.backbuf_old_bmp = SelectObject(app.backbuf_dc, app.backbuf_bmp);
        app.backbuf_w = win_w;
        app.backbuf_h = win_h;
    }
    let mem_dc = app.backbuf_dc;

    if app.font_title.is_null() {
        let fname = to_wstring("Segoe UI");
        app.font_title = CreateFontW(-19, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());
        app.font_sub = CreateFontW(-14, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());
        app.font_btn = CreateFontW(-13, 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());
        app.font_small = CreateFontW(-12, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());
    }
    let f_title = app.font_title;
    let f_sub = app.font_sub;
    let f_btn = app.font_btn;
    let f_small = app.font_small;

    app.hit_zones.clear();
    app.dropdown_overlay_zones.clear();

    // Background Fill
    let bg_main = CreateSolidBrush(rgb(11, 15, 25));
    FillRect(mem_dc, &client_rc, bg_main);
    DeleteObject(bg_main);

    SetBkMode(mem_dc, TRANSPARENT);

    // ========================================================================
    // 1. Top Header Bar (`y = 0..48`, Zero Overlap)
    // ========================================================================
    let header_rc = RECT::new(0, 0, win_w, 48);
    let header_br = CreateSolidBrush(rgb(18, 25, 40));
    FillRect(mem_dc, &header_rc, header_br);
    DeleteObject(header_br);

    draw_text_left(
        mem_dc,
        18,
        5,
        215,
        38,
        "📹 SmartCam Studio",
        rgb(248, 250, 252),
        f_title,
    );

    // Compact 3-Icon Theme Selector Pills in Header (`x = 236..458`)
    let cur_icon = app.cfg.app_icon_theme.clone();
    register_button(
        mem_dc,
        app,
        RECT::new(236, 10, 306, 38),
        "💠 Cyber",
        UiAction::SetIconTheme(0),
        cur_icon == "cyber_lens",
        rgb(2, 132, 199),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(310, 10, 388, 38),
        "🟢 Emerald",
        UiAction::SetIconTheme(1),
        cur_icon == "emerald_cam",
        rgb(16, 185, 129),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(392, 10, 462, 38),
        "🟣 Neon",
        UiAction::SetIconTheme(2),
        cur_icon == "neon_iris",
        rgb(192, 38, 211),
        f_small,
    );

    // Right Navigation Buttons in Header (`win_w - 690 .. win_w - 16`)
    let active_plugs = app.plugin_registry.enabled_count();
    let plug_btn_lbl = if active_plugs > 0 {
        format!("🧩 Plugin ({} attivi)", active_plugs)
    } else {
        "🧩 Plugin & Smartphone".to_string()
    };
    register_button(
        mem_dc,
        app,
        RECT::new(win_w - 690, 8, win_w - 524, 40),
        &plug_btn_lbl,
        UiAction::OpenPluginsModal,
        app.plugins_modal_open || active_plugs > 0,
        rgb(13, 148, 136),
        f_btn,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(win_w - 518, 8, win_w - 332, 40),
        "⚡ Prestazioni & Benchmark",
        UiAction::OpenPerfModal,
        app.perf_modal_open,
        rgb(2, 132, 199),
        f_btn,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(win_w - 326, 8, win_w - 208, 40),
        "⚙️ Calibrazione",
        UiAction::ToggleAdvanced,
        app.show_advanced,
        rgb(79, 70, 229),
        f_btn,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(win_w - 202, 8, win_w - 134, 40),
        "🧭 Guida",
        UiAction::OpenWizard,
        app.wizard_step > 0,
        rgb(14, 116, 144),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(win_w - 128, 8, win_w - 68, 40),
        "📜 PEFL",
        UiAction::OpenPeflModal,
        app.pefl_modal_open,
        rgb(185, 28, 28),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(win_w - 62, 8, win_w - 14, 40),
        "🔽 Tray",
        UiAction::MinimizeToTray,
        false,
        rgb(51, 65, 85),
        f_small,
    );

    // ========================================================================
    // 2. Camera Source Bar (`y = 54..90`)
    // ========================================================================
    let cam_bar_rc = RECT::new(14, 54, win_w - 14, 90);
    let cam_br = CreateSolidBrush(rgb(20, 29, 46));
    FillRect(mem_dc, &cam_bar_rc, cam_br);
    DeleteObject(cam_br);

    let phone_cam_active = app
        .plugin_registry
        .is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA);
    let cam_label = if phone_cam_active {
        format!(
            "📱 Fotocamera Smartphone ({})",
            app.plugin_registry.phone_camera_endpoint()
        )
    } else {
        app.selected_camera_entry()
            .map(|c| c.display_label())
            .unwrap_or_else(|| "[0] Webcam Principale HD".to_string())
    };

    draw_text_left(
        mem_dc,
        24,
        56,
        85,
        32,
        "Sorgente:",
        rgb(148, 163, 184),
        f_btn,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(104, 58, 138, 86),
        "◀",
        UiAction::PrevCamera,
        false,
        rgb(51, 65, 85),
        f_btn,
    );
    draw_text_left(
        mem_dc,
        146,
        56,
        355,
        32,
        &cam_label,
        if phone_cam_active {
            rgb(74, 222, 128)
        } else if app.is_selected_camera_ir() {
            rgb(253, 224, 71)
        } else {
            rgb(56, 189, 248)
        },
        f_sub,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(506, 58, 540, 86),
        "▶",
        UiAction::NextCamera,
        false,
        rgb(51, 65, 85),
        f_btn,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(546, 58, 642, 86),
        "🔄 Aggiorna",
        UiAction::RescanCameras,
        false,
        rgb(51, 65, 85),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(648, 58, 804, 86),
        if phone_cam_active {
            "📱 Smartphone: ON"
        } else {
            "📱 Usa Smartphone"
        },
        UiAction::QuickTogglePhoneCamera,
        phone_cam_active,
        rgb(13, 148, 136),
        f_small,
    );
    let is_64bit_src = app.cfg.camera_input.bit_depth >= 64;
    register_button(
        mem_dc,
        app,
        RECT::new(810, 58, 954, 86),
        if is_64bit_src {
            "🎨 Sorgente: 64-bit"
        } else {
            "🎨 Sorgente: 32-bit"
        },
        UiAction::ToggleSourceBitDepth,
        is_64bit_src,
        rgb(124, 58, 237),
        f_small,
    );

    let drv_badge = if app.diag.clsid_registered {
        "✅ Virtual Cam \"SmartCam\": PRONTA"
    } else {
        "⚠️ Virtual Cam: Da Registrare"
    };
    draw_text_left(
        mem_dc,
        962,
        56,
        win_w - 972,
        32,
        drv_badge,
        if app.diag.clsid_registered {
            rgb(74, 222, 128)
        } else {
            rgb(251, 191, 36)
        },
        f_small,
    );

    // ========================================================================
    // 3. Central Studio Live Video Monitor (`y = 96..490`, Fixed Coordinates)
    // ========================================================================
    let monitor_rc = RECT::new(14, 96, win_w - 14, 490);
    let mon_br = CreateSolidBrush(rgb(7, 10, 17));
    FillRect(mem_dc, &monitor_rc, mon_br);
    DeleteObject(mon_br);
    let mon_bd = CreateSolidBrush(rgb(30, 41, 59));
    FrameRect(mem_dc, &monitor_rc, mon_bd);
    DeleteObject(mon_bd);

    // Fit aspect ratio inside `monitor_rc` (leaving 28px at bottom for telemetry bar)
    let fw = app.out_w.max(160) as i32;
    let fh = app.out_h.max(120) as i32;
    let avail_w = monitor_rc.width() - 24;
    let avail_h = monitor_rc.height() - 36;
    let (draw_w, draw_h) = if fw * avail_h > fh * avail_w {
        (avail_w, (avail_w * fh) / fw.max(1))
    } else {
        ((avail_h * fw) / fh.max(1), avail_h)
    };
    let vx = monitor_rc.left + (monitor_rc.width() - draw_w) / 2;
    let vy = monitor_rc.top + 4 + (avail_h - draw_h) / 2;
    app.video_preview_rect = RECT::new(vx, vy, vx + draw_w, vy + draw_h);

    let buf_to_draw = if !app.preview_buf.is_empty() {
        &app.preview_buf
    } else {
        &app.pipeline.out_buf
    };

    if !buf_to_draw.is_empty() && app.out_w > 0 && app.out_h > 0 {
        let mut bmi: BITMAPINFO = Default::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as DWORD;
        bmi.bmiHeader.biWidth = app.out_w as LONG;
        bmi.bmiHeader.biHeight = -(app.out_h as LONG); // Top-down BGRA32
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        SetStretchBltMode(mem_dc, COLORONCOLOR);
        StretchDIBits(
            mem_dc,
            vx,
            vy,
            draw_w,
            draw_h,
            0,
            0,
            app.out_w as i32,
            app.out_h as i32,
            buf_to_draw.as_ptr() as *const c_void,
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }

    let vborder = CreateSolidBrush(rgb(56, 189, 248));
    FrameRect(mem_dc, &app.video_preview_rect, vborder);
    DeleteObject(vborder);

    // Contextual 1-Line Warning Banner inside Top of Monitor (Zero Button Push!)
    let has_ref = app.pipeline.has_reference_background();
    let show_static_warn = app.bg_capture_countdown_end.is_some()
        || has_ref
        || app.cfg.segmentation.ai_selfie.engine_mode == 2;

    if app.is_selected_camera_ir() {
        let ir_rc = RECT::new(
            monitor_rc.left + 12,
            monitor_rc.top + 6,
            monitor_rc.right - 12,
            monitor_rc.top + 32,
        );
        let ir_br = CreateSolidBrush(rgb(113, 63, 18));
        FillRect(mem_dc, &ir_rc, ir_br);
        DeleteObject(ir_br);
        let ir_border = CreateSolidBrush(rgb(250, 204, 21));
        FrameRect(mem_dc, &ir_rc, ir_border);
        DeleteObject(ir_border);
        draw_text_center(
            mem_dc,
            ir_rc,
            "⚠️ Sensore Infrarossi (Windows Hello) selezionato: l'immagine può apparire B/N. Usa ◀ / ▶ per la webcam a colori.",
            rgb(254, 240, 138),
            f_small,
        );
    } else if show_static_warn && app.bg_capture_countdown_end.is_none() {
        let warn_rc = RECT::new(
            monitor_rc.left + 12,
            monitor_rc.top + 6,
            monitor_rc.right - 12,
            monitor_rc.top + 32,
        );
        let w_fill = CreateSolidBrush(rgb(20, 63, 40));
        FillRect(mem_dc, &warn_rc, w_fill);
        DeleteObject(w_fill);
        let w_bd = CreateSolidBrush(rgb(74, 222, 128));
        FrameRect(mem_dc, &warn_rc, w_bd);
        DeleteObject(w_bd);
        draw_text_center(
            mem_dc,
            warn_rc,
            "✓ Telo Virtuale Attivo (Sfondo Stanza Catturato) — ⚠️ Poggia notebook/webcam su una superficie statica e ferma.",
            rgb(220, 252, 231),
            f_small,
        );
    }

    // 3-Second Countdown Overlay for Virtual Green Screen (Real Background Capture)
    if let Some(deadline) = app.bg_capture_countdown_end {
        let rem_secs = deadline.saturating_duration_since(Instant::now()).as_secs() + 1;
        let banner_rc = RECT::new(
            app.video_preview_rect.left + 20,
            app.video_preview_rect.top + 20,
            app.video_preview_rect.right - 20,
            app.video_preview_rect.top + 118,
        );
        let b_fill = CreateSolidBrush(rgb(15, 23, 42));
        FillRect(mem_dc, &banner_rc, b_fill);
        DeleteObject(b_fill);
        let b_border = CreateSolidBrush(rgb(250, 204, 21));
        FrameRect(mem_dc, &banner_rc, b_border);
        DeleteObject(b_border);

        draw_text_center(
            mem_dc,
            RECT::new(
                banner_rc.left + 8,
                banner_rc.top + 6,
                banner_rc.right - 8,
                banner_rc.top + 32,
            ),
            "📸 Spostati dall'inquadratura per creare il tuo Telo Virtuale...",
            rgb(254, 240, 138),
            f_sub,
        );
        draw_text_center(
            mem_dc,
            RECT::new(
                banner_rc.left + 8,
                banner_rc.top + 32,
                banner_rc.right - 8,
                banner_rc.top + 66,
            ),
            &format!("Cattura Sfondo Reale tra {}...", rem_secs.min(3)),
            rgb(56, 189, 248),
            f_title,
        );
        draw_text_center(
            mem_dc,
            RECT::new(banner_rc.left + 8, banner_rc.top + 68, banner_rc.right - 8, banner_rc.bottom - 6),
            "⚠️ Assicurati che il notebook o la webcam siano su una superficie statica e ferma (non sulle gambe).",
            rgb(251, 191, 36),
            f_small,
        );
    }

    // Bottom Telemetry Pill inside Studio Monitor (`y = 464..488`)
    let (scx, scy) = app.pipeline.get_subject_center();
    let ai_state_str = if app.cfg.ai_enabled {
        "AI: ON"
    } else {
        "AI: OFF (Ultra-Light)"
    };
    let sil_str = if app.cfg.segmentation.ai_selfie.silhouette_mode == 1 {
        "Solo Volto"
    } else {
        "Volto+Busto"
    };
    let overlay_text = format!(
        "● LIVE {:4.1} FPS (Target {} FPS)  •  {}×{} ({} • {})  •  Motore: {}  •  {} ({})  •  Soggetto: ({:.0}%, {:.0}%)",
        app.current_fps,
        app.cfg.camera_input.fps,
        app.out_w,
        app.out_h,
        app.cfg.framing_output.aspect_ratio,
        app.cfg.framing_output.resolution_preset,
        app.pipeline.simd_backend_label,
        ai_state_str,
        sil_str,
        scx * 100.0,
        scy * 100.0
    );
    draw_text_center(
        mem_dc,
        RECT::new(
            monitor_rc.left + 12,
            monitor_rc.bottom - 26,
            monitor_rc.right - 12,
            monitor_rc.bottom - 2,
        ),
        &overlay_text,
        rgb(148, 163, 184),
        f_small,
    );

    // ========================================================================
    // 4. Quick Controls Panel Below Video (`y = 494..634`, 3 Clean Rows, Zero Overlap)
    // ========================================================================
    let ctrl_card = RECT::new(14, 494, win_w - 14, 634);
    let cc_br = CreateSolidBrush(rgb(20, 29, 46));
    FillRect(mem_dc, &ctrl_card, cc_br);
    DeleteObject(cc_br);
    let cc_bd = CreateSolidBrush(rgb(38, 52, 77));
    FrameRect(mem_dc, &ctrl_card, cc_bd);
    DeleteObject(cc_bd);

    let is_orig = app.cfg.segmentation.mode == "passthrough";
    let is_blur =
        app.cfg.segmentation.mode == "ai_selfie" && app.cfg.compositor.background_mode == "blur";
    let is_ai_virtual =
        app.cfg.segmentation.mode == "ai_selfie" && app.cfg.compositor.background_mode != "blur";
    let is_green = app.cfg.segmentation.mode == "green_screen";

    // Row 1 (`y = 500..536`, height 36): 4 Style Buttons + Gallery/Crop + 3s Room Capture
    let r1_y0 = 500;
    let r1_y1 = 536;
    let total_inner_w = win_w - 48;
    let style_btn_w = (total_inner_w * 13) / 100; // 4 buttons = 52% width
    let mut rx = 24;

    register_button(
        mem_dc,
        app,
        RECT::new(rx, r1_y0, rx + style_btn_w, r1_y1),
        "🎥 Originale",
        UiAction::StyleOriginal,
        is_orig,
        rgb(2, 132, 199),
        f_btn,
    );
    rx += style_btn_w + 6;

    register_button(
        mem_dc,
        app,
        RECT::new(rx, r1_y0, rx + style_btn_w, r1_y1),
        "💧 Sfoca Sfondo",
        UiAction::StyleBlur,
        is_blur,
        rgb(2, 132, 199),
        f_btn,
    );
    rx += style_btn_w + 6;

    register_button(
        mem_dc,
        app,
        RECT::new(rx, r1_y0, rx + style_btn_w + 14, r1_y1),
        "🤖 Sfondo Virtuale",
        UiAction::StyleAiOffice,
        is_ai_virtual,
        rgb(2, 132, 199),
        f_btn,
    );
    rx += style_btn_w + 20;

    register_button(
        mem_dc,
        app,
        RECT::new(rx, r1_y0, rx + style_btn_w, r1_y1),
        "🟩 Telo Verde",
        UiAction::StyleGreenScreen,
        is_green,
        rgb(22, 163, 74),
        f_btn,
    );
    rx += style_btn_w + 8;

    // Unified Background Gallery & Social Crop Button
    let active_bg_short = app.active_background_short_name();
    let gal_btn_w = (total_inner_w * 23) / 100;
    register_button(
        mem_dc,
        app,
        RECT::new(rx, r1_y0, rx + gal_btn_w, r1_y1),
        &format!("🖼️ Scegli / Ritaglia Sfondo [{}]", active_bg_short),
        UiAction::OpenBgCropModal,
        app.crop_modal_open,
        rgb(3, 105, 161),
        f_small,
    );
    rx += gal_btn_w + 8;

    // Rightmost button(s) on Row 1: 3s Real Background Capture + Reset / Auto-Calibrate
    let rem_r1_w = (win_w - 24 - rx).max(120);
    if has_ref {
        let cap_w = (rem_r1_w * 68) / 100;
        register_button(
            mem_dc,
            app,
            RECT::new(rx, r1_y0, rx + cap_w, r1_y1),
            "📸 Ricattura Sfondo (3s)",
            UiAction::StartReferenceBgCapture,
            app.bg_capture_countdown_end.is_some(),
            rgb(14, 116, 144),
            f_small,
        );
        register_button(
            mem_dc,
            app,
            RECT::new(rx + cap_w + 6, r1_y0, win_w - 24, r1_y1),
            "🗑️ Reset",
            UiAction::ClearReferenceBg,
            false,
            rgb(153, 27, 27),
            f_small,
        );
    } else if is_green {
        let cap_w = (rem_r1_w * 54) / 100;
        register_button(
            mem_dc,
            app,
            RECT::new(rx, r1_y0, rx + cap_w, r1_y1),
            "🪄 Auto-Calibra",
            UiAction::AutoCalibrate1Click,
            false,
            rgb(21, 128, 61),
            f_small,
        );
        register_button(
            mem_dc,
            app,
            RECT::new(rx + cap_w + 6, r1_y0, win_w - 24, r1_y1),
            "📸 Sfondo (3s)",
            UiAction::StartReferenceBgCapture,
            app.bg_capture_countdown_end.is_some(),
            rgb(14, 116, 144),
            f_small,
        );
    } else {
        register_button(
            mem_dc,
            app,
            RECT::new(rx, r1_y0, win_w - 24, r1_y1),
            "📸 Cattura Sfondo Reale (3s)",
            UiAction::StartReferenceBgCapture,
            app.bg_capture_countdown_end.is_some(),
            rgb(14, 116, 144),
            f_small,
        );
    }

    // Row 2 (`y = 542..578`, height 36): 5-Point Face Calibration + Volto+Busto vs Solo Volto + Mirror Bg + Multi-Person
    let r_mid_y0 = 542;
    let r_mid_y1 = 578;
    let mut rmx = 24;
    let calib_btn_w = (total_inner_w * 26) / 100;
    let sil_btn_w = (total_inner_w * 17) / 100;
    let mir_btn_w = (total_inner_w * 19) / 100;
    let is_calibrated = app.cfg.segmentation.ai_selfie.face_calibrated;
    let calib_lbl = if is_calibrated {
        "🎯 Calibra Volto [✓]"
    } else {
        "🎯 Calibra Volto (5 Punti)"
    };
    register_button(
        mem_dc,
        app,
        RECT::new(rmx, r_mid_y0, rmx + calib_btn_w, r_mid_y1),
        calib_lbl,
        UiAction::OpenFaceCalibModal,
        app.face_calib_modal_open || is_calibrated,
        if is_calibrated {
            rgb(16, 185, 129)
        } else {
            rgb(124, 58, 237)
        },
        f_btn,
    );
    rmx += calib_btn_w + 6;

    let sil_mode = app.cfg.segmentation.ai_selfie.silhouette_mode;
    register_button(
        mem_dc,
        app,
        RECT::new(rmx, r_mid_y0, rmx + sil_btn_w, r_mid_y1),
        "👤 Volto + Busto",
        UiAction::SetSilhouetteMode(0),
        sil_mode == 0,
        rgb(2, 132, 199),
        f_small,
    );
    rmx += sil_btn_w + 6;

    register_button(
        mem_dc,
        app,
        RECT::new(rmx, r_mid_y0, rmx + sil_btn_w, r_mid_y1),
        "🙂 Solo Volto",
        UiAction::SetSilhouetteMode(1),
        sil_mode == 1,
        rgb(13, 148, 136),
        f_small,
    );
    rmx += sil_btn_w + 6;

    let bg_mir_h = app.cfg.compositor.background_mirror_h;
    register_button(
        mem_dc,
        app,
        RECT::new(rmx, r_mid_y0, rmx + mir_btn_w, r_mid_y1),
        if bg_mir_h {
            "↔️ Specchia Sfondo: ON"
        } else {
            "↔️ Specchia Sfondo: OFF"
        },
        UiAction::ToggleBgMirrorH,
        bg_mir_h,
        rgb(3, 105, 161),
        f_small,
    );
    rmx += mir_btn_w + 6;

    let mp_active = app
        .plugin_registry
        .is_plugin_enabled(PLUGIN_ID_MULTI_PERSON);
    register_button(
        mem_dc,
        app,
        RECT::new(rmx, r_mid_y0, win_w - 24, r_mid_y1),
        if mp_active {
            "👥 Multi-Persona: ON"
        } else {
            "👥 Multi-Persona: OFF"
        },
        UiAction::QuickToggleMultiPerson,
        mp_active,
        rgb(14, 116, 144),
        f_small,
    );

    // Row 3 (`y = 584..622`, height 38): 4 Clean Dropdown Triggers + Tolerance (- / +)
    let r2_y0 = 584;
    let r2_y1 = 622;
    let mut r2x = 24;
    let dd_w1 = (total_inner_w * 17) / 100;
    let dd_w2 = (total_inner_w * 21) / 100;
    let dd_w3 = (total_inner_w * 17) / 100;
    let dd_w4 = (total_inner_w * 23) / 100;

    let ar = app.cfg.framing_output.aspect_ratio.as_str();
    let ar_dd_label = match ar {
        "16:9" => "📺 Formato: 16:9 (Wide) ▾",
        "1:1" => "📺 Formato: 1:1 (Social) ▾",
        "9:16" => "📺 Formato: 9:16 (Vert.) ▾",
        _ => "📺 Formato: 4:3 (Std) ▾",
    };
    let dd_ar_rc = RECT::new(r2x, r2_y0, r2x + dd_w1, r2_y1);
    register_button(
        mem_dc,
        app,
        dd_ar_rc,
        ar_dd_label,
        UiAction::ToggleDropdown(DropdownKind::AspectRatio),
        app.open_dropdown == Some(DropdownKind::AspectRatio),
        rgb(14, 116, 144),
        f_small,
    );
    r2x += dd_w1 + 6;

    let res_preset = app.cfg.framing_output.resolution_preset.as_str();
    let res_dd_label = format!(
        "🎞️ Definizione: {} ({}×{}) ▾",
        res_preset, app.out_w, app.out_h
    );
    let dd_res_rc = RECT::new(r2x, r2_y0, r2x + dd_w2, r2_y1);
    register_button(
        mem_dc,
        app,
        dd_res_rc,
        &res_dd_label,
        UiAction::ToggleDropdown(DropdownKind::OutputResolution),
        app.open_dropdown == Some(DropdownKind::OutputResolution),
        rgb(14, 116, 144),
        f_small,
    );
    r2x += dd_w2 + 6;

    let fps_val = app.cfg.camera_input.fps;
    let fps_dd_label = format!("⏱️ Fluidità: {} FPS ▾", fps_val);
    let dd_fps_rc = RECT::new(r2x, r2_y0, r2x + dd_w3, r2_y1);
    register_button(
        mem_dc,
        app,
        dd_fps_rc,
        &fps_dd_label,
        UiAction::ToggleDropdown(DropdownKind::OutputFps),
        app.open_dropdown == Some(DropdownKind::OutputFps),
        rgb(14, 116, 144),
        f_small,
    );
    r2x += dd_w3 + 6;

    let eng_mode = app.cfg.segmentation.ai_selfie.engine_mode;
    let eng_label = match eng_mode {
        1 => "🧠 Scontorno: Solo Sagoma AI ▾",
        2 => "🧠 Scontorno: Differenza Sfondo ▾",
        _ => "🧠 Scontorno: Auto (Ibrido) ▾",
    };
    let dd_eng_rc = RECT::new(r2x, r2_y0, r2x + dd_w4, r2_y1);
    register_button(
        mem_dc,
        app,
        dd_eng_rc,
        eng_label,
        UiAction::ToggleDropdown(DropdownKind::SegmentationEngine),
        app.open_dropdown == Some(DropdownKind::SegmentationEngine),
        rgb(30, 64, 175),
        f_small,
    );
    r2x += dd_w4 + 10;

    // Tolerance (- / +) Control on the right end of Row 3
    let tol_pct = (app.cfg.segmentation.ai_selfie.bg_diff_tolerance * 100.0).round() as i32;
    draw_text_left(
        mem_dc,
        r2x,
        r2_y0,
        82,
        38,
        "💡 Tolleranza:",
        rgb(203, 213, 225),
        f_small,
    );
    let btn_minus_x = r2x + 84;
    register_button(
        mem_dc,
        app,
        RECT::new(btn_minus_x, r2_y0 + 3, btn_minus_x + 32, r2_y1 - 3),
        "−",
        UiAction::AiBgTolDec,
        false,
        rgb(51, 65, 85),
        f_btn,
    );
    draw_text_center(
        mem_dc,
        RECT::new(btn_minus_x + 34, r2_y0, btn_minus_x + 84, r2_y1),
        &format!("{}%", tol_pct),
        rgb(56, 189, 248),
        f_btn,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(
            btn_minus_x + 86,
            r2_y0 + 3,
            (btn_minus_x + 118).min(win_w - 24),
            r2_y1 - 3,
        ),
        "+",
        UiAction::AiBgTolInc,
        false,
        rgb(51, 65, 85),
        f_btn,
    );

    // ========================================================================
    // 5. Bottom Utility & Status Footer (`y = win_h - 42 .. win_h`)
    // ========================================================================
    let footer_y0 = (win_h - 42).max(636);
    let status_rc = RECT::new(0, footer_y0, win_w, win_h);
    let st_br = CreateSolidBrush(rgb(15, 22, 36));
    FillRect(mem_dc, &status_rc, st_br);
    DeleteObject(st_br);

    let util_block_w = 576;
    draw_text_left(
        mem_dc,
        18,
        footer_y0 + 4,
        (win_w - util_block_w - 30).max(200),
        34,
        &app.status_message,
        rgb(125, 211, 252),
        f_small,
    );

    let ux0 = win_w - util_block_w;
    let uy0 = footer_y0 + 6;
    let uy1 = win_h - 6;
    register_button(
        mem_dc,
        app,
        RECT::new(ux0, uy0, ux0 + 132, uy1),
        "🔓 Permessi Cam",
        UiAction::UnlockWin11Privacy,
        false,
        rgb(30, 58, 138),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(ux0 + 138, uy0, ux0 + 282, uy1),
        "🛡️ Ripara Driver",
        UiAction::InstallRepairDriver,
        false,
        rgb(20, 83, 45),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(ux0 + 288, uy0, ux0 + 418, uy1),
        "🌐 Test 2 Webcam",
        UiAction::OpenSideBySideWeb,
        false,
        rgb(51, 65, 85),
        f_small,
    );
    register_button(
        mem_dc,
        app,
        RECT::new(ux0 + 424, uy0, win_w - 14, uy1),
        if app.diag.autostart_enabled {
            "☑ Avvio Windows"
        } else {
            "☐ Avvio Windows"
        },
        UiAction::ToggleAutostart,
        app.diag.autostart_enabled,
        rgb(30, 64, 175),
        f_small,
    );

    // ========================================================================
    // 6. Modal Overlays (Highest Priority Wins, Clearing Underlying Hit Zones)
    // ========================================================================
    if app.pefl_modal_open {
        app.hit_zones.clear();
        app.dropdown_overlay_zones.clear();
        paint_pefl_modal_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if app.face_calib_modal_open {
        app.hit_zones.clear();
        app.dropdown_overlay_zones.clear();
        paint_face_calib_modal_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if app.plugins_modal_open {
        app.hit_zones.clear();
        app.dropdown_overlay_zones.clear();
        paint_plugins_modal_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if app.perf_modal_open {
        app.hit_zones.clear();
        app.dropdown_overlay_zones.clear();
        paint_perf_modal_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if app.crop_modal_open {
        app.hit_zones.clear();
        app.dropdown_overlay_zones.clear();
        paint_crop_modal_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if app.show_advanced {
        app.hit_zones.clear();
        app.dropdown_overlay_zones.clear();
        paint_advanced_modal_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if app.wizard_step > 0 {
        app.dropdown_overlay_zones.clear();
        paint_wizard_overlay(mem_dc, app, win_w, win_h, f_title, f_sub, f_btn, f_small);
    } else if let Some(open_kind) = app.open_dropdown {
        let anchor_rc = match open_kind {
            DropdownKind::AspectRatio => dd_ar_rc,
            DropdownKind::OutputResolution => dd_res_rc,
            DropdownKind::OutputFps => dd_fps_rc,
            DropdownKind::SegmentationEngine => dd_eng_rc,
        };
        paint_open_dropdown_menu(mem_dc, app, open_kind, anchor_rc, win_h, f_small);
    }

    BitBlt(hdc_win, 0, 0, win_w, win_h, mem_dc, 0, 0, SRCCOPY);

    EndPaint(hwnd, &ps);
}

// ============================================================================
// Modal 0: Calibrazione Guidata Volto a 5 Punti + Selettore Volto+Busto / Solo Volto
// ============================================================================

unsafe fn paint_face_calib_modal_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let full_rc = RECT::new(0, 0, win_w, win_h);
    let dim_br = CreateSolidBrush(rgb(9, 13, 22));
    FillRect(hdc, &full_rc, dim_br);
    DeleteObject(dim_br);

    let w_box = 1000.min(win_w - 16);
    let h_box = 654.min(win_h - 14);
    let x0 = ((win_w - w_box) / 2).max(8);
    let y0 = ((win_h - h_box) / 2).max(7);
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(168, 85, 247));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 48);
    let tb_br = CreateSolidBrush(rgb(109, 40, 217));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    draw_text_left(
        hdc,
        x0 + 18,
        y0 + 5,
        w_box - 160,
        22,
        "🎯 Calibrazione Guidata Volto a 5 Punti — Scontorno Anatomico Accuratissimo",
        rgb(255, 255, 255),
        f_title,
    );
    draw_text_left(
        hdc,
        x0 + 18,
        y0 + 27,
        w_box - 160,
        18,
        "Clicca sulla foto i 5 punti del viso per calibrare il tono della tua pelle (YCbCr) e la geometria esatta della tua testa.",
        rgb(233, 213, 255),
        f_small,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 124, y0 + 9, x0 + w_box - 14, y0 + 39),
        "✖ Chiudi",
        UiAction::CloseFaceCalibModal,
        false,
        rgb(51, 65, 85),
        f_btn,
    );

    let pad_x = x0 + 18;
    let inner_w = w_box - 36;
    let mut cy = y0 + 56;

    // 1. Top Selector: [ 👤 Mostra Volto + Busto (Standard) ] vs [ 🙂 Mostra Solo Volto (Ritaglia Spalle e Busto) ]
    let sil_mode = app.cfg.segmentation.ai_selfie.silhouette_mode;
    let half_w = (inner_w - 12) / 2;
    register_button(
        hdc,
        app,
        RECT::new(pad_x, cy, pad_x + half_w, cy + 36),
        "👤 Mostra Volto + Busto (Standard — Testa, Collo e Spalle)",
        UiAction::SetSilhouetteMode(0),
        sil_mode == 0,
        rgb(2, 132, 199),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(pad_x + half_w + 12, cy, pad_x + inner_w, cy + 36),
        "🙂 Mostra Solo Volto (Ritaglia Spalle, Busto e Sedie)",
        UiAction::SetSilhouetteMode(1),
        sil_mode == 1,
        rgb(13, 148, 136),
        f_btn,
    );
    cy += 44;

    // 2. Step-by-Step High-Contrast Instruction Banner (Right Eye First, Left Eye Second)
    let step = app.face_calib_step.min(5);
    let (step_title, step_sub, banner_fill, banner_border) = match step {
        0 => (
            "PASSO 1 di 5: Clicca sul tuo OCCHIO DESTRO nell'immagine qui sotto",
            "Posiziona il mirino (+) al centro del tuo occhio destro e fai clic.",
            rgb(30, 58, 138),
            rgb(56, 189, 248),
        ),
        1 => (
            "PASSO 2 di 5: Ora clicca sul tuo OCCHIO SINISTRO",
            "Posiziona il mirino (+) al centro del tuo occhio sinistro e fai clic.",
            rgb(30, 58, 138),
            rgb(56, 189, 248),
        ),
        2 => (
            "PASSO 3 di 5: Ora clicca sulla PUNTA DEL NASO",
            "Clicca esattamente sulla punta del naso: SmartCam campionerà la tua luce e carnagione reale.",
            rgb(113, 63, 18),
            rgb(250, 204, 21),
        ),
        3 => (
            "PASSO 4 di 5: Ora clicca al CENTRO DELLE LABBRA (Bocca)",
            "Clicca al centro della bocca per definire le proporzioni verticali del viso.",
            rgb(113, 63, 18),
            rgb(250, 204, 21),
        ),
        4 => (
            "PASSO 5 di 5: Infine clicca sulla PUNTA DEL MENTO (Bordo inferiore del viso)",
            "Clicca sul bordo inferiore del mento per completare la sagoma anatomica della testa!",
            rgb(88, 28, 135),
            rgb(192, 132, 252),
        ),
        _ => (
            "✅ CALIBRAZIONE VOLTO COMPLETATA! Profilo biometrico attivo e salvato.",
            "Il motore C++/Rust ora protegge al 100% l'interno del tuo volto ed elimina sedie, porte e oggetti lontani.",
            rgb(20, 83, 45),
            rgb(74, 222, 128),
        ),
    };

    let inst_rc = RECT::new(pad_x, cy, pad_x + inner_w, cy + 52);
    let ib_br = CreateSolidBrush(banner_fill);
    FillRect(hdc, &inst_rc, ib_br);
    DeleteObject(ib_br);
    let ib_bd = CreateSolidBrush(banner_border);
    FrameRect(hdc, &inst_rc, ib_bd);
    DeleteObject(ib_bd);

    draw_text_center(
        hdc,
        RECT::new(
            inst_rc.left + 10,
            inst_rc.top + 4,
            inst_rc.right - 10,
            inst_rc.top + 28,
        ),
        step_title,
        rgb(255, 255, 255),
        f_sub,
    );
    draw_text_center(
        hdc,
        RECT::new(
            inst_rc.left + 10,
            inst_rc.top + 28,
            inst_rc.right - 10,
            inst_rc.bottom - 4,
        ),
        step_sub,
        rgb(226, 232, 240),
        f_small,
    );
    cy += 60;

    // 3. Interactive Snapshot Photo Canvas (`cy .. y0 + h_box - 58`)
    let photo_box_rc = RECT::new(pad_x, cy, pad_x + inner_w, y0 + h_box - 58);
    let pb_br = CreateSolidBrush(rgb(7, 10, 17));
    FillRect(hdc, &photo_box_rc, pb_br);
    DeleteObject(pb_br);
    let pb_bd = CreateSolidBrush(rgb(51, 65, 85));
    FrameRect(hdc, &photo_box_rc, pb_bd);
    DeleteObject(pb_bd);

    let snap_w = app.face_calib_snap_w.max(160) as i32;
    let snap_h = app.face_calib_snap_h.max(120) as i32;
    let avail_w = photo_box_rc.width() - 20;
    let avail_h = photo_box_rc.height() - 20;
    let (draw_w, draw_h) = if snap_w * avail_h > snap_h * avail_w {
        (avail_w, (avail_w * snap_h) / snap_w.max(1))
    } else {
        ((avail_h * snap_w) / snap_h.max(1), avail_h)
    };
    let px = photo_box_rc.left + (photo_box_rc.width() - draw_w) / 2;
    let py = photo_box_rc.top + (photo_box_rc.height() - draw_h) / 2;
    app.face_calib_photo_rect = RECT::new(px, py, px + draw_w, py + draw_h);

    if !app.face_calib_snapshot_buf.is_empty()
        && app.face_calib_snap_w > 0
        && app.face_calib_snap_h > 0
    {
        let mut bmi: BITMAPINFO = Default::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as DWORD;
        bmi.bmiHeader.biWidth = app.face_calib_snap_w as LONG;
        bmi.bmiHeader.biHeight = -(app.face_calib_snap_h as LONG);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        SetStretchBltMode(hdc, COLORONCOLOR);
        StretchDIBits(
            hdc,
            px,
            py,
            draw_w,
            draw_h,
            0,
            0,
            app.face_calib_snap_w as i32,
            app.face_calib_snap_h as i32,
            app.face_calib_snapshot_buf.as_ptr() as *const c_void,
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }

    let ph_border = CreateSolidBrush(if step >= 5 {
        rgb(74, 222, 128)
    } else {
        rgb(168, 85, 247)
    });
    FrameRect(hdc, &app.face_calib_photo_rect, ph_border);
    DeleteObject(ph_border);

    // Draw anatomical head ellipse preview when all 5 points are placed
    if step >= 5 {
        let pts = app.face_calib_points;
        let eye_cx = (pts[0].0 + pts[1].0) * 0.5;
        let eye_cy = (pts[0].1 + pts[1].1) * 0.5;
        let head_cx = (eye_cx * 0.45 + pts[2].0 * 0.35 + pts[3].0 * 0.20).clamp(0.10, 0.90);
        let head_cy = (eye_cy * 0.48 + pts[2].1 * 0.32 + pts[3].1 * 0.20).clamp(0.10, 0.88);
        let h_rx = app
            .cfg
            .segmentation
            .ai_selfie
            .calib_head_rx
            .clamp(0.05, 0.36);
        let h_ry = app
            .cfg
            .segmentation
            .ai_selfie
            .calib_head_ry
            .clamp(0.08, 0.46);

        let el_l = px + ((head_cx - h_rx * 1.15) * draw_w as f32) as i32;
        let el_r = px + ((head_cx + h_rx * 1.15) * draw_w as f32) as i32;
        let el_t = py + ((head_cy - h_ry * 1.15) * draw_h as f32) as i32;
        let el_b = py + ((head_cy + h_ry * 1.15) * draw_h as f32) as i32;

        let pen_head = CreatePen(PS_SOLID, 2, rgb(74, 222, 128));
        let old_pen = SelectObject(hdc, pen_head);
        let old_br = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        RoundRect(
            hdc,
            el_l,
            el_t,
            el_r,
            el_b,
            (el_r - el_l).max(10),
            (el_b - el_t).max(10),
        );

        // If Volto + Busto is selected, also show the anatomical bust envelope preview
        if sil_mode == 0 {
            let bust_cy = (head_cy + h_ry * 2.15).min(1.05);
            let bust_rx = (h_rx * 2.35).clamp(0.14, 0.58);
            let bust_ry = (h_ry * 1.65).clamp(0.14, 0.55);
            let bl = (px + ((head_cx - bust_rx) * draw_w as f32) as i32).max(px);
            let br = (px + ((head_cx + bust_rx) * draw_w as f32) as i32).min(px + draw_w);
            let bt = (py + ((bust_cy - bust_ry) * draw_h as f32) as i32).max(py);
            let bb = (py + ((bust_cy + bust_ry) * draw_h as f32) as i32).min(py + draw_h);
            let pen_bust = CreatePen(PS_SOLID, 2, rgb(56, 189, 248));
            SelectObject(hdc, pen_bust);
            RoundRect(hdc, bl, bt, br, bb, (br - bl).max(10), (bb - bt).max(10));
            SelectObject(hdc, pen_head);
            DeleteObject(pen_bust);
        }

        SelectObject(hdc, old_br);
        SelectObject(hdc, old_pen);
        DeleteObject(pen_head);
    }

    // Draw numbered crosshairs for each clicked point (1..=step)
    let pt_labels: [&str; 5] = [
        "1: Occhio DX",
        "2: Occhio SX",
        "3: Naso",
        "4: Labbra",
        "5: Mento",
    ];
    let pt_colors: [u32; 5] = [
        rgb(56, 189, 248),
        rgb(56, 189, 248),
        rgb(250, 204, 21),
        rgb(244, 114, 182),
        rgb(74, 222, 128),
    ];

    for i in 0..step.min(5) {
        let (nx, ny) = app.face_calib_points[i];
        let cx = px + (nx.clamp(0.0, 1.0) * draw_w as f32).round() as i32;
        let cy_pt = py + (ny.clamp(0.0, 1.0) * draw_h as f32).round() as i32;
        let col = pt_colors[i];

        // Crosshair arms + center dot
        let h_line = RECT::new(cx - 10, cy_pt - 1, cx + 11, cy_pt + 2);
        let v_line = RECT::new(cx - 1, cy_pt - 10, cx + 2, cy_pt + 11);
        let c_br = CreateSolidBrush(col);
        FillRect(hdc, &h_line, c_br);
        FillRect(hdc, &v_line, c_br);
        DeleteObject(c_br);

        // Place tag pill outward so it NEVER covers the other eye or central facial landmarks
        let place_left = (i <= 1 && nx < 0.50) || (cx + 96 > px + draw_w - 4);
        let (tag_l, tag_r) = if place_left {
            ((cx - 96).max(px + 2), (cx - 8).max(px + 90))
        } else {
            (
                (cx + 8).min(px + draw_w - 92),
                (cx + 96).min(px + draw_w - 2),
            )
        };
        let (tag_t, tag_b) = if i <= 1 {
            // Place eye labels slightly above the eye line to leave the bridge and other eye completely clear
            ((cy_pt - 24).max(py + 2), (cy_pt - 2).max(py + 24))
        } else {
            ((cy_pt - 11).max(py + 2), (cy_pt + 11).min(py + draw_h - 2))
        };
        let tag_rc = RECT::new(tag_l, tag_t, tag_r, tag_b);
        let t_bg = CreateSolidBrush(rgb(15, 23, 42));
        FillRect(hdc, &tag_rc, t_bg);
        DeleteObject(t_bg);
        let t_bd = CreateSolidBrush(col);
        FrameRect(hdc, &tag_rc, t_bd);
        DeleteObject(t_bd);
        draw_text_center(hdc, tag_rc, pt_labels[i], col, f_small);
    }

    // 4. Bottom Action Bar
    let fy = y0 + h_box - 46;
    register_button(
        hdc,
        app,
        RECT::new(pad_x, fy, pad_x + 230, fy + 34),
        "📸 Scatta Nuova Foto",
        UiAction::RetakeFaceCalibSnapshot,
        false,
        rgb(14, 116, 144),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(pad_x + 240, fy, pad_x + 470, fy + 34),
        "↩️ Correggi Ultimo Punto",
        UiAction::UndoFaceCalibPoint,
        false,
        rgb(71, 85, 105),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(pad_x + 480, fy, pad_x + 720, fy + 34),
        "🗑️ Ripristina Automatico",
        UiAction::ResetFaceCalibration,
        false,
        rgb(153, 27, 27),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 230, fy, x0 + w_box - 18, fy + 34),
        "✅ Applica e Chiudi",
        UiAction::CloseFaceCalibModal,
        true,
        rgb(16, 185, 129),
        f_sub,
    );
}

// ============================================================================
// Modal 1: Gestione Plugin (Sorgenti Fotocamera Smartphone & Filtri Studio)
// ============================================================================

unsafe fn paint_plugins_modal_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let full_rc = RECT::new(0, 0, win_w, win_h);
    let dim_br = CreateSolidBrush(rgb(9, 13, 22));
    FillRect(hdc, &full_rc, dim_br);
    DeleteObject(dim_br);

    let w_box = 1020.min(win_w - 16);
    let h_box = 656.min(win_h - 12);
    let x0 = ((win_w - w_box) / 2).max(8);
    let y0 = ((win_h - h_box) / 2).max(6);
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(20, 184, 166));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 48);
    let tb_br = CreateSolidBrush(rgb(15, 118, 110));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    draw_text_left(
        hdc,
        x0 + 20,
        y0 + 5,
        w_box - 180,
        22,
        "🧩 Gestione Plugin Modulari — Sorgenti Fotocamera Smartphone & Filtri Video Studio",
        rgb(255, 255, 255),
        f_title,
    );
    draw_text_left(
        hdc,
        x0 + 20,
        y0 + 27,
        w_box - 180,
        18,
        "Attiva o disattiva con 1 clic i plugin integrati o i file .dll esterni presenti nella cartella plugins/",
        rgb(204, 251, 241),
        f_small,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 130, y0 + 9, x0 + w_box - 16, y0 + 39),
        "✖ Chiudi",
        UiAction::ClosePluginsModal,
        false,
        rgb(51, 65, 85),
        f_btn,
    );

    let mut py = y0 + 54;
    let plugins_snapshot: Vec<_> = app.plugin_registry.plugins().to_vec();

    for (idx, p) in plugins_snapshot.iter().enumerate() {
        let is_phone = p.id == PLUGIN_ID_PHONE_CAMERA;
        let row_h = if is_phone { 174 } else { 80 };
        if py + row_h > y0 + h_box - 48 {
            break;
        }

        let item_rc = RECT::new(x0 + 18, py, x0 + w_box - 18, py + row_h);
        let ib = CreateSolidBrush(if p.enabled {
            rgb(19, 38, 47)
        } else {
            rgb(20, 29, 46)
        });
        FillRect(hdc, &item_rc, ib);
        DeleteObject(ib);

        let bd = CreateSolidBrush(if p.enabled {
            rgb(45, 212, 191)
        } else {
            rgb(51, 65, 85)
        });
        FrameRect(hdc, &item_rc, bd);
        DeleteObject(bd);

        let text_max_w = if is_phone {
            item_rc.width() - 315
        } else {
            item_rc.width() - 220
        };

        // Plugin Title + Version + Category Badge
        let cat_badge = format!(
            "[{}] • v{} • Autore: {}",
            p.category.display_label_it(),
            p.version,
            p.author
        );
        draw_text_left(
            hdc,
            item_rc.left + 14,
            py + 4,
            text_max_w,
            20,
            &p.name,
            if p.enabled {
                rgb(94, 234, 212)
            } else {
                rgb(248, 250, 252)
            },
            f_sub,
        );
        draw_text_left(
            hdc,
            item_rc.left + 14,
            py + 23,
            text_max_w,
            18,
            &cat_badge,
            rgb(125, 211, 252),
            f_small,
        );
        draw_text_left(
            hdc,
            item_rc.left + 14,
            py + 41,
            text_max_w,
            18,
            &p.description,
            rgb(203, 213, 225),
            f_small,
        );
        draw_text_left(
            hdc,
            item_rc.left + 14,
            py + 59,
            text_max_w,
            18,
            &format!("Stato: {}", p.status_label),
            if p.enabled {
                rgb(74, 222, 128)
            } else {
                rgb(148, 163, 184)
            },
            f_small,
        );

        // Right 1-Click Toggle Button ([ ✅ ATTIVO ] / [ ⚪ DISATTIVATO ])
        let toggle_rc = RECT::new(item_rc.right - 190, py + 8, item_rc.right - 14, py + 42);
        register_button(
            hdc,
            app,
            toggle_rc,
            if p.enabled {
                "✅ ATTIVO (Disattiva)"
            } else {
                "⚪ DISATTIVATO (Attiva)"
            },
            UiAction::TogglePluginByIndex(idx),
            p.enabled,
            if p.enabled {
                rgb(16, 185, 129)
            } else {
                rgb(51, 65, 85)
            },
            f_btn,
        );

        // Extra Quick-Connect, QR Code, Live Telemetry & Port / Firewall / USB-Hotspot Bar for Smartphone Camera Plugin
        if is_phone {
            let srv_url = app.plugin_registry.phone_server_url();
            let srv_running = app.plugin_registry.is_phone_server_running();
            let rx_frames = app.plugin_registry.phone_connected_frames();
            let tcp_conns = app.plugin_registry.phone_tcp_connections();
            let last_log = app.plugin_registry.phone_last_client_log();
            let cur_port = app.plugin_registry.phone_server_port();

            // Draw Scannable QR Code on the right side of the Smartphone card (`item_rc.right - 118 .. item_rc.right - 14`)
            let (qr_n, qr_mods) = crate::plugins::generate_qr_code_matrix(&srv_url);
            let mod_px = 3i32;
            let quiet = 2i32;
            let qr_box_sz = (qr_n as i32 + quiet * 2) * mod_px; // 29 * 3 = 87 px
            let qr_x0 = item_rc.right - qr_box_sz - 14;
            let qr_y0 = py + 46;
            let qr_bg_rc = RECT::new(qr_x0, qr_y0, qr_x0 + qr_box_sz, qr_y0 + qr_box_sz);
            let white_br = CreateSolidBrush(rgb(255, 255, 255));
            FillRect(hdc, &qr_bg_rc, white_br);
            DeleteObject(white_br);
            let black_br = CreateSolidBrush(rgb(0, 0, 0));
            for r in 0..qr_n {
                for c in 0..qr_n {
                    if qr_mods[r * qr_n + c] {
                        let mx = qr_x0 + (c as i32 + quiet) * mod_px;
                        let my = qr_y0 + (r as i32 + quiet) * mod_px;
                        let m_rc = RECT::new(mx, my, mx + mod_px, my + mod_px);
                        FillRect(hdc, &m_rc, black_br);
                    }
                }
            }
            DeleteObject(black_br);
            draw_text_center(
                hdc,
                RECT::new(
                    qr_x0 - 16,
                    qr_y0 + qr_box_sz + 2,
                    item_rc.right - 8,
                    py + row_h - 4,
                ),
                "📱 Inquadra QR",
                rgb(254, 240, 138),
                f_small,
            );

            let left_panel_right = qr_x0 - 12;
            let url_banner_y = py + 79;
            let url_banner = RECT::new(
                item_rc.left + 12,
                url_banner_y,
                left_panel_right,
                url_banner_y + 50,
            );
            let ub_fill = CreateSolidBrush(if srv_running {
                rgb(15, 52, 43)
            } else {
                rgb(30, 41, 59)
            });
            FillRect(hdc, &url_banner, ub_fill);
            DeleteObject(ub_fill);
            let ub_bd = CreateSolidBrush(if srv_running {
                rgb(74, 222, 128)
            } else {
                rgb(71, 85, 105)
            });
            FrameRect(hdc, &url_banner, ub_bd);
            DeleteObject(ub_bd);

            let srv_status_txt = if srv_running {
                format!(
                    "🟢 SERVER ATTIVO 0.0.0.0:{}  •  Apri sul Telefono: {}  (Usa http://)",
                    cur_port, srv_url
                )
            } else {
                format!(
                    "⚠️ Porta {} occupata — Scegli un'altra porta qui sotto e premi Applica",
                    cur_port
                )
            };
            draw_text_left(
                hdc,
                url_banner.left + 10,
                url_banner.top + 4,
                url_banner.width() - 360,
                20,
                &srv_status_txt,
                rgb(254, 240, 138),
                f_small,
            );

            let telemetry_txt = format!(
                "📡 Tentativi TCP ricevuti: {}  •  Frame video: {}  •  Log: {}",
                tcp_conns, rx_frames, last_log
            );
            draw_text_left(
                hdc,
                url_banner.left + 10,
                url_banner.top + 25,
                url_banner.width() - 360,
                20,
                &telemetry_txt,
                rgb(167, 243, 208),
                f_small,
            );

            // 1-Click Firewall + Private Network Unlock Button & USB / Hotspot Bridge Button
            register_button(
                hdc,
                app,
                RECT::new(
                    url_banner.right - 348,
                    url_banner.top + 8,
                    url_banner.right - 182,
                    url_banner.bottom - 8,
                ),
                &format!("🛡️ Sblocca Firewall ({})", cur_port),
                UiAction::UnlockPhoneFirewall,
                false,
                rgb(21, 128, 61),
                f_small,
            );
            register_button(
                hdc,
                app,
                RECT::new(
                    url_banner.right - 176,
                    url_banner.top + 8,
                    url_banner.right - 6,
                    url_banner.bottom - 8,
                ),
                "📶 Ponte USB / Hotspot",
                UiAction::ActivatePhoneUsbOrHotspotBridge,
                false,
                rgb(180, 83, 9),
                f_small,
            );

            // Row 2 of Phone controls: Manual Keyboard Port Input Box + Apply + Default 8080 + USB/Clipboard
            let bar_y = py + 135;
            let mut bx = item_rc.left + 12;
            draw_text_left(
                hdc,
                bx,
                bar_y + 4,
                136,
                24,
                "Porta manuale:",
                rgb(125, 211, 252),
                f_small,
            );
            bx += 138;

            let input_focused = app.phone_port_input_focused;
            let input_display = if input_focused {
                format!("⌨️ {}|", app.phone_port_input_text)
            } else {
                format!("✏️ {} (Digita)", app.phone_port_input_text)
            };
            register_button(
                hdc,
                app,
                RECT::new(bx, bar_y, bx + 152, bar_y + 30),
                &input_display,
                UiAction::FocusPhonePortInput,
                input_focused,
                if input_focused {
                    rgb(180, 83, 9)
                } else {
                    rgb(30, 41, 59)
                },
                f_btn,
            );
            bx += 158;

            register_button(
                hdc,
                app,
                RECT::new(bx, bar_y, bx + 124, bar_y + 30),
                "✅ Applica Porta",
                UiAction::ApplyManualPhonePort,
                false,
                rgb(16, 185, 129),
                f_small,
            );
            bx += 130;

            register_button(
                hdc,
                app,
                RECT::new(bx, bar_y, bx + 124, bar_y + 30),
                "🔄 Default (8080)",
                UiAction::SetPhoneServerPortPreset(8080),
                cur_port == 8080,
                rgb(2, 132, 199),
                f_small,
            );
            bx += 130;

            register_button(
                hdc,
                app,
                RECT::new(bx, bar_y, bx + 128, bar_y + 30),
                "🔌 USB (:4747)",
                UiAction::SetPhonePreset(1),
                cur_port == 4747,
                rgb(2, 132, 199),
                f_small,
            );
            bx += 134;

            register_button(
                hdc,
                app,
                RECT::new(bx, bar_y, left_panel_right, bar_y + 30),
                "📋 Incolla URL",
                UiAction::PastePhoneUrlFromClipboard,
                false,
                rgb(14, 116, 144),
                f_small,
            );
        }

        py += row_h + 6;
    }

    // Footer of Plugin Modal
    let fy = y0 + h_box - 44;
    register_button(
        hdc,
        app,
        RECT::new(x0 + 18, fy, x0 + 270, fy + 34),
        "🔄 Riscansiona Cartella plugins/",
        UiAction::RescanPluginsDir,
        false,
        rgb(51, 65, 85),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + 280, fy, x0 + 590, fy + 34),
        "📖 Come Creare un Plugin (how-to-make-a-plugin.md)",
        UiAction::OpenPluginGuideDoc,
        false,
        rgb(30, 64, 175),
        f_small,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 230, fy, x0 + w_box - 18, fy + 34),
        "✅ Salva e Chiudi",
        UiAction::ClosePluginsModal,
        true,
        rgb(16, 185, 129),
        f_sub,
    );
}

// ============================================================================
// Modal 2: Centro Prestazioni — CPU/GPU (Vulkan, OpenCL, CUDA, ROCm), AVX, AI & Benchmark
// ============================================================================

unsafe fn paint_perf_modal_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let full_rc = RECT::new(0, 0, win_w, win_h);
    let dim_br = CreateSolidBrush(rgb(9, 13, 22));
    FillRect(hdc, &full_rc, dim_br);
    DeleteObject(dim_br);

    let w_box = 1040.min(win_w - 16);
    let h_box = 650.min(win_h - 16);
    let x0 = ((win_w - w_box) / 2).max(8);
    let y0 = ((win_h - h_box) / 2).max(8);
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(56, 189, 248));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 48);
    let tb_br = CreateSolidBrush(rgb(2, 105, 161));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    draw_text_left(
        hdc,
        x0 + 18,
        y0 + 5,
        w_box - 160,
        22,
        "⚡ Centro Prestazioni Hardware — CPU / GPU (Vulkan, OpenCL, CUDA, ROCm), AVX, AI & Benchmark",
        rgb(255, 255, 255),
        f_title,
    );
    draw_text_left(
        hdc,
        x0 + 18,
        y0 + 27,
        w_box - 160,
        18,
        &format!(
            "Processore: {} ({} core)  •  Motore Attivo: {}",
            app.benchmark_report.cpu_name,
            app.benchmark_report.logical_cores,
            app.pipeline.simd_backend_label
        ),
        rgb(186, 230, 253),
        f_small,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 124, y0 + 9, x0 + w_box - 14, y0 + 39),
        "✖ Chiudi",
        UiAction::ClosePerfModal,
        false,
        rgb(51, 65, 85),
        f_btn,
    );

    let pad_x = x0 + 18;
    let inner_w = w_box - 36;
    let mut cy = y0 + 56;

    // ------------------------------------------------------------------------
    // Section 1: Dispositivo di Calcolo (CPU vs GPU Vulkan / OpenCL / CUDA / ROCm)
    // ------------------------------------------------------------------------
    draw_text_left(
        hdc,
        pad_x,
        cy,
        inner_w,
        20,
        "1. Dispositivo di Calcolo Video & Scontorno (Seleziona CPU o Accelerazione GPU / iGPU):",
        rgb(248, 250, 252),
        f_sub,
    );
    cy += 22;

    let cur_backend = app.cfg.compute_backend.to_ascii_lowercase();
    let backend_buttons: [(u8, &str, &str); 6] = [
        (0, "auto", "✨ Auto"),
        (1, "cpu", "🖥️ CPU"),
        (2, "vulkan", "🌋 Vulkan"),
        (3, "opencl", "⚙️ OpenCL"),
        (4, "cuda", "💚 NVIDIA CUDA"),
        (5, "rocm", "❤️ AMD ROCm"),
    ];
    let b_col_w = (inner_w - 5 * 8) / 6;
    for (i, (bid, bkey, blbl)) in backend_buttons.iter().enumerate() {
        let bx = pad_x + (i as i32) * (b_col_w + 8);
        let (avail, sub_status) = if *bid == 0 {
            (true, "Selezione Ottimale".to_string())
        } else if let Some(d) = app
            .benchmark_report
            .devices
            .iter()
            .find(|d| d.backend_id == *bid as u32)
        {
            (
                d.available,
                if d.available {
                    "✓ Disponibile".to_string()
                } else {
                    "Non supportato".to_string()
                },
            )
        } else {
            (
                *bid == 1,
                if *bid == 1 {
                    "✓ Disponibile".to_string()
                } else {
                    "Non supportato".to_string()
                },
            )
        };
        let is_sel = (cur_backend == *bkey || (cur_backend.is_empty() && *bid == 0)) && avail;
        let btn_lbl = format!("{} [{}]", blbl, sub_status);
        let btn_rc = RECT::new(bx, cy, bx + b_col_w, cy + 34);
        if avail {
            let accent = if *bid >= 2 {
                rgb(13, 148, 136)
            } else {
                rgb(2, 132, 199)
            };
            register_button(
                hdc,
                app,
                btn_rc,
                &btn_lbl,
                UiAction::SelectComputeBackend(*bid),
                is_sel,
                accent,
                f_small,
            );
        } else {
            // Visually disabled button for unsupported hardware backends (no hit zone)
            let d_br = CreateSolidBrush(rgb(18, 25, 38));
            FillRect(hdc, &btn_rc, d_br);
            DeleteObject(d_br);
            let d_bd = CreateSolidBrush(rgb(45, 55, 72));
            FrameRect(hdc, &btn_rc, d_bd);
            DeleteObject(d_bd);
            draw_text_center(hdc, btn_rc, &btn_lbl, rgb(100, 116, 139), f_small);
        }
    }
    cy += 42;

    // ------------------------------------------------------------------------
    // Section 2: Accelerazione Vettoriale CPU (SIMD AVX) & Selettore AI
    // ------------------------------------------------------------------------
    let half_w = (inner_w - 14) / 2;
    draw_text_left(
        hdc,
        pad_x,
        cy,
        half_w,
        20,
        &format!(
            "2. Istruzioni Vettoriali CPU (Supporto PC: AVX2={} • AVX-512={}):",
            if cpu_supports_avx2() { "SÌ" } else { "NO" },
            if bench_cpu_supports_avx512() {
                "SÌ"
            } else {
                "NO"
            }
        ),
        rgb(248, 250, 252),
        f_sub,
    );
    draw_text_left(
        hdc,
        pad_x + half_w + 14,
        cy,
        half_w,
        20,
        "3. Intelligenza Artificiale (Qualità Scontorno vs Ultra-Leggero):",
        rgb(248, 250, 252),
        f_sub,
    );
    cy += 22;

    let cur_simd = app.cfg.compositor.simd_acceleration.to_ascii_lowercase();
    let simd_w = (half_w - 3 * 6) / 4;
    let simd_opts: [(u8, &str, &str); 4] = [
        (0, "auto", "⚡ Auto"),
        (1, "avx512", "🚀 AVX-512"),
        (2, "avx2", "🔹 AVX2"),
        (3, "scalar", "🛡️ Senza AVX"),
    ];
    for (i, (sid, skey, slbl)) in simd_opts.iter().enumerate() {
        let sx = pad_x + (i as i32) * (simd_w + 6);
        let simd_avail = match *sid {
            1 => bench_cpu_supports_avx512(),
            2 => cpu_supports_avx2(),
            _ => true,
        };
        let is_sel = (cur_simd == *skey || (cur_simd.is_empty() && *sid == 0)) && simd_avail;
        let s_rc = RECT::new(sx, cy, sx + simd_w, cy + 34);
        if simd_avail {
            register_button(
                hdc,
                app,
                s_rc,
                slbl,
                UiAction::SelectSimdMode(*sid),
                is_sel,
                rgb(79, 70, 229),
                f_small,
            );
        } else {
            let d_br = CreateSolidBrush(rgb(18, 25, 38));
            FillRect(hdc, &s_rc, d_br);
            DeleteObject(d_br);
            let d_bd = CreateSolidBrush(rgb(45, 55, 72));
            FrameRect(hdc, &s_rc, d_bd);
            DeleteObject(d_bd);
            draw_text_center(
                hdc,
                s_rc,
                &format!("{} [Assente]", slbl),
                rgb(100, 116, 139),
                f_small,
            );
        }
    }

    let ai_x0 = pad_x + half_w + 14;
    let ai_btn_w = (half_w - 8) / 2;
    register_button(
        hdc,
        app,
        RECT::new(ai_x0, cy, ai_x0 + ai_btn_w, cy + 34),
        "🤖 AI Attiva (Massima Qualità)",
        UiAction::SetAiEnabled(true),
        app.cfg.ai_enabled,
        rgb(16, 185, 129),
        f_small,
    );
    register_button(
        hdc,
        app,
        RECT::new(ai_x0 + ai_btn_w + 8, cy, ai_x0 + half_w, cy + 34),
        "🍃 AI Disattivata (Ultra-Leggero)",
        UiAction::SetAiEnabled(false),
        !app.cfg.ai_enabled,
        rgb(217, 119, 6),
        f_small,
    );
    cy += 42;

    // ------------------------------------------------------------------------
    // Section 3: Tabella Benchmark Hardware (`1440x1080`) & Scelta Consigliata
    // ------------------------------------------------------------------------
    let rec_banner = RECT::new(pad_x, cy, pad_x + inner_w, cy + 30);
    let rb_fill = CreateSolidBrush(rgb(20, 63, 40));
    FillRect(hdc, &rec_banner, rb_fill);
    DeleteObject(rb_fill);
    let rb_bd = CreateSolidBrush(rgb(74, 222, 128));
    FrameRect(hdc, &rec_banner, rb_bd);
    DeleteObject(rb_bd);

    draw_text_left(
        hdc,
        pad_x + 12,
        cy + 3,
        inner_w - 24,
        24,
        &format!(
            "⭐ CONSIGLIATO PER IL TUO HARDWARE: {}   (Clicca su una riga disponibile per attivarla subito)",
            app.benchmark_report.recommendation_reason
        ),
        rgb(220, 252, 231),
        f_small,
    );
    cy += 36;

    // Table Header
    let th_rc = RECT::new(pad_x, cy, pad_x + inner_w, cy + 24);
    let th_br = CreateSolidBrush(rgb(30, 41, 59));
    FillRect(hdc, &th_rc, th_br);
    DeleteObject(th_br);
    draw_text_left(
        hdc,
        pad_x + 12,
        cy + 2,
        230,
        20,
        "Configurazione / Profilo",
        rgb(148, 163, 184),
        f_small,
    );
    draw_text_left(
        hdc,
        pad_x + 250,
        cy + 2,
        220,
        20,
        "Stato Hardware su questo PC",
        rgb(148, 163, 184),
        f_small,
    );
    draw_text_left(
        hdc,
        pad_x + 480,
        cy + 2,
        190,
        20,
        "Punteggio Reale",
        rgb(148, 163, 184),
        f_small,
    );
    draw_text_left(
        hdc,
        pad_x + 680,
        cy + 2,
        120,
        20,
        "FPS Misurati",
        rgb(148, 163, 184),
        f_small,
    );
    draw_text_left(
        hdc,
        pad_x + 810,
        cy + 2,
        180,
        20,
        "Carico CPU (30 FPS)",
        rgb(148, 163, 184),
        f_small,
    );
    cy += 26;

    let profiles_clone = app.benchmark_report.profiles.clone();
    let max_score = profiles_clone
        .iter()
        .filter(|p| p.available)
        .map(|p| p.score)
        .max()
        .unwrap_or(10000)
        .max(1);
    let row_h = 34;
    for (idx, p) in profiles_clone.iter().enumerate() {
        if cy + row_h > y0 + h_box - 54 {
            break;
        }
        let r_rc = RECT::new(pad_x, cy, pad_x + inner_w, cy + row_h - 2);
        let hover = r_rc.contains(app.hover_pos.x, app.hover_pos.y);
        let is_cur_active = app
            .cfg
            .compute_backend
            .eq_ignore_ascii_case(&p.compute_backend)
            && (p.simd_mode == "auto"
                || app
                    .cfg
                    .compositor
                    .simd_acceleration
                    .eq_ignore_ascii_case(&p.simd_mode))
            && app.cfg.ai_enabled == p.ai_enabled;

        let row_bg = if p.recommended {
            rgb(19, 48, 38)
        } else if is_cur_active {
            rgb(23, 37, 84)
        } else if hover && p.available {
            rgb(38, 52, 77)
        } else {
            rgb(20, 29, 46)
        };
        let r_br = CreateSolidBrush(row_bg);
        FillRect(hdc, &r_rc, r_br);
        DeleteObject(r_br);

        let r_bd = CreateSolidBrush(if p.recommended {
            rgb(74, 222, 128)
        } else if is_cur_active {
            rgb(56, 189, 248)
        } else {
            rgb(45, 58, 82)
        });
        FrameRect(hdc, &r_rc, r_bd);
        DeleteObject(r_bd);

        let name_lbl = if p.recommended {
            format!("⭐ {}", p.label)
        } else if is_cur_active {
            format!("▶ {}", p.label)
        } else {
            format!("   {}", p.label)
        };
        draw_text_left(
            hdc,
            pad_x + 10,
            cy + 4,
            235,
            24,
            &name_lbl,
            if p.available {
                rgb(248, 250, 252)
            } else {
                rgb(100, 116, 139)
            },
            f_btn,
        );

        let avail_txt = if p.available {
            if p.recommended {
                "✅ Disponibile • ⭐ CONSIGLIATO".to_string()
            } else {
                "✅ Disponibile".to_string()
            }
        } else {
            format!("⚪ {}", p.availability_label)
        };
        draw_text_left(
            hdc,
            pad_x + 250,
            cy + 4,
            225,
            24,
            &avail_txt,
            if p.recommended {
                rgb(74, 222, 128)
            } else if p.available {
                rgb(125, 211, 252)
            } else {
                rgb(100, 116, 139)
            },
            f_small,
        );

        if p.available {
            // Visual Score Bar scaled to max_score + Uncapped Pt
            let bar_rc = RECT::new(pad_x + 480, cy + 8, pad_x + 584, cy + 24);
            let b_bg = CreateSolidBrush(rgb(11, 15, 25));
            FillRect(hdc, &bar_rc, b_bg);
            DeleteObject(b_bg);
            let fill_w = (((p.score as u64) * (bar_rc.width() as u64)) / (max_score as u64)) as i32;
            let bar_fg_rc = RECT::new(
                bar_rc.left,
                bar_rc.top,
                bar_rc.left + fill_w.clamp(4, bar_rc.width()),
                bar_rc.bottom,
            );
            let b_fg = CreateSolidBrush(if p.recommended {
                rgb(34, 197, 94)
            } else {
                rgb(14, 165, 233)
            });
            FillRect(hdc, &bar_fg_rc, b_fg);
            DeleteObject(b_fg);

            draw_text_left(
                hdc,
                pad_x + 590,
                cy + 4,
                86,
                24,
                &format!("{} Pt", p.score),
                rgb(253, 224, 71),
                f_btn,
            );
            draw_text_left(
                hdc,
                pad_x + 680,
                cy + 4,
                120,
                24,
                &format!("{:.1} FPS", p.estimated_fps),
                rgb(74, 222, 128),
                f_btn,
            );
            draw_text_left(
                hdc,
                pad_x + 810,
                cy + 4,
                175,
                24,
                &format!(
                    "~{:.1}% CPU ({:.2} ms)",
                    p.estimated_cpu_load_pct, p.avg_frame_ms
                ),
                rgb(203, 213, 225),
                f_small,
            );

            app.hit_zones.push(HitZone {
                rect: r_rc,
                action: UiAction::ApplyBenchmarkProfile(idx),
            });
        } else {
            draw_text_left(
                hdc,
                pad_x + 480,
                cy + 4,
                400,
                24,
                "— Hardware o driver non rilevato su questo PC —",
                rgb(100, 116, 139),
                f_small,
            );
        }

        cy += row_h;
    }

    // Footer Buttons of Performance & Benchmark Modal
    let fy = y0 + h_box - 44;
    register_button(
        hdc,
        app,
        RECT::new(pad_x, fy, pad_x + 280, fy + 34),
        "🚀 Esegui Test Prestazioni Ora",
        UiAction::RunHardwareBenchmarkNow,
        false,
        rgb(2, 132, 199),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(pad_x + 290, fy, pad_x + 640, fy + 34),
        &format!(
            "⭐ Applica Configurazione Consigliata ({})",
            app.benchmark_report.recommended_profile_label
        ),
        UiAction::ApplyRecommendedBenchmark,
        true,
        rgb(22, 163, 74),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 210, fy, x0 + w_box - 18, fy + 34),
        "✅ Salva e Chiudi",
        UiAction::ClosePerfModal,
        false,
        rgb(51, 65, 85),
        f_sub,
    );
}

// ============================================================================
// Modal 3: Calibrazione Avanzata (HSV Telo Verde, Soglie AI & Sfocatura Bokeh)
// ============================================================================

unsafe fn paint_advanced_modal_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let w_box = 780.min(win_w - 24);
    let h_box = 490.min(win_h - 24);
    let x0 = ((win_w - w_box) / 2).max(12);
    let y0 = ((win_h - h_box) / 2).max(12);
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(99, 102, 241));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 48);
    let tb_br = CreateSolidBrush(rgb(67, 56, 202));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    draw_text_left(
        hdc,
        x0 + 20,
        y0 + 10,
        w_box - 160,
        28,
        "⚙️ Calibrazione Avanzata — Telo Verde HSV, Soglie AI & Effetto Bokeh",
        rgb(255, 255, 255),
        f_title,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 124, y0 + 9, x0 + w_box - 14, y0 + 39),
        "✖ Chiudi",
        UiAction::CloseAdvancedModal,
        false,
        rgb(51, 65, 85),
        f_btn,
    );

    let gs = app.cfg.segmentation.green_screen.clone();
    let ai = app.cfg.segmentation.ai_selfie.clone();
    let rgb_col = app.cfg.compositor.background_color_rgb;
    let bg_blur = app.cfg.compositor.background_blur_radius;

    let pad_x = x0 + 24;
    let inner_w = w_box - 48;
    let mut cy = y0 + 62;

    let rows: [(&str, String, UiAction, UiAction); 8] = [
        (
            "Tonalità Verde (Hue Min):",
            format!("{}° (Range {}°..{}°)", gs.hue_min, gs.hue_min, gs.hue_max),
            UiAction::HueMinDec,
            UiAction::HueMinInc,
        ),
        (
            "Tonalità Verde (Hue Max):",
            format!("{}°", gs.hue_max),
            UiAction::HueMaxDec,
            UiAction::HueMaxInc,
        ),
        (
            "Saturazione Minima (Sat Min):",
            format!("{} / 255", gs.sat_min),
            UiAction::SatMinDec,
            UiAction::SatMinInc,
        ),
        (
            "Luminosità Minima (Val Min):",
            format!("{} / 255", gs.val_min),
            UiAction::ValMinDec,
            UiAction::ValMinInc,
        ),
        (
            "Morbidezza Bordi (Feather Blur):",
            format!("{} px", gs.blur_radius),
            UiAction::BlurDec,
            UiAction::BlurInc,
        ),
        (
            "Rimozione Alone Verde (Despill):",
            format!("{:.0}%", gs.spill_suppression * 100.0),
            UiAction::SpillDec,
            UiAction::SpillInc,
        ),
        (
            "Soglia Confidenza Maschera AI:",
            format!(
                "{:.0}% (EMA {:.0}%)",
                ai.confidence_threshold * 100.0,
                ai.temporal_smoothing * 100.0
            ),
            UiAction::AiConfDec,
            UiAction::AiConfInc,
        ),
        (
            "Raggio Sfocatura Sfondo Bokeh:",
            format!(
                "{} px (Colore RGB [{},{},{}])",
                bg_blur, rgb_col[0], rgb_col[1], rgb_col[2]
            ),
            UiAction::BgBlurDec,
            UiAction::BgBlurInc,
        ),
    ];

    for (lbl, val_str, act_dec, act_inc) in rows {
        draw_text_left(hdc, pad_x, cy, 260, 28, lbl, rgb(226, 232, 240), f_btn);
        register_button(
            hdc,
            app,
            RECT::new(pad_x + 270, cy, pad_x + 314, cy + 28),
            "−",
            act_dec,
            false,
            rgb(51, 65, 85),
            f_title,
        );
        draw_text_center(
            hdc,
            RECT::new(pad_x + 320, cy, pad_x + 560, cy + 28),
            &val_str,
            rgb(56, 189, 248),
            f_btn,
        );
        register_button(
            hdc,
            app,
            RECT::new(pad_x + 566, cy, pad_x + 610, cy + 28),
            "+",
            act_inc,
            false,
            rgb(51, 65, 85),
            f_title,
        );
        cy += 34;
    }

    cy += 8;
    let third_w = (inner_w - 16) / 3;
    register_button(
        hdc,
        app,
        RECT::new(pad_x, cy, pad_x + third_w, cy + 32),
        &format!(
            "🔬 Morfologia: {}x{}",
            gs.morph_kernel_size, gs.morph_kernel_size
        ),
        UiAction::MorphCycle,
        false,
        rgb(51, 65, 85),
        f_small,
    );
    register_button(
        hdc,
        app,
        RECT::new(pad_x + third_w + 8, cy, pad_x + third_w * 2 + 8, cy + 32),
        "🎨 Cambia Colore Tinta Unita RGB",
        UiAction::BgColorPresetCycle,
        false,
        rgb(79, 70, 229),
        f_small,
    );
    register_button(
        hdc,
        app,
        RECT::new(pad_x + third_w * 2 + 16, cy, pad_x + inner_w, cy + 32),
        "🪄 Auto-Calibra Telo Verde (1 Clic)",
        UiAction::AutoCalibrate1Click,
        false,
        rgb(21, 128, 61),
        f_small,
    );

    let fy = y0 + h_box - 44;
    register_button(
        hdc,
        app,
        RECT::new(pad_x, fy, pad_x + 280, fy + 32),
        "⚡ Apri Centro Prestazioni & GPU",
        UiAction::OpenPerfModal,
        false,
        rgb(2, 132, 199),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 230, fy, x0 + w_box - 24, fy + 32),
        "💾 Salva e Chiudi",
        UiAction::CloseAdvancedModal,
        true,
        rgb(22, 163, 74),
        f_sub,
    );
}

unsafe fn paint_open_dropdown_menu(
    hdc: HDC,
    app: &mut AppGuiState,
    kind: DropdownKind,
    anchor: RECT,
    win_h: i32,
    hfont: HFONT,
) {
    let ar = app.cfg.framing_output.aspect_ratio.clone();
    let cur_res = app.cfg.framing_output.resolution_preset.clone();
    let cur_fps = app.cfg.camera_input.fps;
    let cur_eng = app.cfg.segmentation.ai_selfie.engine_mode;

    let items: Vec<(String, UiAction, bool)> = match kind {
        DropdownKind::AspectRatio => vec![
            (
                "16:9 (Widescreen)".to_string(),
                UiAction::SelectAspectRatio(0),
                ar == "16:9",
            ),
            (
                "4:3 (Standard)".to_string(),
                UiAction::SelectAspectRatio(1),
                ar == "4:3",
            ),
            (
                "1:1 (Quadrato)".to_string(),
                UiAction::SelectAspectRatio(2),
                ar == "1:1",
            ),
            (
                "9:16 (Verticale)".to_string(),
                UiAction::SelectAspectRatio(3),
                ar == "9:16",
            ),
        ],
        DropdownKind::OutputResolution => {
            let presets = [
                (0u8, "1080p", "1080p Full HD"),
                (1u8, "900p", "900p HD+"),
                (2u8, "720p", "720p HD"),
                (3u8, "540p", "540p qHD"),
                (4u8, "480p", "480p SD"),
                (5u8, "360p", "360p Low"),
                (6u8, "240p", "240p Ultra-Low"),
            ];
            presets
                .iter()
                .map(|(idx, code, label)| {
                    let (w, h) =
                        crate::config::FramingOutputConfig::compute_output_dimensions_for_preset(
                            &ar, code,
                        );
                    (
                        format!("{} ({}×{})", label, w, h),
                        UiAction::SelectResolutionPreset(*idx),
                        cur_res.eq_ignore_ascii_case(code),
                    )
                })
                .collect()
        }
        DropdownKind::OutputFps => vec![
            (
                "60 FPS (Ultra Fluido)".to_string(),
                UiAction::SelectOutputFps(60),
                cur_fps == 60,
            ),
            (
                "30 FPS (Standard)".to_string(),
                UiAction::SelectOutputFps(30),
                cur_fps == 30,
            ),
            (
                "25 FPS (PAL)".to_string(),
                UiAction::SelectOutputFps(25),
                cur_fps == 25,
            ),
            (
                "24 FPS (Cinema)".to_string(),
                UiAction::SelectOutputFps(24),
                cur_fps == 24,
            ),
            (
                "20 FPS (Eco)".to_string(),
                UiAction::SelectOutputFps(20),
                cur_fps == 20,
            ),
            (
                "15 FPS (Risparmio CPU)".to_string(),
                UiAction::SelectOutputFps(15),
                cur_fps == 15,
            ),
            (
                "10 FPS (Minimo)".to_string(),
                UiAction::SelectOutputFps(10),
                cur_fps == 10,
            ),
        ],
        DropdownKind::SegmentationEngine => vec![
            (
                "🤖 Auto (Ibrido Intelligente)".to_string(),
                UiAction::SelectAiEngineMode(0),
                cur_eng == 0,
            ),
            (
                "🧑 Solo Sagoma & Volto (Senza Stanza Vuota)".to_string(),
                UiAction::SelectAiEngineMode(1),
                cur_eng == 1,
            ),
            (
                "📸 Differenza Sfondo (Stanza Vuota Prioritaria)".to_string(),
                UiAction::SelectAiEngineMode(2),
                cur_eng == 2,
            ),
        ],
    };

    let item_h = 26;
    let total_h = (items.len() as i32) * item_h + 4;
    let menu_y0 = if anchor.bottom + total_h + 44 > win_h {
        (anchor.top - total_h).max(8)
    } else {
        anchor.bottom + 1
    };
    let menu_w = anchor.width().max(220);
    let menu_rc = RECT::new(
        anchor.left,
        menu_y0,
        anchor.left + menu_w,
        menu_y0 + total_h,
    );

    let shadow_rc = RECT::new(
        menu_rc.left + 3,
        menu_rc.top + 3,
        menu_rc.right + 3,
        menu_rc.bottom + 3,
    );
    let sh_br = CreateSolidBrush(rgb(5, 8, 14));
    FillRect(hdc, &shadow_rc, sh_br);
    DeleteObject(sh_br);

    let bg_br = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &menu_rc, bg_br);
    DeleteObject(bg_br);

    let bd_br = CreateSolidBrush(rgb(56, 189, 248));
    FrameRect(hdc, &menu_rc, bd_br);
    DeleteObject(bd_br);

    for (i, (label, action, selected)) in items.into_iter().enumerate() {
        let iy = menu_y0 + 2 + (i as i32) * item_h;
        let item_rc = RECT::new(menu_rc.left + 2, iy, menu_rc.right - 2, iy + item_h);
        let hover = item_rc.contains(app.hover_pos.x, app.hover_pos.y);

        let fill_col = if selected {
            rgb(3, 105, 161)
        } else if hover {
            rgb(51, 65, 85)
        } else {
            rgb(20, 29, 47)
        };
        let ib = CreateSolidBrush(fill_col);
        FillRect(hdc, &item_rc, ib);
        DeleteObject(ib);

        let prefix = if selected { "✓ " } else { "    " };
        draw_text_left(
            hdc,
            item_rc.left + 8,
            item_rc.top,
            item_rc.width() - 14,
            item_h,
            &format!("{}{}", prefix, label),
            if selected || hover {
                rgb(255, 255, 255)
            } else {
                rgb(226, 232, 240)
            },
            hfont,
        );

        app.dropdown_overlay_zones.push(HitZone {
            rect: item_rc,
            action,
        });
    }
}

// ============================================================================
// Mandatory PEFL v1.0 Clause 3 Foreground License Approval Modal Overlay
// ============================================================================

unsafe fn paint_pefl_modal_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let full_rc = RECT::new(0, 0, win_w, win_h);
    let dim_br = CreateSolidBrush(rgb(9, 13, 22));
    FillRect(hdc, &full_rc, dim_br);
    DeleteObject(dim_br);

    let w_box = 840;
    let h_box = 560;
    let x0 = ((win_w - w_box) / 2).max(16);
    let y0 = ((win_h - h_box) / 2).max(16);
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(18, 24, 38));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(239, 68, 68));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 62);
    let tb_br = CreateSolidBrush(rgb(153, 27, 27));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    draw_text_left(
        hdc,
        x0 + 24,
        y0 + 6,
        w_box - 48,
        30,
        "📜 Personal and Educational Free License (PEFL) v1.0 — Clausola 3",
        rgb(255, 255, 255),
        f_title,
    );
    draw_text_left(
        hdc,
        x0 + 24,
        y0 + 34,
        w_box - 48,
        24,
        "Copyright (c) 2026 Rubens Rainelli — Approvazione Obbligatoria in Primo Piano prima dell'utilizzo",
        rgb(254, 202, 202),
        f_small,
    );

    let author_rc = RECT::new(x0 + 24, y0 + 74, x0 + w_box - 24, y0 + 112);
    let au_br = CreateSolidBrush(rgb(12, 17, 28));
    FillRect(hdc, &author_rc, au_br);
    DeleteObject(au_br);
    let au_bd = CreateSolidBrush(rgb(51, 65, 85));
    FrameRect(hdc, &author_rc, au_bd);
    DeleteObject(au_bd);

    draw_text_left(
        hdc,
        x0 + 36,
        y0 + 81,
        w_box - 310,
        24,
        "Autore: Rubens Rainelli  |  📧 rubens@rainelli.it  |  📞 (+39) 342 811 5882",
        rgb(125, 211, 252),
        f_btn,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + w_box - 266, y0 + 79, x0 + w_box - 32, y0 + 107),
        "🌐 Leggi Licenza Integrale",
        UiAction::PeflOpenFullLicense,
        false,
        rgb(30, 64, 175),
        f_small,
    );

    let terms_rc = RECT::new(x0 + 24, y0 + 122, x0 + w_box - 24, y0 + 428);
    let tr_br = CreateSolidBrush(rgb(11, 15, 25));
    FillRect(hdc, &terms_rc, tr_br);
    DeleteObject(tr_br);
    let tr_bd = CreateSolidBrush(rgb(51, 65, 85));
    FrameRect(hdc, &terms_rc, tr_bd);
    DeleteObject(tr_bd);

    let mut ty = y0 + 132;
    let tx = x0 + 38;
    let tw = w_box - 76;

    draw_text_left(
        hdc,
        tx,
        ty,
        tw,
        24,
        "✅ 1. CONCESSIONE D'USO GRATUITO (Uso Personale, Didattico e No-Profit):",
        rgb(74, 222, 128),
        f_sub,
    );
    ty += 24;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "Il software SmartCam è concesso gratuitamente (100% Free) esclusivamente a persone fisiche per uso",
        rgb(226, 232, 240),
        f_small,
    );
    ty += 20;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "personale o domestico, a studenti, docenti, istituti scolastici, università ed enti No-Profit.",
        rgb(226, 232, 240),
        f_small,
    );
    ty += 28;

    draw_text_left(
        hdc,
        tx,
        ty,
        tw,
        24,
        "🚫 2. DIVIETO TASSATIVO DI USO AZIENDALE, PROFESSIONALE O COMMERCIALE:",
        rgb(248, 113, 113),
        f_sub,
    );
    ty += 24;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "Qualsiasi installazione o utilizzo in contesti aziendali, societari, commerciali, professionali, governativi",
        rgb(226, 232, 240),
        f_small,
    );
    ty += 20;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "o su postazioni di lavoro aziendali è SEVERAMENTE VIETATO senza preventiva autorizzazione scritta",
        rgb(226, 232, 240),
        f_small,
    );
    ty += 20;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "dell'Autore (Rubens Rainelli — rubens@rainelli.it) e pagamento del relativo canone di licenza.",
        rgb(253, 224, 71),
        f_small,
    );
    ty += 28;

    draw_text_left(
        hdc,
        tx,
        ty,
        tw,
        24,
        "⛪ 3. ESCLUSIONE ENTI RELIGIOSI E LUOGHI DI CULTO:",
        rgb(251, 191, 36),
        f_sub,
    );
    ty += 24;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "Ai sensi della specifica PEFL v1.0, i luoghi di culto e le organizzazioni religiose di qualsiasi confessione",
        rgb(226, 232, 240),
        f_small,
    );
    ty += 20;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "sono esclusi dalla concessione gratuita ed equiparati a tutti gli effetti all'Uso Aziendale.",
        rgb(226, 232, 240),
        f_small,
    );
    ty += 28;

    draw_text_left(
        hdc,
        tx,
        ty,
        tw,
        24,
        "⚖️ 4. CLAUSOLA 3 — VERIFICA OBBLIGATORIA IN PRIMO PIANO (GARANZIA ASSENTE \"AS-IS\"):",
        rgb(56, 189, 248),
        f_sub,
    );
    ty += 24;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "Il software è fornito \"COSÌ COM'È\" senza garanzie. Premendo \"Accetto\", dichiari di aver letto e",
        rgb(203, 213, 225),
        f_small,
    );
    ty += 20;
    draw_text_left(
        hdc,
        tx + 14,
        ty,
        tw - 14,
        20,
        "accettato integralmente i termini della licenza PEFL v1.0 (https://github.com/RubensRainelli/pefl-license-spec).",
        rgb(203, 213, 225),
        f_small,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + 24, y0 + 440, x0 + w_box - 24, y0 + 476),
        if app.pefl_remember_7_days {
            "☑ Ricorda la mia approvazione della licenza PEFL v1.0 per i prossimi 7 giorni su questo PC"
        } else {
            "☐ Ricorda la mia approvazione della licenza PEFL v1.0 per i prossimi 7 giorni su questo PC (Chiedi a ogni avvio)"
        },
        UiAction::PeflToggleRemember,
        app.pefl_remember_7_days,
        rgb(3, 105, 161),
        f_btn,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + 24, y0 + h_box - 64, x0 + 275, y0 + h_box - 18),
        "❌ Rifiuto i Termini ed Esci",
        UiAction::PeflReject,
        false,
        rgb(153, 27, 27),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + 292, y0 + h_box - 64, x0 + w_box - 24, y0 + h_box - 18),
        "✅ Dichiaro Uso Personale/Didattico — Accetto PEFL v1.0 e Avvia SmartCam",
        UiAction::PeflAccept,
        true,
        rgb(22, 163, 74),
        f_sub,
    );
}

// ============================================================================
// Unified Background Gallery, File Picker & Interactive Social Crop Modal
// ============================================================================

unsafe fn paint_crop_modal_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let w_box = 900;
    let h_box = 610;
    let x0 = ((win_w - w_box) / 2).max(10);
    let y0 = ((win_h - h_box) / 2).max(10);
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(56, 189, 248));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 44);
    let tb_br = CreateSolidBrush(rgb(3, 105, 161));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    draw_text_left(
        hdc,
        x0 + 18,
        y0 + 5,
        w_box - 36,
        34,
        &format!(
            "🖼️ Galleria Sfondi Studio, Caricamento Immagine & Ritaglio — Formato {} ({}×{})",
            app.cfg.framing_output.aspect_ratio, app.out_w, app.out_h
        ),
        rgb(255, 255, 255),
        f_title,
    );

    draw_text_left(
        hdc,
        x0 + 20,
        y0 + 48,
        w_box - 40,
        20,
        "1. Scegli uno Sfondo Studio predefinito (clicca sulla miniatura) oppure carica una tua foto dal PC:",
        rgb(125, 211, 252),
        f_small,
    );

    let gal_y = y0 + 70;
    let thumb_card_w = 152;
    let thumb_card_h = 104;
    let cur_bg_path_low = app
        .cfg
        .compositor
        .background_image_path
        .to_ascii_lowercase();

    for i in 0..app.pipeline.studio_preset_thumbs.len() {
        let thumb_id = app.pipeline.studio_preset_thumbs[i].id;
        let thumb_label = app.pipeline.studio_preset_thumbs[i].label.clone();
        let thumb_w = app.pipeline.studio_preset_thumbs[i].width as i32;
        let thumb_h = app.pipeline.studio_preset_thumbs[i].height as i32;

        let tx0 = x0 + 20 + (i as i32) * (thumb_card_w + 10);
        let card_rc = RECT::new(tx0, gal_y, tx0 + thumb_card_w, gal_y + thumb_card_h);
        let is_active = app.cfg.compositor.background_mode == "image"
            && cur_bg_path_low.contains(&format!("preset_{}", thumb_id));
        let hover = card_rc.contains(app.hover_pos.x, app.hover_pos.y);

        let c_fill = CreateSolidBrush(if is_active {
            rgb(3, 105, 161)
        } else if hover {
            rgb(51, 65, 85)
        } else {
            rgb(22, 31, 48)
        });
        FillRect(hdc, &card_rc, c_fill);
        DeleteObject(c_fill);

        let mut bmi: BITMAPINFO = Default::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as DWORD;
        bmi.bmiHeader.biWidth = thumb_w;
        bmi.bmiHeader.biHeight = -thumb_h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        SetStretchBltMode(hdc, COLORONCOLOR);
        StretchDIBits(
            hdc,
            tx0 + 4,
            gal_y + 4,
            thumb_card_w - 8,
            74,
            0,
            0,
            thumb_w,
            thumb_h,
            app.pipeline.studio_preset_thumbs[i].pixels.as_ptr() as *const c_void,
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );

        let c_bd = CreateSolidBrush(if is_active {
            rgb(74, 222, 128)
        } else if hover {
            rgb(125, 211, 252)
        } else {
            rgb(71, 85, 105)
        });
        FrameRect(hdc, &card_rc, c_bd);
        DeleteObject(c_bd);

        draw_text_center(
            hdc,
            RECT::new(
                tx0 + 2,
                gal_y + 80,
                tx0 + thumb_card_w - 2,
                gal_y + thumb_card_h - 2,
            ),
            &thumb_label,
            if is_active {
                rgb(255, 255, 255)
            } else {
                rgb(226, 232, 240)
            },
            f_small,
        );

        app.hit_zones.push(HitZone {
            rect: card_rc,
            action: UiAction::SelectStudioPreset(thumb_id),
        });
    }

    let custom_x0 = x0 + 20 + 4 * (thumb_card_w + 10);
    let custom_rc = RECT::new(custom_x0, gal_y, x0 + w_box - 20, gal_y + thumb_card_h);
    register_button(
        hdc,
        app,
        custom_rc,
        "📂 Carica Immagine dal PC...",
        UiAction::OpenCustomBgImageDialog,
        false,
        rgb(14, 116, 144),
        f_btn,
    );

    draw_text_left(
        hdc,
        x0 + 20,
        gal_y + thumb_card_h + 6,
        w_box - 40,
        20,
        "2. Trascina il riquadro sull'immagine per centrare l'inquadratura e usa la rotella del mouse o lo slider per lo Zoom:",
        rgb(203, 213, 225),
        f_small,
    );

    let editor_top = gal_y + thumb_card_h + 28;
    let editor_bottom = y0 + h_box - 92;

    let canvas_area = RECT::new(x0 + 20, editor_top, x0 + 555, editor_bottom);
    let ca_br = CreateSolidBrush(rgb(8, 11, 18));
    FillRect(hdc, &canvas_area, ca_br);
    DeleteObject(ca_br);

    let src_w = app.pipeline.bg_source_w.max(16) as i32;
    let src_h = app.pipeline.bg_source_h.max(16) as i32;
    let max_cw = canvas_area.width() - 16;
    let max_ch = canvas_area.height() - 16;
    let (disp_w, disp_h) = if src_w * max_ch > src_h * max_cw {
        (max_cw, (max_cw * src_h) / src_w.max(1))
    } else {
        ((max_ch * src_w) / src_h.max(1), max_ch)
    };
    let cx0 = canvas_area.left + (canvas_area.width() - disp_w) / 2;
    let cy0 = canvas_area.top + (canvas_area.height() - disp_h) / 2;
    app.crop_canvas_rect = RECT::new(cx0, cy0, cx0 + disp_w, cy0 + disp_h);

    if !app.pipeline.bg_source_image.is_empty() {
        let (rx0, ry0, rw, rh) = compute_social_crop_rect(
            app.pipeline.bg_source_w,
            app.pipeline.bg_source_h,
            app.out_w.max(16),
            app.out_h.max(16),
            app.cfg.compositor.background_zoom,
            app.cfg.compositor.background_pan_x,
            app.cfg.compositor.background_pan_y,
        );

        let needed_modal_bytes = app.pipeline.bg_source_image.len();
        if app.crop_modal_scratch.len() != needed_modal_bytes {
            app.crop_modal_scratch.resize(needed_modal_bytes, 0);
        }
        let sw = app.pipeline.bg_source_w as usize;
        let sh = app.pipeline.bg_source_h as usize;
        let mir_h = app.cfg.compositor.background_mirror_h;
        let mir_v = app.cfg.compositor.background_mirror_v;
        for y in 0..sh {
            let in_y = (y as u32) >= ry0 && (y as u32) < ry0 + rh;
            let sy = if mir_v { sh.saturating_sub(1 + y) } else { y };
            for x in 0..sw {
                let in_x = (x as u32) >= rx0 && (x as u32) < rx0 + rw;
                let sx = if mir_h { sw.saturating_sub(1 + x) } else { x };
                let src_idx = (sy * sw + sx) * 4;
                let dst_idx = (y * sw + x) * 4;
                let mut b = app.pipeline.bg_source_image[src_idx];
                let mut g = app.pipeline.bg_source_image[src_idx + 1];
                let mut r = app.pipeline.bg_source_image[src_idx + 2];
                if !(in_x && in_y) {
                    b = (b as u16 * 35 / 100) as u8;
                    g = (g as u16 * 35 / 100) as u8;
                    r = (r as u16 * 35 / 100) as u8;
                }
                app.crop_modal_scratch[dst_idx] = b;
                app.crop_modal_scratch[dst_idx + 1] = g;
                app.crop_modal_scratch[dst_idx + 2] = r;
                app.crop_modal_scratch[dst_idx + 3] = 255;
            }
        }

        let mut bmi: BITMAPINFO = Default::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as DWORD;
        bmi.bmiHeader.biWidth = src_w;
        bmi.bmiHeader.biHeight = -src_h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        SetStretchBltMode(hdc, COLORONCOLOR);
        StretchDIBits(
            hdc,
            cx0,
            cy0,
            disp_w,
            disp_h,
            0,
            0,
            src_w,
            src_h,
            app.crop_modal_scratch.as_ptr() as *const c_void,
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );

        let box_l = cx0 + ((rx0 as i32) * disp_w) / src_w.max(1);
        let box_t = cy0 + ((ry0 as i32) * disp_h) / src_h.max(1);
        let box_r = cx0 + (((rx0 + rw) as i32) * disp_w) / src_w.max(1);
        let box_b = cy0 + (((ry0 + rh) as i32) * disp_h) / src_h.max(1);
        let crop_box = RECT::new(box_l, box_t, box_r, box_b);

        let cyan_br = CreateSolidBrush(rgb(56, 189, 248));
        FrameRect(hdc, &crop_box, cyan_br);
        let inner_box = RECT::new(box_l + 1, box_t + 1, box_r - 1, box_b - 1);
        FrameRect(hdc, &inner_box, cyan_br);
        DeleteObject(cyan_br);
    }

    let prev_area = RECT::new(x0 + 570, editor_top, x0 + w_box - 20, editor_bottom);
    let pa_br = CreateSolidBrush(rgb(8, 11, 18));
    FillRect(hdc, &prev_area, pa_br);
    DeleteObject(pa_br);

    draw_text_center(
        hdc,
        RECT::new(
            prev_area.left,
            prev_area.top + 6,
            prev_area.right,
            prev_area.top + 28,
        ),
        "Anteprima Sfondo Ritagliato:",
        rgb(125, 211, 252),
        f_sub,
    );

    let pw_avail = prev_area.width() - 24;
    let ph_avail = prev_area.height() - 68;
    let ow = app.out_w.max(16) as i32;
    let oh = app.out_h.max(16) as i32;
    let (pw, ph) = if ow * ph_avail > oh * pw_avail {
        (pw_avail, (pw_avail * oh) / ow.max(1))
    } else {
        ((ph_avail * ow) / oh.max(1), ph_avail)
    };
    let px0 = prev_area.left + (prev_area.width() - pw) / 2;
    let py0 = prev_area.top + 34 + (ph_avail - ph) / 2;

    if !app.pipeline.bg_cropped_cache.is_empty() {
        let mut bmi: BITMAPINFO = Default::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as DWORD;
        bmi.bmiHeader.biWidth = ow;
        bmi.bmiHeader.biHeight = -oh;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        SetStretchBltMode(hdc, COLORONCOLOR);
        StretchDIBits(
            hdc,
            px0,
            py0,
            pw,
            ph,
            0,
            0,
            ow,
            oh,
            app.pipeline.bg_cropped_cache.as_ptr() as *const c_void,
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
    let p_border = CreateSolidBrush(rgb(74, 222, 128));
    FrameRect(hdc, &RECT::new(px0, py0, px0 + pw, py0 + ph), p_border);
    DeleteObject(p_border);

    draw_text_center(
        hdc,
        RECT::new(
            prev_area.left + 8,
            prev_area.bottom - 28,
            prev_area.right - 8,
            prev_area.bottom - 4,
        ),
        &format!(
            "Zoom: {:.0}% | Pan: ({:.0}%, {:.0}%)",
            app.cfg.compositor.background_zoom * 100.0,
            app.cfg.compositor.background_pan_x * 100.0,
            app.cfg.compositor.background_pan_y * 100.0
        ),
        rgb(148, 163, 184),
        f_small,
    );

    let ctrl_y = y0 + h_box - 82;
    draw_text_left(
        hdc,
        x0 + 20,
        ctrl_y,
        58,
        30,
        "Zoom:",
        rgb(226, 232, 240),
        f_sub,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + 78, ctrl_y, x0 + 114, ctrl_y + 30),
        "−",
        UiAction::CropZoomOut,
        false,
        rgb(51, 65, 85),
        f_title,
    );

    let slider_rc = RECT::new(x0 + 124, ctrl_y + 5, x0 + 374, ctrl_y + 25);
    app.crop_slider_rect = slider_rc;
    let sl_bg = CreateSolidBrush(rgb(30, 41, 59));
    FillRect(hdc, &slider_rc, sl_bg);
    DeleteObject(sl_bg);

    let zoom_norm = ((app.cfg.compositor.background_zoom - 1.0) / 3.0).clamp(0.0, 1.0);
    let fill_w = (slider_rc.width() as f32 * zoom_norm).round() as i32;
    let sl_fill = RECT::new(
        slider_rc.left,
        slider_rc.top,
        slider_rc.left + fill_w.max(6),
        slider_rc.bottom,
    );
    let sl_fg = CreateSolidBrush(rgb(2, 132, 199));
    FillRect(hdc, &sl_fill, sl_fg);
    DeleteObject(sl_fg);
    let sl_bd = CreateSolidBrush(rgb(125, 211, 252));
    FrameRect(hdc, &slider_rc, sl_bd);
    DeleteObject(sl_bd);

    register_button(
        hdc,
        app,
        RECT::new(x0 + 384, ctrl_y, x0 + 420, ctrl_y + 30),
        "+",
        UiAction::CropZoomIn,
        false,
        rgb(51, 65, 85),
        f_title,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + 428, ctrl_y, x0 + 544, ctrl_y + 30),
        "🔄 Reset 100%",
        UiAction::CropReset,
        false,
        rgb(51, 65, 85),
        f_small,
    );

    let mir_h = app.cfg.compositor.background_mirror_h;
    let mir_v = app.cfg.compositor.background_mirror_v;
    register_button(
        hdc,
        app,
        RECT::new(x0 + 554, ctrl_y, x0 + 738, ctrl_y + 30),
        if mir_h {
            "↔️ Specchia Orizz.: ON"
        } else {
            "↔️ Specchia Orizz.: OFF"
        },
        UiAction::ToggleBgMirrorH,
        mir_h,
        rgb(3, 105, 161),
        f_small,
    );
    register_button(
        hdc,
        app,
        RECT::new(x0 + 746, ctrl_y, x0 + w_box - 20, ctrl_y + 30),
        if mir_v {
            "↕️ Specchia Vert.: ON"
        } else {
            "↕️ Specchia Vert.: OFF"
        },
        UiAction::ToggleBgMirrorV,
        mir_v,
        rgb(3, 105, 161),
        f_small,
    );

    register_button(
        hdc,
        app,
        RECT::new(x0 + 20, y0 + h_box - 44, x0 + 270, y0 + h_box - 12),
        "📂 Scegli Altra Immagine dal PC...",
        UiAction::OpenCustomBgImageDialog,
        false,
        rgb(3, 105, 161),
        f_btn,
    );
    register_button(
        hdc,
        app,
        RECT::new(
            x0 + w_box - 260,
            y0 + h_box - 44,
            x0 + w_box - 20,
            y0 + h_box - 12,
        ),
        "✅ Applica Sfondo e Chiudi",
        UiAction::CropApplyAndClose,
        true,
        rgb(22, 163, 74),
        f_sub,
    );
}

unsafe fn paint_wizard_overlay(
    hdc: HDC,
    app: &mut AppGuiState,
    win_w: i32,
    win_h: i32,
    f_title: HFONT,
    f_sub: HFONT,
    f_btn: HFONT,
    f_small: HFONT,
) {
    let w_box = 680;
    let h_box = 370;
    let x0 = (win_w - w_box) / 2;
    let y0 = (win_h - h_box) / 2;
    let card = RECT::new(x0, y0, x0 + w_box, y0 + h_box);

    let bg = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &card, bg);
    DeleteObject(bg);

    let border = CreateSolidBrush(rgb(56, 189, 248));
    FrameRect(hdc, &card, border);
    DeleteObject(border);

    let top_banner = RECT::new(x0 + 1, y0 + 1, x0 + w_box - 1, y0 + 52);
    let tb_br = CreateSolidBrush(rgb(3, 105, 161));
    FillRect(hdc, &top_banner, tb_br);
    DeleteObject(tb_br);

    let step_title = match app.wizard_step {
        1 => "🧭 Guida Rapida SmartCam — Passo 1 di 3: La tua Fotocamera",
        2 => "🧭 Guida Rapida SmartCam — Passo 2 di 3: Scegli il tuo Effetto",
        _ => "🧭 Guida Rapida SmartCam — Passo 3 di 3: Pronto per le Videochiamate!",
    };
    draw_text_left(
        hdc,
        x0 + 20,
        y0 + 8,
        w_box - 40,
        36,
        step_title,
        rgb(255, 255, 255),
        f_title,
    );

    match app.wizard_step {
        1 => {
            let cam_name = app
                .selected_camera_entry()
                .map(|c| c.display_label())
                .unwrap_or_else(|| "Webcam Principale HD".to_string());
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 74,
                w_box - 56,
                28,
                "✅ Benvenuto in SmartCam! Abbiamo già configurato tutto per te:",
                rgb(74, 222, 128),
                f_sub,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 114,
                w_box - 56,
                24,
                &format!("• Fotocamera a colori selezionata: {}", cam_name),
                rgb(248, 250, 252),
                f_btn,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 148,
                w_box - 56,
                24,
                "• Puoi anche usare il tuo telefono come webcam tramite il pulsante '📱 Usa Smartphone'!",
                rgb(203, 213, 225),
                f_small,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 178,
                w_box - 56,
                24,
                &format!(
                    "• Benchmark Hardware automatico: {}",
                    app.benchmark_report.recommended_profile_label
                ),
                rgb(56, 189, 248),
                f_btn,
            );
        }
        2 => {
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 70,
                w_box - 56,
                28,
                "Clicca sull'effetto che preferisci usare nelle tue videochiamate:",
                rgb(248, 250, 252),
                f_sub,
            );
            let bw = (w_box - 76) / 2;
            register_button(
                hdc,
                app,
                RECT::new(x0 + 28, y0 + 112, x0 + 28 + bw, y0 + 168),
                "💧 Sfoca il mio Sfondo (Consigliato)",
                UiAction::WizardSelectEffect(1),
                app.cfg.segmentation.mode == "ai_selfie"
                    && app.cfg.compositor.background_mode == "blur",
                rgb(2, 132, 199),
                f_btn,
            );
            register_button(
                hdc,
                app,
                RECT::new(x0 + 48 + bw, y0 + 112, x0 + w_box - 28, y0 + 168),
                "🤖 Sfondo Ufficio AI (Foto / Preset)",
                UiAction::WizardSelectEffect(2),
                app.cfg.segmentation.mode == "ai_selfie"
                    && app.cfg.compositor.background_mode != "blur",
                rgb(2, 132, 199),
                f_btn,
            );
            register_button(
                hdc,
                app,
                RECT::new(x0 + 28, y0 + 184, x0 + 28 + bw, y0 + 240),
                "🟩 Ho un Telo Verde (Green Screen)",
                UiAction::WizardSelectEffect(3),
                app.cfg.segmentation.mode == "green_screen",
                rgb(22, 163, 74),
                f_btn,
            );
            register_button(
                hdc,
                app,
                RECT::new(x0 + 48 + bw, y0 + 184, x0 + w_box - 28, y0 + 240),
                "🎥 Solo Inquadratura Originale",
                UiAction::WizardSelectEffect(0),
                app.cfg.segmentation.mode == "passthrough",
                rgb(51, 65, 85),
                f_btn,
            );
        }
        _ => {
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 74,
                w_box - 56,
                28,
                "🎉 Tutto pronto! Ecco come selezionarla nei tuoi programmi:",
                rgb(74, 222, 128),
                f_sub,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 114,
                w_box - 56,
                24,
                "• Su Zoom / Microsoft Teams / Google Meet / WhatsApp Desktop:",
                rgb(248, 250, 252),
                f_btn,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 144,
                w_box - 56,
                26,
                "  Apri le Impostazioni Video e scegli la fotocamera  \"SmartCam\"",
                rgb(56, 189, 248),
                f_sub,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 186,
                w_box - 56,
                24,
                "• Quando chiudi SmartCam con la X, rimane attiva vicino all'orologio",
                rgb(203, 213, 225),
                f_small,
            );
            draw_text_left(
                hdc,
                x0 + 28,
                y0 + 210,
                w_box - 56,
                24,
                "  e si accende da sola solo quando inizi una videochiamata!",
                rgb(203, 213, 225),
                f_small,
            );
        }
    }

    register_button(
        hdc,
        app,
        RECT::new(x0 + 28, y0 + h_box - 56, x0 + 170, y0 + h_box - 16),
        "Chiudi Guida",
        UiAction::WizardClose,
        false,
        rgb(51, 65, 85),
        f_btn,
    );
    if app.wizard_step > 1 {
        register_button(
            hdc,
            app,
            RECT::new(
                x0 + w_box - 330,
                y0 + h_box - 56,
                x0 + w_box - 190,
                y0 + h_box - 16,
            ),
            "◀ Indietro",
            UiAction::WizardPrev,
            false,
            rgb(51, 65, 85),
            f_btn,
        );
    }
    register_button(
        hdc,
        app,
        RECT::new(
            x0 + w_box - 178,
            y0 + h_box - 56,
            x0 + w_box - 28,
            y0 + h_box - 16,
        ),
        if app.wizard_step < 3 {
            "Avanti ▶"
        } else {
            "✅ Inizia Ora!"
        },
        UiAction::WizardNext,
        true,
        rgb(2, 132, 199),
        f_btn,
    );
}

// ============================================================================
// Main Window Procedure
// ============================================================================

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let app_ptr = GLOBAL_APP;
    if app_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let app = &mut *app_ptr;

    match msg {
        WM_CREATE => {
            register_tray_icon(hwnd, app);
            let interval_ms = (1000 / app.cfg.camera_input.fps.max(1)).max(15);
            SetTimer(hwnd, TIMER_FRAME_ID, interval_ms, std::ptr::null());
            0
        }
        WM_TIMER => {
            if wparam == TIMER_FRAME_ID {
                let updated = app.tick_frame();
                if updated && !app.minimized_to_tray {
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                }
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint_main_window(hwnd, app);
            0
        }
        WM_KEYDOWN => {
            if app.plugins_modal_open && app.phone_port_input_focused {
                let vk = wparam as u32;
                if vk == 0x0D {
                    // VK_RETURN (Enter) -> Apply manual port immediately
                    app.handle_action(hwnd, UiAction::ApplyManualPhonePort);
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                } else if vk == 0x1B {
                    // VK_ESCAPE -> Cancel focus and restore active port text
                    app.phone_port_input_focused = false;
                    app.phone_port_input_text = app.plugin_registry.phone_server_port().to_string();
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                } else if vk == 0x2E {
                    // VK_DELETE -> Clear field
                    app.phone_port_input_text.clear();
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                }
            }
            0
        }
        WM_CHAR => {
            if app.plugins_modal_open && app.phone_port_input_focused {
                let ch_code = (wparam & 0xFFFF) as u16;
                if ch_code == 0x08 {
                    // Backspace
                    app.phone_port_fresh_focus = false;
                    app.phone_port_input_text.pop();
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                } else if (b'0' as u16..=b'9' as u16).contains(&ch_code) {
                    if app.phone_port_fresh_focus {
                        app.phone_port_input_text.clear();
                        app.phone_port_fresh_focus = false;
                    }
                    if app.phone_port_input_text.len() < 5 {
                        app.phone_port_input_text.push((ch_code as u8) as char);
                    }
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let old_pos = app.hover_pos;
            app.hover_pos = POINT { x, y };

            if app.crop_modal_open && app.crop_dragging {
                let cw = app.crop_canvas_rect.width().max(1) as f32;
                let ch = app.crop_canvas_rect.height().max(1) as f32;
                let dx = (x - app.crop_drag_start_mouse.x) as f32 / cw;
                let dy = (y - app.crop_drag_start_mouse.y) as f32 / ch;
                app.cfg.compositor.background_pan_x =
                    (app.crop_drag_start_pan.0 + dx * 1.4).clamp(0.0, 1.0);
                app.cfg.compositor.background_pan_y =
                    (app.crop_drag_start_pan.1 + dy * 1.4).clamp(0.0, 1.0);
                app.pipeline.rebuild_cropped_background(
                    app.out_w.max(16),
                    app.out_h.max(16),
                    app.cfg.compositor.background_zoom,
                    app.cfg.compositor.background_pan_x,
                    app.cfg.compositor.background_pan_y,
                    app.cfg.compositor.background_mirror_h,
                    app.cfg.compositor.background_mirror_v,
                );
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
                return 0;
            }

            if !app.pefl_modal_open
                && ((app.face_calib_modal_open && app.face_calib_photo_rect.contains(x, y))
                    || (app.crop_modal_open && app.crop_canvas_rect.contains(x, y))
                    || (!app.any_modal_open()
                        && app.open_dropdown.is_none()
                        && app.video_preview_rect.contains(x, y)))
            {
                SetCursor(LoadCursorW(NULL, IDC_CROSS));
            } else {
                SetCursor(LoadCursorW(NULL, IDC_ARROW));
            }

            let old_hit = app
                .hit_zones
                .iter()
                .position(|z| z.rect.contains(old_pos.x, old_pos.y));
            let new_hit = app.hit_zones.iter().position(|z| z.rect.contains(x, y));
            let old_dd = app
                .dropdown_overlay_zones
                .iter()
                .position(|z| z.rect.contains(old_pos.x, old_pos.y));
            let new_dd = app
                .dropdown_overlay_zones
                .iter()
                .position(|z| z.rect.contains(x, y));
            if old_hit != new_hit || old_dd != new_dd {
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            // 1. Floating GDI dropdown menu hit test
            if !app.any_modal_open() && app.open_dropdown.is_some() {
                for zone in app.dropdown_overlay_zones.iter() {
                    if zone.rect.contains(x, y) {
                        let act = zone.action;
                        app.handle_action(hwnd, act);
                        InvalidateRect(hwnd, std::ptr::null(), FALSE);
                        return 0;
                    }
                }
                for zone in app.hit_zones.iter().rev() {
                    if zone.rect.contains(x, y) {
                        if let UiAction::ToggleDropdown(_) = zone.action {
                            let act = zone.action;
                            app.handle_action(hwnd, act);
                            InvalidateRect(hwnd, std::ptr::null(), FALSE);
                            return 0;
                        }
                    }
                }
                app.open_dropdown = None;
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
                return 0;
            }

            // 2. Interactive 5-Point Face Calibration Snapshot Click
            if !app.pefl_modal_open
                && app.face_calib_modal_open
                && app.face_calib_photo_rect.contains(x, y)
            {
                let pr = app.face_calib_photo_rect;
                let nx = ((x - pr.left) as f32 / (pr.width().max(1) as f32)).clamp(0.02, 0.98);
                let ny = ((y - pr.top) as f32 / (pr.height().max(1) as f32)).clamp(0.02, 0.98);

                let idx = if app.face_calib_step < 5 {
                    app.face_calib_step
                } else {
                    // Clicking again after 5 points restarts from Point 1 (Right Eye)
                    0
                };
                app.face_calib_points[idx] = (nx, ny);
                app.face_calib_step = idx + 1;

                if app.face_calib_step >= 5 {
                    let snap_clone = app.face_calib_snapshot_buf.clone();
                    let sw = app.face_calib_snap_w;
                    let sh = app.face_calib_snap_h;
                    let pts = app.face_calib_points;
                    app.pipeline.calibrate_face_from_landmarks(
                        &snap_clone,
                        sw,
                        sh,
                        pts,
                        &mut app.cfg,
                    );
                    if app.cfg.segmentation.mode == "passthrough" {
                        app.cfg.segmentation.mode = "ai_selfie".to_string();
                    }
                    let msg = format!(
                        "✅ Calibrazione Volto a 5 Punti completata! Pelle YCbCr=({:.0}, {:.0}) • Raggio Testa=({:.0}%, {:.0}%)",
                        app.cfg.segmentation.ai_selfie.calib_skin_cb,
                        app.cfg.segmentation.ai_selfie.calib_skin_cr,
                        app.cfg.segmentation.ai_selfie.calib_head_rx * 100.0,
                        app.cfg.segmentation.ai_selfie.calib_head_ry * 100.0
                    );
                    app.apply_and_save_config(&msg);
                } else {
                    let next_lbl = match app.face_calib_step {
                        1 => "Passo 2/5: Clicca sul tuo OCCHIO SINISTRO",
                        2 => "Passo 3/5: Clicca sulla PUNTA DEL NASO",
                        3 => "Passo 4/5: Clicca al CENTRO DELLE LABBRA",
                        _ => "Passo 5/5: Clicca sulla PUNTA DEL MENTO",
                    };
                    app.status_message = format!(
                        "🎯 Punto {}/5 registrato ({:.0}%, {:.0}%) — {}",
                        app.face_calib_step,
                        nx * 100.0,
                        ny * 100.0,
                        next_lbl
                    );
                }
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
                return 0;
            }

            // 3. Interactive Crop Modal Canvas or Zoom Slider
            if !app.pefl_modal_open && app.crop_modal_open {
                if app.crop_slider_rect.contains(x, y) {
                    let norm = ((x - app.crop_slider_rect.left) as f32
                        / (app.crop_slider_rect.width().max(1) as f32))
                        .clamp(0.0, 1.0);
                    app.cfg.compositor.background_zoom = (1.0 + norm * 3.0).clamp(1.0, 4.0);
                    app.pipeline.rebuild_cropped_background(
                        app.out_w.max(16),
                        app.out_h.max(16),
                        app.cfg.compositor.background_zoom,
                        app.cfg.compositor.background_pan_x,
                        app.cfg.compositor.background_pan_y,
                        app.cfg.compositor.background_mirror_h,
                        app.cfg.compositor.background_mirror_v,
                    );
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                }
                if app.crop_canvas_rect.contains(x, y) {
                    app.crop_dragging = true;
                    app.crop_drag_start_mouse = POINT { x, y };
                    app.crop_drag_start_pan = (
                        app.cfg.compositor.background_pan_x,
                        app.cfg.compositor.background_pan_y,
                    );
                    SetCapture(hwnd);
                    return 0;
                }
            }

            // 3. UI Buttons (reverse order so top overlays win)
            for zone in app.hit_zones.iter().rev() {
                if zone.rect.contains(x, y) {
                    let act = zone.action;
                    app.handle_action(hwnd, act);
                    InvalidateRect(hwnd, std::ptr::null(), FALSE);
                    return 0;
                }
            }

            // 4. Contagocce (Eyedropper) click directly inside Live Video Preview!
            if !app.any_modal_open() && app.video_preview_rect.contains(x, y) {
                let vr = app.video_preview_rect;
                let fx = ((x - vr.left) * (app.out_w as i32)) / vr.width().max(1);
                let fy = ((y - vr.top) * (app.out_h as i32)) / vr.height().max(1);
                app.cfg.segmentation.mode = "green_screen".to_string();
                let frame_clone = app.pipeline.framed_buf.clone();
                let (w, h) = (app.out_w, app.out_h);
                let (hmin, hmax, smin, vmin) = app.pipeline.auto_calibrate_greenscreen(
                    &frame_clone,
                    w,
                    h,
                    fx,
                    fy,
                    &mut app.cfg,
                );
                let msg = format!(
                    "🎯 Contagocce Telo Verde calibrato sul punto ({}, {})! Hue=[{}..{}], SatMin={}, ValMin={}",
                    fx, fy, hmin, hmax, smin, vmin
                );
                app.apply_and_save_config(&msg);
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
                return 0;
            }
            0
        }
        WM_LBUTTONUP => {
            if app.crop_dragging {
                app.crop_dragging = false;
                ReleaseCapture();
                let _ = app.cfg.save_to_file(&app.config_path);
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
            }
            0
        }
        WM_MOUSEWHEEL => {
            if !app.pefl_modal_open && app.crop_modal_open {
                let delta = ((wparam >> 16) & 0xFFFF) as i16;
                let step = if delta > 0 { 0.12 } else { -0.12 };
                app.cfg.compositor.background_zoom =
                    (app.cfg.compositor.background_zoom + step).clamp(1.0, 4.0);
                app.pipeline.rebuild_cropped_background(
                    app.out_w.max(16),
                    app.out_h.max(16),
                    app.cfg.compositor.background_zoom,
                    app.cfg.compositor.background_pan_x,
                    app.cfg.compositor.background_pan_y,
                    app.cfg.compositor.background_mirror_h,
                    app.cfg.compositor.background_mirror_v,
                );
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
                return 0;
            }
            0
        }
        WM_TRAYICON => {
            let event = (lparam & 0xFFFF) as UINT;
            if event == WM_LBUTTONUP {
                app.minimized_to_tray = false;
                if app.on_demand_sleeping && app.pefl_accepted_session {
                    app.on_demand_sleeping = false;
                    let _ = app.pipeline.open_hardware_camera(
                        app.cfg.camera_input.device_index,
                        app.cfg.camera_input.width,
                        app.cfg.camera_input.height,
                        app.cfg.camera_input.fps,
                    );
                }
                ShowWindow(hwnd, SW_RESTORE);
                ShowWindow(hwnd, SW_SHOWNORMAL);
                BringWindowToTop(hwnd);
                SetForegroundWindow(hwnd);
                InvalidateRect(hwnd, std::ptr::null(), FALSE);
            } else if event == WM_RBUTTONUP {
                show_tray_context_menu(hwnd, app);
            }
            0
        }
        WM_SMARTCAM_SHOW_EXISTING => {
            app.minimized_to_tray = false;
            if app.on_demand_sleeping && app.pefl_accepted_session {
                app.on_demand_sleeping = false;
                let _ = app.pipeline.open_hardware_camera(
                    app.cfg.camera_input.device_index,
                    app.cfg.camera_input.width,
                    app.cfg.camera_input.height,
                    app.cfg.camera_input.fps,
                );
            }
            ShowWindow(hwnd, SW_RESTORE);
            ShowWindow(hwnd, SW_SHOWNORMAL);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
            0
        }
        WM_SMARTCAM_FORCE_EXIT => {
            KillTimer(hwnd, TIMER_FRAME_ID);
            remove_tray_icon(hwnd);
            DestroyWindow(hwnd);
            PostQuitMessage(0);
            0
        }
        WM_CLOSE => {
            if !app.pefl_accepted_session {
                remove_tray_icon(hwnd);
                DestroyWindow(hwnd);
                return 0;
            }
            minimize_window_to_tray(hwnd, app);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_FRAME_ID);
            remove_tray_icon(hwnd);
            app.release_gdi_resources();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

struct SingleInstanceAlertState {
    existing_hwnd: HWND,
    hover_pos: POINT,
    btn_use_existing_rect: RECT,
    btn_kill_all_rect: RECT,
}

static mut GLOBAL_ALERT_STATE: *mut SingleInstanceAlertState = std::ptr::null_mut();

unsafe fn bring_existing_instance_to_foreground(existing_hwnd: HWND) {
    let _ = AllowSetForegroundWindow(ASFW_ANY);
    let target = if !existing_hwnd.is_null() {
        existing_hwnd
    } else {
        let class_name = to_wstring("SmartCamMainNativeWindowClass");
        FindWindowW(class_name.as_ptr(), std::ptr::null())
    };
    if !target.is_null() {
        let _ = PostMessageW(target, WM_SMARTCAM_SHOW_EXISTING, 0, 0);
        let _ = ShowWindowAsync(target, SW_RESTORE);
        let _ = ShowWindowAsync(target, SW_SHOWNORMAL);
        let _ = BringWindowToTop(target);
        let _ = SetForegroundWindow(target);
    }
}

unsafe fn terminate_all_smartcam_instances(existing_hwnd: HWND) {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let class_name = to_wstring("SmartCamMainNativeWindowClass");
    let mut target = if !existing_hwnd.is_null() {
        existing_hwnd
    } else {
        FindWindowW(class_name.as_ptr(), std::ptr::null())
    };
    if !target.is_null() {
        let _ = PostMessageW(target, WM_SMARTCAM_FORCE_EXIT, 0, 0);
    }
    target = FindWindowW(class_name.as_ptr(), std::ptr::null());
    if !target.is_null() {
        let _ = PostMessageW(target, WM_SMARTCAM_FORCE_EXIT, 0, 0);
    }

    let my_pid = GetCurrentProcessId();
    let filter_arg = format!("PID ne {}", my_pid);
    let _ = Command::new("taskkill.exe")
        .args(["/F", "/IM", "SmartCam.exe", "/FI", &filter_arg])
        .creation_flags(0x0800_0000)
        .output();
}

unsafe extern "system" fn single_instance_alert_wnd_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let st = if GLOBAL_ALERT_STATE.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    } else {
        &mut *GLOBAL_ALERT_STATE
    };

    match msg {
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            st.hover_pos = POINT { x, y };
            let on_btn =
                st.btn_use_existing_rect.contains(x, y) || st.btn_kill_all_rect.contains(x, y);
            SetCursor(LoadCursorW(NULL, if on_btn { IDC_HAND } else { IDC_ARROW }));
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
            0
        }
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            if st.btn_use_existing_rect.contains(x, y) {
                bring_existing_instance_to_foreground(st.existing_hwnd);
                DestroyWindow(hwnd);
                return 0;
            }
            if st.btn_kill_all_rect.contains(x, y) {
                terminate_all_smartcam_instances(st.existing_hwnd);
                DestroyWindow(hwnd);
                return 0;
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc_win = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = Default::default();
            GetClientRect(hwnd, &mut rc);
            let w = rc.width().max(100);
            let h = rc.height().max(100);

            let mem_dc = CreateCompatibleDC(hdc_win);
            let mem_bmp = CreateCompatibleBitmap(hdc_win, w, h);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            let bg_br = CreateSolidBrush(rgb(15, 23, 42));
            FillRect(mem_dc, &rc, bg_br);
            DeleteObject(bg_br);

            let border_br = CreateSolidBrush(rgb(245, 158, 11));
            FrameRect(mem_dc, &rc, border_br);
            DeleteObject(border_br);

            let hdr_rc = RECT::new(2, 2, w - 2, 54);
            let hdr_br = CreateSolidBrush(rgb(30, 41, 59));
            FillRect(mem_dc, &hdr_rc, hdr_br);
            DeleteObject(hdr_br);

            SetBkMode(mem_dc, TRANSPARENT);
            let fname = to_wstring("Segoe UI");
            let f_title = CreateFontW(-19, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());
            let f_body = CreateFontW(-14, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());
            let f_btn = CreateFontW(-14, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, fname.as_ptr());

            draw_text_left(
                mem_dc,
                20,
                10,
                w - 40,
                36,
                "⚠️ SmartCam è già in esecuzione",
                rgb(253, 224, 71),
                f_title,
            );

            draw_multiline_text(
                mem_dc,
                RECT::new(22, 68, w - 22, 168),
                "Un'altra istanza di SmartCam è già avviata su questo PC ed è stata portata in primo piano.\r\n\r\nSe l'applicazione precedente dovesse risultare bloccata o non rispondere ai comandi, puoi chiudere forzatamente tutte le istanze di SmartCam premendo il pulsante rosso qui sotto.",
                rgb(226, 232, 240),
                f_body,
            );

            st.btn_use_existing_rect = RECT::new(22, h - 66, w / 2 - 8, h - 18);
            let hov1 = st
                .btn_use_existing_rect
                .contains(st.hover_pos.x, st.hover_pos.y);
            draw_button(
                mem_dc,
                st.btn_use_existing_rect,
                "🔼 OK, Usa Istanza Attiva",
                true,
                rgb(14, 116, 144),
                hov1,
                f_btn,
            );

            st.btn_kill_all_rect = RECT::new(w / 2 + 8, h - 66, w - 22, h - 18);
            let hov2 = st
                .btn_kill_all_rect
                .contains(st.hover_pos.x, st.hover_pos.y);
            draw_button(
                mem_dc,
                st.btn_kill_all_rect,
                "🛑 Chiudi Tutte le Istanze",
                true,
                rgb(185, 28, 28),
                hov2,
                f_btn,
            );

            BitBlt(hdc_win, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);

            DeleteObject(f_title);
            DeleteObject(f_body);
            DeleteObject(f_btn);
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_CLOSE => {
            bring_existing_instance_to_foreground(st.existing_hwnd);
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_single_instance_alert_dialog(repo_root: &Path, existing_hwnd: HWND) {
    let mut alert_state = Box::new(SingleInstanceAlertState {
        existing_hwnd,
        hover_pos: POINT { x: -1, y: -1 },
        btn_use_existing_rect: RECT::new(0, 0, 0, 0),
        btn_kill_all_rect: RECT::new(0, 0, 0, 0),
    });
    GLOBAL_ALERT_STATE = &mut *alert_state as *mut SingleInstanceAlertState;

    let hinst = GetModuleHandleW(std::ptr::null());
    let hicon_big = load_theme_icon(repo_root, "cyber_lens", 32);
    let hicon_sm = load_theme_icon(repo_root, "cyber_lens", 16);
    let alert_class = to_wstring("SmartCamSingleInstanceAlertClass");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as UINT,
        style: 0,
        lpfnWndProc: single_instance_alert_wnd_proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: hicon_big,
        hCursor: LoadCursorW(NULL, IDC_ARROW),
        hbrBackground: CreateSolidBrush(rgb(15, 23, 42)),
        lpszMenuName: std::ptr::null(),
        lpszClassName: alert_class.as_ptr(),
        hIconSm: hicon_sm,
    };
    RegisterClassExW(&wc);

    let win_w = 580;
    let win_h = 265;
    let scr_w = GetSystemMetrics(SM_CXSCREEN).max(800);
    let scr_h = GetSystemMetrics(SM_CYSCREEN).max(600);
    let pos_x = (scr_w - win_w) / 2;
    let pos_y = (scr_h - win_h) / 2;

    let title = to_wstring("SmartCam — Avviso Istanza Già Avviata");
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_APPWINDOW,
        alert_class.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        pos_x,
        pos_y,
        win_w,
        win_h,
        NULL,
        NULL,
        hinst,
        NULL,
    );

    if !hwnd.is_null() {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        UpdateWindow(hwnd);
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, NULL, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    GLOBAL_ALERT_STATE = std::ptr::null_mut();
}

pub fn run_gui_app(repo_root: &Path, config_path: &Path, start_minimized: bool) -> i32 {
    unsafe {
        let mutex_name = to_wstring("Local\\SmartCam_SingleInstance_Mutex_v1");
        let hmutex = CreateMutexW(std::ptr::null_mut(), TRUE, mutex_name.as_ptr());
        let mutex_already_exists = !hmutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS;
        let class_name = to_wstring("SmartCamMainNativeWindowClass");
        let existing_hwnd = FindWindowW(class_name.as_ptr(), std::ptr::null());

        if mutex_already_exists || !existing_hwnd.is_null() {
            if !hmutex.is_null() {
                CloseHandle(hmutex);
            }
            bring_existing_instance_to_foreground(existing_hwnd);
            show_single_instance_alert_dialog(repo_root, existing_hwnd);
            return 0;
        }

        let mut state = Box::new(AppGuiState::new(
            repo_root.to_path_buf(),
            config_path.to_path_buf(),
            start_minimized,
        ));
        GLOBAL_APP = &mut *state as *mut AppGuiState;

        let hinst = GetModuleHandleW(std::ptr::null());
        let hicon_big = load_theme_icon(repo_root, &state.cfg.app_icon_theme, 32);
        let hicon_sm = load_theme_icon(repo_root, &state.cfg.app_icon_theme, 16);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as UINT,
            style: 0,
            lpfnWndProc: main_wnd_proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: hicon_big,
            hCursor: LoadCursorW(NULL, IDC_ARROW),
            hbrBackground: CreateSolidBrush(rgb(11, 15, 25)),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: hicon_sm,
        };
        RegisterClassExW(&wc);

        let title = to_wstring(
            "SmartCam Studio — Webcam Intelligente, Plugin Smartphone & GPU/AVX Benchmark (PEFL v1.0)",
        );
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            90,
            40,
            1180,
            720,
            NULL,
            NULL,
            hinst,
            NULL,
        );

        if hwnd.is_null() {
            GLOBAL_APP = std::ptr::null_mut();
            if !hmutex.is_null() {
                CloseHandle(hmutex);
            }
            return 1;
        }

        if state.minimized_to_tray {
            ShowWindow(hwnd, SW_HIDE);
        } else {
            ShowWindow(hwnd, SW_SHOWNORMAL);
            UpdateWindow(hwnd);
            SetForegroundWindow(hwnd);
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, NULL, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        GLOBAL_APP = std::ptr::null_mut();
        if !hmutex.is_null() {
            CloseHandle(hmutex);
        }
        0
    }
}
