# Changelog

All notable changes to **SmartCam** are documented in this file.

## [2.1.1] — 2026-11-02

### 🛡️ Security, Memory-Safety, Media Foundation COM & Linux V4L2 Hardening Release

Special thanks to **[@maksimtech](https://github.com/maksimtech)** (**Debugger & Security / Quality Contributor**) for contributing PRs #16–#24 and reporting the multi-user IPC privacy and cross-platform findings addressed in this release:

* **Multi-User IPC Privacy & Security Hardening (`crates/smartcam-capture/src/ipc.rs`, `app/src/pefl_guard.rs`, `app/src/diagnostics.rs`)**:
  * **Explicit Protected SDDL DACL**: Replaced the permissive null DACL on the cross-session IPC backing file, named file mapping, and frame event with a Protected DACL (`D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;LS)(A;;GA;;;<UserSid>)`) that restricts access exclusively to `SYSTEM`, `Administrators`, `LOCAL SERVICE` (Windows Camera `FrameServer` in Session 0), and the active interactive user's SID, blocking other local accounts on shared/school PCs from reading or injecting frames.
  * **Payload Zeroing & Automatic Unlink on Close**: When the producer closes or drops, the entire shared memory header and 33 MB pixel payload are zeroed out in memory before unmapping, and the backing file (now stored under `%PROGRAMDATA%\SmartCam`) is immediately unlinked from disk.
  * **Per-User PEFL Persistence & Clean Test Suite Isolation**: Removed `C:\Users\Public\SmartCam` from PEFL acceptance caching so license consent is strictly per-user, and isolated all `cargo test --workspace` runs so zero files or registry keys are left behind on developer/contributor machines (`PR #24`).
* **Engine SIMD, Segmenter & IPC Buffer Bounds Validation (`crates/smartcam-engine`, `crates/smartcam-capture`)**:
  * **`blend_bgra` SIMD Pre-Validation (`PR #16`)**: Validates buffer lengths and row strides across all planes before dispatching to `AVX-512`, `AVX2`, `NEON`, or `Scalar` kernels.
  * **`crop_scale_bgra` & `resize_bilinear_roi` Bounds & Half-Pixel Center Alignment (`PR #17`)**: Rejects undersized buffers or narrow strides with `false` instead of panicking, and aligns 16.16 fixed-point bilinear resampling to pixel centers (`(d + 0.5) * src / dst - 0.5`).
  * **`compute_mask` & `calibrate_face_landmarks` Input Checks (`PR #18`)**: Rejects strides narrower than a full row in both `GreenScreenSegmenter` and `AiOnnxSegmenter`, and safely handles snapshots smaller than the 5×5 landmark sampling window.
  * **`SharedFrameIpc::write_frame` Source Stride Validation (`PR #19`)**: Rejects source strides that are narrower than a row or would read past `src_data`.
* **Windows Media Foundation Virtual Camera COM & NV12 Colorimetry Fixes (`crates/smartcam-mf-source`)**:
  * **COM Object Lifetime Tracking in `DllCanUnloadNow` (`PR #20`)**: Tracks active `SmartCamClassFactory`, `SmartCamMfMediaSource`, and `SmartCamMfMediaStream` instances via an atomic `ModuleRef` guard so `CoFreeUnusedLibraries` never unloads `smartcam_mf_source.dll` while COM objects are alive.
  * **Accurate BT.601 Studio-Range NV12 Media Type Attributes (`PR #21`)**: Advertises `MFVideoTransferMatrix_BT601` and `MFNominalRange_16_235` on NV12 media types to match the exact encoding produced by `bgra_to_nv12_pitched`.
* **Desktop App Config & Network Stream Hardening (`app/src/config.rs`, `app/src/plugins.rs`, `app/src/main.rs`)**:
  * **Overflow-Safe MJPEG `Content-Length` Parsing (`PR #22`)**: Uses `checked_add` when parsing multipart `Content-Length` headers from HTTP/MJPEG camera streams.
  * **Finite Float Filtering & Backslash Preservation in YAML Config (`PR #23`)**: Rejects `NaN`/`inf` float values on config load and CLI arguments (`--set-bg-tolerance`), and escapes backslashes across all quoted string fields on save so Windows device IDs and UNC paths survive repeated save/load cycles.
* **Linux Native V4L2 Capture & Pipeline Completion (`crates/smartcam-capture/src/linux_backend/mod.rs`, `app/src/linux_app.rs`)**:
  * Implemented full Linux V4L2 format negotiation (`VIDIOC_S_FMT` / `VIDIOC_G_FMT`), zero-copy kernel `V4L2_MEMORY_MMAP` ring-buffer streaming (`VIDIOC_REQBUFS`, `VIDIOC_QUERYBUF`, `mmap`, `VIDIOC_QBUF`, `VIDIOC_STREAMON`, `VIDIOC_DQBUF`) with `read()` fallback, and `YUYV` / `RGB24` / `BGR24` / `BGR32` to `BGRA32` decoding.
  * Added `VIDIOC_S_FMT` output format configuration on `v4l2loopback` virtual camera sinks, dynamic `raw_buf` resizing for `>1080p` cameras, and PEFL v1.0 Clause 3 startup verification in `linux_app.rs`.

---

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
