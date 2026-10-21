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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_DSHOW_VIRTUAL_CAM_STUB_H_
#define SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_DSHOW_VIRTUAL_CAM_STUB_H_

#include "../../../../common/include/ivirtual_camera.h"

namespace smartcam {
namespace platforms {
namespace windows {

/**
 * @brief [STUB/FUTURO] Legacy DirectShow (`IBaseFilter` / `IAMStreamConfig`) Virtual Camera Sink.
 *
 * Retained as a structural placeholder for legacy Win32 DirectShow-only applications.
 * Windows 11 primary output uses `MfVirtualCameraSink` (`platforms/windows/drivers/media_foundation/`)
 * for native compatibility with Chrome, Edge, Teams, PWA, and WebRTC.
 */
class DirectShowVirtualCameraStub final : public common::IVirtualCameraSink {
public:
    DirectShowVirtualCameraStub() noexcept = default;
    ~DirectShowVirtualCameraStub() override = default;

    bool Initialize(const common::VirtualCameraConfig& /*config*/) override {
        return false; // Deprecated in favor of Media Foundation Virtual Camera on Windows 11
    }

    bool PushFrame(const common::FrameView& /*frame*/) override {
        return false;
    }

    void Shutdown() override {}

    common::VirtualCameraState GetState() const noexcept override {
        return common::VirtualCameraState::UNINITIALIZED;
    }

    const char* GetBackendName() const noexcept override {
        return "Windows DirectShow Legacy Virtual Camera [Stub/Future]";
    }
};

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_DSHOW_VIRTUAL_CAM_STUB_H_
