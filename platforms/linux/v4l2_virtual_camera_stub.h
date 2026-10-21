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

#ifndef SMARTCAM_PLATFORMS_LINUX_V4L2_VIRTUAL_CAMERA_STUB_H_
#define SMARTCAM_PLATFORMS_LINUX_V4L2_VIRTUAL_CAMERA_STUB_H_

#include "../../common/include/ivirtual_camera.h"

namespace smartcam {
namespace platforms {
namespace linux_os {

/**
 * @brief [STUB/FUTURO] Linux `v4l2loopback` Virtual Camera Sink (`IVirtualCameraSink`).
 *
 * Target workflow for Phase 2 (Linux Agent):
 *   1. Open `/dev/videoX` created by `modprobe v4l2loopback exclusive_caps=1 card_label="SmartCam Virtual Camera"`.
 *   2. Issue `ioctl(fd, VIDIOC_S_FMT, &v4l2_format)` with `V4L2_PIX_FMT_YUYV` or `V4L2_PIX_FMT_RGB24`.
 *   3. Write composited frames from `common/engine/` via `write(fd, frame.data, frame.ByteSize())`.
 */
class V4l2VirtualCameraStub final : public common::IVirtualCameraSink {
public:
    V4l2VirtualCameraStub() noexcept = default;
    ~V4l2VirtualCameraStub() override = default;

    bool Initialize(const common::VirtualCameraConfig& /*config*/) override { return false; }
    bool PushFrame(const common::FrameView& /*frame*/) override { return false; }
    void Shutdown() override {}

    common::VirtualCameraState GetState() const noexcept override {
        return common::VirtualCameraState::UNINITIALIZED;
    }

    const char* GetBackendName() const noexcept override {
        return "Linux v4l2loopback Virtual Camera Sink [Stub/Phase 2]";
    }
};

}  // namespace linux_os
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_LINUX_V4L2_VIRTUAL_CAMERA_STUB_H_
