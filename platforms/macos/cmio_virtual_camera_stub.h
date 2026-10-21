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

#ifndef SMARTCAM_PLATFORMS_MACOS_CMIO_VIRTUAL_CAMERA_STUB_H_
#define SMARTCAM_PLATFORMS_MACOS_CMIO_VIRTUAL_CAMERA_STUB_H_

#include "../../common/include/ivirtual_camera.h"

namespace smartcam {
namespace platforms {
namespace macos {

/**
 * @brief [STUB/FUTURO] macOS CoreMediaIO Camera Extension (`CMIOExtensionProvider`) Virtual Camera Sink.
 *
 * Target workflow for Phase 3 (macOS Agent):
 *   1. Implement a System Extension using `CoreMediaIO.framework` (`CMIOExtensionProviderSource`,
 *      `CMIOExtensionDeviceSource`, `CMIOExtensionStreamSource`).
 *   2. Bridge frames from user-space SmartCam engine via Mach XPC / `IOSurface` shared memory.
 */
class CmioVirtualCameraStub final : public common::IVirtualCameraSink {
public:
    CmioVirtualCameraStub() noexcept = default;
    ~CmioVirtualCameraStub() override = default;

    bool Initialize(const common::VirtualCameraConfig& /*config*/) override { return false; }
    bool PushFrame(const common::FrameView& /*frame*/) override { return false; }
    void Shutdown() override {}

    common::VirtualCameraState GetState() const noexcept override {
        return common::VirtualCameraState::UNINITIALIZED;
    }

    const char* GetBackendName() const noexcept override {
        return "macOS CoreMediaIO Camera Extension Sink [Stub/Phase 3]";
    }
};

}  // namespace macos
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_MACOS_CMIO_VIRTUAL_CAMERA_STUB_H_
