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

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod config;
#[cfg(not(target_os = "windows"))]
mod linux_app;

#[cfg(target_os = "windows")]
mod benchmark;
#[cfg(target_os = "windows")]
mod diagnostics;
#[cfg(target_os = "windows")]
mod engine_bridge;
#[cfg(target_os = "windows")]
mod gui;
#[cfg(target_os = "windows")]
mod pefl_guard;
#[cfg(target_os = "windows")]
mod plugins;
#[cfg(target_os = "windows")]
mod win32_ffi;

#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use benchmark::run_hardware_benchmark_suite;
#[cfg(target_os = "windows")]
use config::{resolve_repo_root, SmartCamConfig};
#[cfg(target_os = "windows")]
use diagnostics::*;
#[cfg(target_os = "windows")]
use engine_bridge::*;
#[cfg(target_os = "windows")]
use gui::run_gui_app;
#[cfg(target_os = "windows")]
use pefl_guard::check_pefl_cli;
#[cfg(target_os = "windows")]
use plugins::{normalize_plugin_id, PluginRegistry};
#[cfg(target_os = "windows")]
use win32_ffi::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS};

#[cfg(not(target_os = "windows"))]
fn main() {
    linux_app::run_linux_main();
}

#[cfg(target_os = "windows")]
fn print_help() {
    println!("SmartCam.exe — Applicazione Desktop Nativa & Virtual Camera \"SmartCam\" Windows 11 (PEFL v1.0)");
    println!("Copyright (c) 2026 Rubens Rainelli — https://github.com/RubensRainelli/\n");
    println!("Utilizzo:");
    println!("  SmartCam.exe                      Avvia l'interfaccia grafica con anteprima live (senza terminale)");
    println!(
        "  SmartCam.exe --minimized          Avvia ridotto nella System Tray (Modalità On-Demand)"
    );
    println!("  SmartCam.exe --status [--json]    Mostra stato driver \"SmartCam\", webcam (con filtro IR) e SIMD");
    println!("  SmartCam.exe --benchmark [--json] Esegue il benchmark hardware (CPU AVX-512/AVX2/Scalar, Vulkan, OpenCL, CUDA, ROCm)");
    println!("  SmartCam.exe --register-driver    Registra il driver Virtual Camera \"SmartCam\" COM (UAC)");
    println!("  SmartCam.exe --unregister-driver  Rimuove il driver Virtual Camera \"SmartCam\" COM (UAC)");
    println!("  SmartCam.exe --headless           Esegue la pipeline Virtual Camera \"SmartCam\" da riga di comando");
    println!(
        "  SmartCam.exe --frames <N>         Elabora N fotogrammi in modalità headless ed esce"
    );
    println!(
        "  SmartCam.exe --open-web-testbench Apre il test Side-by-Side (2 Webcam) nel browser"
    );
    println!("  SmartCam.exe --set-aspect <4:3|16:9|1:1|9:16>");
    println!("  SmartCam.exe --set-resolution <1080p|900p|720p|540p|480p|360p|240p>");
    println!("  SmartCam.exe --set-fps <60|30|25|24|20|15|10>");
    println!("  SmartCam.exe --set-engine-mode <0=Auto|1=Sagoma|2=DifferenzaSfondo>");
    println!("  SmartCam.exe --set-silhouette <0=Volto+Busto|1=SoloVolto>");
    println!("  SmartCam.exe --set-bg-tolerance <0.05..1.00>");
    println!("  SmartCam.exe --set-mode <green_screen|ai_selfie|passthrough>");
    println!("  SmartCam.exe --set-bg <solid_color|blur|image>");
    println!("  SmartCam.exe --mirror-bg <on|off>");
    println!("  SmartCam.exe --bit-depth <64|32>");
    println!("  SmartCam.exe --set-backend <auto|cpu|vulkan|opencl|cuda|rocm>");
    println!("  SmartCam.exe --set-simd <auto|avx512|avx2|scalar>");
    println!("  SmartCam.exe --set-ai <on|off>");
    println!("  SmartCam.exe --set-phone-port <1024..65535>");
    println!("  SmartCam.exe --enable-plugin <phone_camera|webrtc_rtsp|studio_color|teleprompter|multi_person_tracker|id>");
    println!("  SmartCam.exe --disable-plugin <phone_camera|webrtc_rtsp|studio_color|teleprompter|multi_person_tracker|id>");
    println!("  SmartCam.exe --set-icon <cyber_lens|emerald_cam|neon_iris>");
}

#[cfg(target_os = "windows")]
fn main() {
    let repo_root = resolve_repo_root();
    let mut config_path: PathBuf = repo_root.join("config").join("default_config.yaml");

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        unsafe {
            // STD_OUTPUT_HANDLE = -11i32 as u32 (0xFFFFFFF5).
            // Only attach to parent console if stdout is not already redirected to a pipe/file.
            let h_out = GetStdHandle(0xFFFF_FFF5u32);
            if h_out.is_null() {
                let _ = AttachConsole(ATTACH_PARENT_PROCESS);
            }
        }
    }

    let mut flag_status = false;
    let mut flag_benchmark = false;
    let mut flag_json = false;
    let mut flag_register = false;
    let mut flag_unregister = false;
    let mut flag_headless = false;
    let mut flag_minimized = false;
    let mut flag_open_web = false;
    let mut flag_accept_pefl = false;
    let mut max_frames: u64 = 0;
    let mut build_variant = String::from("out_avx2");

    let mut set_aspect: Option<String> = None;
    let mut set_resolution: Option<String> = None;
    let mut set_fps: Option<u32> = None;
    let mut set_engine_mode: Option<u8> = None;
    let mut set_silhouette: Option<i32> = None;
    let mut set_bg_tol: Option<f32> = None;
    let mut set_mode: Option<String> = None;
    let mut set_bg: Option<String> = None;
    let mut set_mirror_bg: Option<bool> = None;
    let mut set_bit_depth: Option<u8> = None;
    let mut set_backend: Option<String> = None;
    let mut set_simd: Option<String> = None;
    let mut set_ai: Option<bool> = None;
    let mut set_phone_port: Option<u16> = None;
    let mut enable_plugins: Vec<String> = Vec::new();
    let mut disable_plugins: Vec<String> = Vec::new();
    let mut set_icon: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "--status" => flag_status = true,
            "--benchmark" => flag_benchmark = true,
            "--json" => flag_json = true,
            "--register-driver" => flag_register = true,
            "--unregister-driver" => flag_unregister = true,
            "--headless" => flag_headless = true,
            "--minimized" => flag_minimized = true,
            "--open-web-testbench" => flag_open_web = true,
            "--accept-pefl" => flag_accept_pefl = true,
            "--config" if i + 1 < args.len() => {
                i += 1;
                config_path = PathBuf::from(&args[i]);
            }
            "--frames" if i + 1 < args.len() => {
                i += 1;
                max_frames = args[i].parse().unwrap_or(0);
                flag_headless = true;
            }
            "--build-variant" if i + 1 < args.len() => {
                i += 1;
                build_variant = args[i].clone();
            }
            "--set-aspect" | "--aspect-ratio" if i + 1 < args.len() => {
                i += 1;
                set_aspect = Some(args[i].clone());
            }
            "--set-resolution" | "--resolution" if i + 1 < args.len() => {
                i += 1;
                set_resolution = Some(args[i].clone());
            }
            "--set-fps" | "--fps" if i + 1 < args.len() => {
                i += 1;
                set_fps = args[i].parse().ok();
            }
            "--set-engine-mode" | "--engine-mode" if i + 1 < args.len() => {
                i += 1;
                set_engine_mode = args[i].parse().ok();
            }
            "--set-silhouette" | "--silhouette" if i + 1 < args.len() => {
                i += 1;
                let v = args[i].trim().to_ascii_lowercase();
                set_silhouette = Some(if v == "1" || v.contains("solo") || v.contains("face") {
                    1
                } else {
                    0
                });
            }
            "--set-phone-port" | "--phone-port" if i + 1 < args.len() => {
                i += 1;
                set_phone_port = args[i].parse::<u16>().ok().map(|p| p.clamp(1024, 65535));
            }
            "--set-bg-tolerance" | "--bg-tolerance" if i + 1 < args.len() => {
                i += 1;
                set_bg_tol = args[i].parse().ok();
            }
            "--set-mode" | "--mode" if i + 1 < args.len() => {
                i += 1;
                set_mode = Some(args[i].clone());
            }
            "--set-bg" if i + 1 < args.len() => {
                i += 1;
                set_bg = Some(args[i].clone());
            }
            "--mirror-bg" | "--set-mirror-bg" if i + 1 < args.len() => {
                i += 1;
                let v = args[i].trim().to_ascii_lowercase();
                set_mirror_bg = Some(matches!(v.as_str(), "on" | "true" | "1" | "yes" | "si"));
            }
            "--bit-depth" | "--set-bit-depth" if i + 1 < args.len() => {
                i += 1;
                if let Ok(bd) = args[i].trim().parse::<u8>() {
                    set_bit_depth = Some(if bd >= 64 { 64 } else { 32 });
                }
            }
            "--set-backend" | "--backend" if i + 1 < args.len() => {
                i += 1;
                set_backend = Some(args[i].to_ascii_lowercase());
            }
            "--set-simd" | "--simd" if i + 1 < args.len() => {
                i += 1;
                set_simd = Some(args[i].to_ascii_lowercase());
            }
            "--set-ai" | "--ai" if i + 1 < args.len() => {
                i += 1;
                let v = args[i].trim().to_ascii_lowercase();
                set_ai = Some(matches!(v.as_str(), "on" | "true" | "1" | "yes" | "attiva"));
            }
            "--enable-plugin" if i + 1 < args.len() => {
                i += 1;
                enable_plugins.push(args[i].clone());
            }
            "--disable-plugin" if i + 1 < args.len() => {
                i += 1;
                disable_plugins.push(args[i].clone());
            }
            "--set-icon" | "--icon" if i + 1 < args.len() => {
                i += 1;
                set_icon = Some(args[i].clone());
            }
            _ => {}
        }
        i += 1;
    }

    let is_cli_invocation = flag_status
        || flag_benchmark
        || flag_register
        || flag_unregister
        || flag_headless
        || flag_open_web
        || set_aspect.is_some()
        || set_resolution.is_some()
        || set_fps.is_some()
        || set_engine_mode.is_some()
        || set_silhouette.is_some()
        || set_phone_port.is_some()
        || set_bg_tol.is_some()
        || set_mode.is_some()
        || set_bg.is_some()
        || set_mirror_bg.is_some()
        || set_bit_depth.is_some()
        || set_backend.is_some()
        || set_simd.is_some()
        || set_ai.is_some()
        || !enable_plugins.is_empty()
        || !disable_plugins.is_empty()
        || set_icon.is_some();

    // 1. Enforce Clause 3 of PEFL v1.0 before any CLI operation
    // Note: --accept-pefl only bypasses interactive CLI prompts in-memory for build scripts
    // and NEVER writes .pefl_accepted_SmartCam.exe to disk, ensuring the GUI executable
    // always prompts the user explicitly as required by PEFL v1.0 Clause 3.
    if is_cli_invocation && !flag_accept_pefl {
        if !check_pefl_cli(&repo_root) {
            std::process::exit(1);
        }
    }

    let mut cfg = SmartCamConfig::load_from_file(&config_path);
    let mut cfg_modified = false;
    if let Some(ar) = set_aspect {
        cfg.framing_output.aspect_ratio = ar;
        cfg.framing_output.sync_dimensions_from_preset();
        cfg_modified = true;
    }
    if let Some(res) = set_resolution {
        cfg.framing_output.resolution_preset = res;
        cfg.framing_output.sync_dimensions_from_preset();
        cfg_modified = true;
    }
    if let Some(fps) = set_fps {
        cfg.camera_input.fps = fps.clamp(10, 60);
        cfg_modified = true;
    }
    if let Some(em) = set_engine_mode {
        cfg.segmentation.ai_selfie.engine_mode = (em.min(2)) as i32;
        cfg_modified = true;
    }
    if let Some(sil) = set_silhouette {
        cfg.segmentation.ai_selfie.silhouette_mode = sil.clamp(0, 1);
        cfg_modified = true;
    }
    if let Some(port) = set_phone_port {
        cfg.phone_camera_port = port.clamp(1024, 65535);
        let mut reg = PluginRegistry::new(&repo_root);
        let actual_port = reg.set_phone_server_port(cfg.phone_camera_port);
        cfg.phone_camera_port = actual_port;
        let _ = reg.save_state(&repo_root);
        cfg_modified = true;
    }
    if let Some(tol) = set_bg_tol {
        cfg.segmentation.ai_selfie.bg_diff_tolerance = tol.clamp(0.05, 1.0);
        cfg_modified = true;
    }
    if let Some(m) = set_mode {
        cfg.segmentation.mode = m;
        cfg_modified = true;
    }
    if let Some(bg) = set_bg {
        cfg.compositor.background_mode = bg;
        cfg_modified = true;
    }
    if let Some(mir) = set_mirror_bg {
        cfg.compositor.background_mirror_h = mir;
        cfg_modified = true;
    }
    if let Some(bd) = set_bit_depth {
        cfg.camera_input.bit_depth = bd;
        cfg_modified = true;
    }
    if let Some(b) = set_backend {
        cfg.compute_backend = b.clone();
        cfg.compositor.compute_backend = b;
        cfg_modified = true;
    }
    if let Some(s) = set_simd {
        cfg.compositor.simd_acceleration = s;
        cfg_modified = true;
    }
    if let Some(ai) = set_ai {
        cfg.ai_enabled = ai;
        cfg.segmentation.ai_selfie.ai_enabled = ai;
        cfg_modified = true;
    }
    if !enable_plugins.is_empty() || !disable_plugins.is_empty() {
        let mut reg = PluginRegistry::new(&repo_root);
        for pid in &enable_plugins {
            let cid = normalize_plugin_id(pid);
            reg.set_plugin_enabled(&cid, true);
            cfg.set_plugin_enabled(&cid, true);
        }
        for pid in &disable_plugins {
            let cid = normalize_plugin_id(pid);
            reg.set_plugin_enabled(&cid, false);
            cfg.set_plugin_enabled(&cid, false);
        }
        cfg.enabled_plugins = reg.enabled_plugin_ids();
        let _ = reg.save_state(&repo_root);
        cfg_modified = true;
    }
    if let Some(ic) = set_icon {
        cfg.app_icon_theme = ic;
        cfg_modified = true;
    }
    if cfg_modified {
        if let Err(e) = cfg.save_to_file(&config_path) {
            eprintln!("[SmartCam] Errore salvataggio configurazione: {}", e);
            std::process::exit(1);
        }
        println!(
            "[SmartCam] Configurazione aggiornata in {}",
            config_path.display()
        );
    }

    if flag_benchmark {
        let report = run_hardware_benchmark_suite(&repo_root, None);
        if flag_json {
            println!("{{");
            println!(
                "  \"hardware_fingerprint\": \"{}\",",
                report.hardware_fingerprint
            );
            println!("  \"cpu_name\": \"{}\",", report.cpu_name);
            println!("  \"logical_cores\": {},", report.logical_cores);
            println!("  \"avx2_supported\": {},", report.avx2_supported);
            println!("  \"avx512_supported\": {},", report.avx512_supported);
            println!(
                "  \"recommended_profile_id\": \"{}\",",
                report.recommended_profile_id
            );
            println!(
                "  \"recommended_profile_label\": \"{}\",",
                report.recommended_profile_label
            );
            println!(
                "  \"recommended_backend\": \"{}\",",
                report.recommended_backend
            );
            println!("  \"recommended_simd\": \"{}\",", report.recommended_simd);
            println!(
                "  \"recommended_ai_enabled\": {},",
                report.recommended_ai_enabled
            );
            println!("  \"recommended_score\": {},", report.recommended_score);
            println!("  \"recommended_fps\": {:.1},", report.recommended_fps);
            println!(
                "  \"recommended_cpu_load_pct\": {:.1}",
                report.recommended_cpu_load_pct
            );
            println!("}}");
        } else {
            println!("=========================================================================");
            println!("  SmartCam — Benchmark Hardware & Classifica Prestazioni (1440x1080)");
            println!("=========================================================================");
            println!(
                "  CPU Rilevata : {} ({} core logici)",
                report.cpu_name, report.logical_cores
            );
            println!("  Fingerprint  : {}", report.hardware_fingerprint);
            println!(
                "  Supporto AVX : AVX2={} | AVX-512={}",
                report.avx2_supported, report.avx512_supported
            );
            println!("-------------------------------------------------------------------------");
            for p in &report.profiles {
                let rec = if p.recommended {
                    " ⭐ CONSIGLIATO"
                } else {
                    ""
                };
                if p.available {
                    println!(
                        "  [✅] {:<28} | {:>5} Pt | {:>5.1} FPS | ~{:>4.1}% CPU{}",
                        p.label, p.score, p.estimated_fps, p.estimated_cpu_load_pct, rec
                    );
                } else {
                    println!("  [⚪] {:<28} | {}", p.label, p.availability_label);
                }
            }
            println!("-------------------------------------------------------------------------");
            println!("  ⭐ Scelta Consigliata: {}", report.recommendation_reason);
            println!("=========================================================================");
        }
        return;
    }

    if flag_register {
        let (ok, msg) = invoke_driver_install(&repo_root, &build_variant);
        println!("{}", msg);
        std::process::exit(if ok { 0 } else { 1 });
    }

    if flag_unregister {
        let (ok, msg) = invoke_driver_uninstall(&repo_root);
        println!("{}", msg);
        std::process::exit(if ok { 0 } else { 1 });
    }

    if flag_open_web {
        let _ = ensure_virtual_camera_registered(&repo_root, &build_variant);
        let url = open_web_testbench(&repo_root);
        println!("[SmartCam] Aperto Test Side-by-Side: {}", url);
        return;
    }

    if flag_status {
        let diag = collect_diagnostics(&repo_root, true);
        print_cli_status(&diag, &cfg, flag_json);
        return;
    }

    let _ = ensure_virtual_camera_registered(&repo_root, &build_variant);

    if flag_headless {
        let code = run_headless_pipeline(&repo_root, &mut cfg, &build_variant, max_frames);
        std::process::exit(code);
    }

    if cfg_modified && !flag_minimized {
        return;
    }

    let code = run_gui_app(&repo_root, &config_path, flag_minimized);
    std::process::exit(code);
}

#[cfg(target_os = "windows")]
fn run_headless_pipeline(
    repo_root: &std::path::Path,
    cfg: &mut SmartCamConfig,
    build_variant: &str,
    max_frames: u64,
) -> i32 {
    let diag = collect_diagnostics(repo_root, true);
    let best_idx =
        select_best_default_camera_index(&diag.physical_cameras, cfg.camera_input.device_index);
    cfg.camera_input.device_index = best_idx;

    let mut plugin_registry = PluginRegistry::new(repo_root);
    if !cfg.enabled_plugins.is_empty() {
        plugin_registry.set_enabled_plugin_ids(&cfg.enabled_plugins);
    }
    if cfg.phone_camera_port >= 1024 {
        let _ = plugin_registry.set_phone_server_port(cfg.phone_camera_port);
    }
    if !cfg.phone_camera_url.trim().is_empty() {
        let _ = plugin_registry.set_phone_camera_endpoint(&cfg.phone_camera_url);
    }

    let mut pipeline = SmartCamPipelineInstance::new(repo_root, cfg, build_variant);
    pipeline.sync_face_and_silhouette_settings(
        cfg,
        plugin_registry.is_plugin_enabled(crate::plugins::PLUGIN_ID_MULTI_PERSON),
    );
    let cam_opened = pipeline.open_hardware_camera(
        cfg.camera_input.device_index,
        cfg.camera_input.width,
        cfg.camera_input.height,
        cfg.camera_input.fps,
    );

    println!("[SmartCam] Pipeline Headless Inizializzata:");
    println!(
        "  * Fotocamera Input : Dispositivo #{} ({}x{} @ {} FPS, Hardware={})",
        cfg.camera_input.device_index,
        cfg.camera_input.width,
        cfg.camera_input.height,
        cfg.camera_input.fps,
        cam_opened
    );
    println!(
        "  * Formato Output   : {} ({}x{})",
        cfg.framing_output.aspect_ratio,
        cfg.framing_output.output_width,
        cfg.framing_output.output_height
    );
    println!(
        "  * Motore Compute   : {} (AI={})",
        pipeline.simd_backend_label,
        if pipeline.active_ai_enabled {
            "ON"
        } else {
            "OFF"
        }
    );
    println!(
        "  * Plugin Attivi    : {}",
        plugin_registry.summary_line_it()
    );

    let mut raw_buf = Vec::new();
    let mut frame_count = 0u64;
    let t_start = Instant::now();
    let frame_interval = Duration::from_secs_f64(1.0 / (cfg.camera_input.fps.max(1) as f64));

    loop {
        let loop_t0 = Instant::now();
        let cam_w = cfg.camera_input.width;
        let cam_h = cfg.camera_input.height;
        let needed_bytes = (cam_w * cam_h * 4) as usize;
        if raw_buf.len() < needed_bytes {
            raw_buf.resize(needed_bytes, 255);
        }

        let (rw, rh) = if let Some((w, h)) =
            plugin_registry.read_active_camera_plugin_frame(&mut raw_buf, cam_w, cam_h)
        {
            (w, h)
        } else if let Some((w, h)) = pipeline.read_camera_frame(&mut raw_buf, cam_w, cam_h) {
            (w, h)
        } else {
            generate_synthetic_bgra_frame(&mut raw_buf, cam_w, cam_h, frame_count);
            (cam_w, cam_h)
        };

        let (ow, oh) = pipeline.process_frame(&raw_buf, rw, rh, cfg);
        plugin_registry.apply_active_video_filter_plugins(
            &mut pipeline.out_buf,
            Some(&mut pipeline.mask_buf),
            ow,
            oh,
            false,
        );
        let out_ptr = pipeline.out_buf.as_ptr();
        let out_len = pipeline.out_buf.len();
        let out_slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        pipeline.push_frame_to_sink(out_slice, ow, oh, cfg.camera_input.fps);

        frame_count += 1;
        if max_frames > 0 && frame_count >= max_frames {
            break;
        }

        let elapsed = loop_t0.elapsed();
        if max_frames == 0 && elapsed < frame_interval {
            thread::sleep(frame_interval - elapsed);
        }
    }

    let total_sec = t_start.elapsed().as_secs_f64().max(1e-6);
    let avg_fps = (frame_count as f64) / total_sec;
    println!(
        "[SmartCam] Elaborati {} fotogrammi in {:.2}s (Throughput Medio: {:.1} FPS).",
        frame_count, total_sec, avg_fps
    );
    0
}
