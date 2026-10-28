# Changelog

All notable changes to **SmartCam** are documented in this file.

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
