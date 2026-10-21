# SmartCam macOS Backend (`CoreMediaIO Camera Extension`) — Agent & Developer Guide

[![License: PEFL v1.0](https://raw.githubusercontent.com/RubensRainelli/pefl-license-spec/main/badges/pefl-badge-red.svg)](../../LICENSE)

## Scope & Isolation Rules
This directory (`platforms/macos/`) contains the macOS-specific Virtual Camera Sink and AVFoundation capture implementation.
- **Contract to implement:** [`smartcam::common::IVirtualCameraSink`](../../common/include/ivirtual_camera.h) (see stub in [`cmio_virtual_camera_stub.h`](cmio_virtual_camera_stub.h)).
- **Shared Core Engine:** Link against `smartcam_engine_static` / `libsmartcam_engine.dylib` from [`common/engine/CMakeLists.txt`](../../common/engine/CMakeLists.txt).

## Implementation Blueprint (Phase 3)
1. **Modern Camera Extension (`CMIOExtension` on macOS 12.3+):**
   - Implement `CMIOExtensionProviderSource`, `CMIOExtensionDeviceSource`, and `CMIOExtensionStreamSource` inside a signed System Extension bundle.
   - Expose an `IOSurface` or Mach XPC sink stream so the unprivileged `SmartCam` user-space process can push processed frames without elevated permissions.
2. **PEFL v1.0 Compliance:**
   - Prepend the official PEFL v1.0 header to all `.h`, `.mm`, `.swift`, and `.zsh` files.
   - Enforce Clause 3 startup EULA verification in all CLI/GUI entry points.
