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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_VIRTUAL_CAMERA_SINK_H_
#define SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_VIRTUAL_CAMERA_SINK_H_

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <wrl/client.h>

#include "../../../../common/include/ivirtual_camera.h"
#include "../../ipc/shared_frame_ipc.h"
#include "smartcam_mf_guids.h"

namespace smartcam {
namespace platforms {
namespace windows {

/**
 * @brief Windows 11 Native Media Foundation Virtual Camera Sink (`IVirtualCameraSink`).
 *
 * Operates at standard user privileges:
 *   1. Opens the Named Shared Memory IPC channel (`Local\SmartCam_MF_SharedFrame_v1`).
 *   2. Invokes Windows 11 `MFCreateVirtualCamera` (with `MFVirtualCameraType_SoftwareCameraSource`
 *      and `MFVirtualCameraLifetime_Session`) referencing `CLSID_SmartCamMfVirtualCameraSource`.
 *   3. Starts the virtual camera so Chrome, Edge, Teams, Zoom, and WebRTC immediately enumerate
 *      `"SmartCam Virtual Camera"` as a native OS video capture device.
 *   4. Pushes processed `BGRA32` frames into the lock-free seqlock Shared Memory buffer.
 */
class MfVirtualCameraSink final : public common::IVirtualCameraSink {
public:
    MfVirtualCameraSink() noexcept = default;
    ~MfVirtualCameraSink() override { Shutdown(); }

    bool Initialize(const common::VirtualCameraConfig& config) override;
    bool PushFrame(const common::FrameView& frame) override;
    void Shutdown() override;

    common::VirtualCameraState GetState() const noexcept override { return state_; }
    const char* GetBackendName() const noexcept override {
        return "Windows 11 Media Foundation Virtual Camera (MFCreateVirtualCamera + SharedMemory IPC)";
    }

    bool IsComVirtualCameraActive() const noexcept { return vcam_handle_ != nullptr; }

    bool IsConsumerActive(uint64_t timeout_ms = 2000) const noexcept {
        return ipc_.IsOpen() && ipc_.IsConsumerActive(timeout_ms);
    }

private:
    common::VirtualCameraConfig      config_{};
    common::VirtualCameraState       state_        = common::VirtualCameraState::UNINITIALIZED;
    SharedFrameIpc                   ipc_{};
    Microsoft::WRL::ComPtr<IUnknown> vcam_handle_  = nullptr;
    HMODULE                          vcam_dll_     = nullptr;
    bool                             mf_started_   = false;
    uint64_t                         frame_count_  = 0;
};

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_VIRTUAL_CAMERA_SINK_H_
