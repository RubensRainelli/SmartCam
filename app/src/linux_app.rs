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

#![cfg(not(target_os = "windows"))]

use std::time::{Duration, Instant};

use smartcam_capture::{
    enumerate_camera_devices, CameraCaptureConfig, CameraSource, VirtualCameraSink,
    VirtualCameraSinkConfig,
};
use smartcam_engine::{CropStrategy, SimdTier, SmartCamEngine};

use crate::config::{resolve_repo_root, SmartCamConfig};

#[cfg(target_os = "linux")]
type PlatformCamera = smartcam_capture::V4l2CameraCapture;
#[cfg(target_os = "linux")]
type PlatformSink = smartcam_capture::V4l2VirtualCameraSink;

#[cfg(target_os = "macos")]
type PlatformCamera = smartcam_capture::CmioCameraCapture;
#[cfg(target_os = "macos")]
type PlatformSink = smartcam_capture::CmioVirtualCameraSink;

pub fn run_linux_main() {
    let repo_root = resolve_repo_root();
    let cfg_path = repo_root.join("config").join("default_config.yaml");
    let cfg = SmartCamConfig::load_from_file(&cfg_path);

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut max_frames: u64 = 0;
    let mut status_only = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                println!(
                    "SmartCam v2.0.0 (100% Pure Rust — PEFL v1.0) — Copyright (c) 2026 Rubens Rainelli"
                );
                println!("Usage: SmartCam [--status] [--frames N] [--headless] [--accept-pefl]");
                return;
            }
            "--status" => status_only = true,
            "--frames" if i + 1 < args.len() => {
                i += 1;
                max_frames = args[i].parse().unwrap_or(10);
            }
            _ => {}
        }
        i += 1;
    }

    let mut engine = SmartCamEngine::new(SimdTier::Auto);
    let simd_name = engine.get_active_simd_name();
    let cameras = enumerate_camera_devices();

    println!("============================================================");
    println!("  SmartCam v2.0.0 (100% Pure Rust Engine & Native Capture)");
    println!("  SIMD Backend : {}", simd_name);
    println!("  Cameras      : {} detected", cameras.len());

    let mut selected_cam = 0u32;
    let mut found_rgb = false;
    for dev in &cameras {
        println!(
            "    [{}] {} ({}) {}",
            dev.index,
            dev.friendly_name,
            dev.symbolic_link,
            if dev.is_infrared { "[IR]" } else { "[RGB]" }
        );
        if !found_rgb && !dev.is_infrared {
            selected_cam = dev.index;
            found_rgb = true;
        }
    }
    println!("============================================================");

    if status_only {
        return;
    }

    let out_w = cfg.framing_output.output_width.max(640);
    let out_h = cfg.framing_output.output_height.max(480);
    let fps = cfg.camera_input.fps.max(15);

    let mut camera = PlatformCamera::new();
    let _ = camera.open(&CameraCaptureConfig {
        device_index: selected_cam,
        device_id: String::new(),
        width: cfg.camera_input.width.max(640),
        height: cfg.camera_input.height.max(480),
        target_fps: fps,
        bit_depth: cfg.camera_input.bit_depth,
    });

    let mut sink = PlatformSink::new();
    let _ = sink.initialize(&VirtualCameraSinkConfig {
        friendly_name: cfg.virtual_camera.friendly_name.clone(),
        shared_memory_name: cfg.virtual_camera.shared_memory_name.clone(),
        width: out_w,
        height: out_h,
        fps,
    });

    let mut raw_buf = vec![0u8; (1920 * 1080 * 4) as usize];
    let mut framed_buf = vec![0u8; (out_w * out_h * 4) as usize];
    let mut bg_buf = vec![0u8; (out_w * out_h * 4) as usize];
    let mut mask_buf = vec![255u8; (out_w * out_h) as usize];
    let mut out_buf = vec![0u8; (out_w * out_h * 4) as usize];

    let interval = Duration::from_millis((1000 / fps.max(1)) as u64);
    let mut frames = 0u64;

    println!(
        "[SmartCam v2.0.0] Active Virtual Camera 'SmartCam' ({}x{} @ {} FPS). Press Ctrl+C to stop.",
        out_w, out_h, fps
    );

    loop {
        let t0 = Instant::now();
        let (rw, rh) = match camera.read_frame(&mut raw_buf) {
            Some((w, h)) if w > 0 && h > 0 => (w, h),
            _ => (out_w, out_h),
        };

        engine.crop_scale_bgra(
            &raw_buf,
            rw,
            rh,
            rw * 4,
            &mut framed_buf,
            out_w,
            out_h,
            out_w * 4,
            4,
            3,
            CropStrategy::CenterCrop,
        );

        engine.generate_blurred_bg(&framed_buf, &mut bg_buf, out_w, out_h, out_w * 4, 14);
        engine.compute_ai_mask(
            &framed_buf,
            out_w,
            out_h,
            out_w * 4,
            &mut mask_buf,
            out_w,
            true,
            cfg.ai_enabled,
        );
        engine.blend_bgra(
            &framed_buf,
            &bg_buf,
            &mask_buf,
            &mut out_buf,
            out_w,
            out_h,
            out_w * 4,
            out_w * 4,
            out_w,
            out_w * 4,
        );

        let _ = sink.push_frame(&out_buf, out_w, out_h, fps);

        frames += 1;
        if max_frames > 0 && frames >= max_frames {
            break;
        }
        let el = t0.elapsed();
        if el < interval {
            std::thread::sleep(interval - el);
        }
    }

    camera.close();
    sink.shutdown();
}
