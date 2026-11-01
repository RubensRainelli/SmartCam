# Changelog

All notable changes to **SmartCam** are documented in this file.

## [2.1.0] — 2026-11-01

### ⚡ Feature & Performance Release: Desktop Zero-Copy Pipeline, Mobile Native Lens Detection & Power Optimization

* **Smartphone OLED Studio — Native Lens Detection & Desktop-Style Aspect Ratio / Resolution Selectors (`app/src/plugins.rs`)**:
  * **Automatic Hardware Lens Probing**: Queries `MediaStreamTrack.getCapabilities()` and `getSettings()` on the smartphone browser to detect each physical camera lens's native resolution (`lensMaxW × lensMaxH`), native aspect ratio, and maximum hardware frame rate.
  * **Coordinated Aspect Ratio & Bandwidth-Scaled Resolution Presets**: Replaced the legacy single percentage scale dropdown with two coordinated selectors matching the Desktop App:
    * **`📺 Formato (Aspect Ratio)`**: `📱 Nativo Obiettivo` (dynamically displaying the lens's real sensor ratio), `16:9 (Widescreen)`, `4:3 (Standard)`, `1:1 (Quadrato)`, and `9:16 (Verticale)` with automatic distortion-free center-cropping (`computeCenterCropRect`).
    * **`📐 Risoluzione & Banda`**: `📱 Nativa Obiettivo`, `1080p Full HD`, `900p HD+`, `720p HD — Consigliato`, `540p qHD — Risparmio Banda`, `480p SD — Bassa Banda`, `360p Low — Consumo Minimo`, and `240p Ultra-Low — Rete Lenta`, dynamically computing exact `(W×H)` dimensions for the chosen aspect ratio and applying hardware ISP capture constraints (`track.applyConstraints`) to save mobile battery and Wi-Fi bandwidth.
  * **Removed Redundant Frame Counter**: Removed the frame counter (`sentFrames` / `frame ricevuti`) from the mobile web UI, test signal canvas, HTTP `/frame` JSON responses, and desktop plugin telemetry bar.
  * **Mobile GPU & Battery Optimization**: Enabled hardware-accelerated GPU canvas rendering (`{ alpha: false, desynchronized: true }`), native camera sensor synchronization via `video.requestVideoFrameCallback`, adaptive JPEG quality (`0.80` / `0.75` / `0.70`), and zero-allocation BGRA buffer swapping (`decode_image_bytes_gdiplus_into` + `std::mem::swap`) in the Rust HTTP receiver.
* **Desktop Video Engine & Win32 GUI Performance / Power Optimization (`smartcam-engine`, `smartcam-capture`, `smartcam-mf-source`, `app`)**:
  * **Eliminated ~18.6–24.8 MB/frame of Redundant Allocations & Copies (>550 MB/s saved at 30 FPS)**: Replaced per-frame heap allocations in `generate_blurred_bg_scalar` with reusable `thread_local!` scratch buffers, implemented zero-copy background blending in `"image"` mode, added solid-color background caching, and added direct `read_frame_into_raw` Media Foundation capture.
  * **Row-Major Cache-Friendly Kernels & Hoisted Math**: Converted vertical passes in Bokeh box blur (`compositor.rs`) and Green Screen morphological/blur filters (`green_screen.rs`) from column-major to row-major for L1 cache locality and SIMD auto-vectorization; hoisted invariant reciprocals out of `ai_segmenter.rs` and `crop_scaler.rs` inner loops.
  * **Win32 GDI Partial Repaint & Minimized Sleep**: Cached the Smartphone QR Code matrix (`cached_qr_matrix`), suspended GDI repaints when minimized (`IsIconic`), and restricted 30 FPS timer invalidation to the video monitor rectangle (`monitor_rc`).
  * **Media Foundation Virtual Camera NV12 Cache & Seqlock Hardening (`smartcam-mf-source`)**: Added NV12 frame caching (`nv12_cache_buf`), 64-attempt hybrid seqlock backoff, and last-good-frame hold to eliminate any standby flash or banding under heavy load.

---

## [2.0.0] — 2026-10-28

### 🚀 Major Architectural Milestone: 100% Pure Rust & Exclusive Media Foundation

* **100% Pure Rust Codebase**: Rewrote the entire video engine, SIMD kernels, GPU runtime probes, native capture backends, and Windows Media Foundation COM Virtual Camera driver in pure Rust (`app`, `crates/smartcam-engine`, `crates/smartcam-capture`, `crates/smartcam-mf-source`). Permanently eliminated all C++17 source/header files, CMake/Ninja build scripts, and FFI bridge DLLs.
* **Single-Binary Runtime SIMD Dispatch**: Unified `AVX-512` (`avx512f,avx512bw`), `AVX2`, `ARM64 NEON`, and `Scalar` kernels into a single binary with automatic runtime CPU feature detection (`is_x86_feature_detected!`), eliminating separate `smartcam_engine_avx2.dll` and `smartcam_engine_avx512.dll` files.
* **Pure Media Foundation Virtual Camera (`smartcam_mf_source.dll`)**: Implemented `IClassFactory`, `IMFMediaSourceEx`, `IMFMediaSource`, `IMFMediaStream`, `IMFMediaEventGenerator`, and `IMFGetService` in idiomatic Rust using the official `windows` crate (`#[implement]`).
* **Complete Deprecation of Legacy DirectShow**: Removed all legacy DirectShow virtual camera filter code and registry categories. Windows 10 and Windows 11 now use Media Foundation exclusively. Users requiring legacy DirectShow support on older systems can continue using **SmartCam v1.x**.
* **In-Process Native Rust HTTPS/WebSocket Smartphone Server**: Replaced the external `powershell.exe` / `.NET SslStream` subprocess with an in-process pure-Rust TLS server (`rustls` + `rcgen`), reducing RAM usage to `< 1 MB` and eliminating all external script calls.
* **Digitally Signed Windows Installer & Multi-Platform Packages**:
  * RSA-4096 Authenticode digital signatures applied to `SmartCam.exe`, `smartcam_mf_source.dll`, and `SmartCam-Setup-v2.0.0-x64.exe`.
  * Official Windows Inno Setup installer (`SmartCam-Setup-v2.0.0-x64.exe`) with automatic Media Foundation COM registration and WinGet support (`winget install RubensRainelli.SmartCam`).
  * Official macOS Universal `.dmg`, Linux `.deb` and `.tar.gz` packages, plus `SHA256SUMS.txt`.
* **User-Friendly Bilingual Documentation & Technical Guide**:
  * Redesigned [`README.md`](README.md) (English) and [`README.it.md`](README.it.md) (Italian) for everyday users.
  * Added comprehensive English engineering documentation in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

---

## [1.0.0] — 2026-05-26

* Initial public release of SmartCam featuring hybrid Rust GUI + C++17 SIMD engine, Media Foundation & DirectShow virtual camera support, hardware benchmark suite, and PEFL v1.0 licensing.
