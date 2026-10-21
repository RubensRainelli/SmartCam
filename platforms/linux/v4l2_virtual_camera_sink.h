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

#ifndef SMARTCAM_PLATFORMS_LINUX_V4L2_VIRTUAL_CAMERA_SINK_H_
#define SMARTCAM_PLATFORMS_LINUX_V4L2_VIRTUAL_CAMERA_SINK_H_

#include <cstdint>
#include <string>
#include <vector>

#include "../../common/include/ivirtual_camera.h"

namespace smartcam {
namespace platforms {
namespace linux_os {

/**
 * @brief Native Linux Virtual Camera Sink (`IVirtualCameraSink`) backed by `v4l2loopback` (`/dev/videoN`).
 *
 * Responsibilities:
 *   1. Automatically discovers the `v4l2loopback` device node (`/dev/video0`..`/dev/video63`)
 *      matching `v4l2_capability.driver == "v4l2 loopback"` or `card` containing `"SmartCam"`.
 *   2. Negotiates the output format via `ioctl(fd, VIDIOC_S_FMT, &fmt)` with `V4L2_BUF_TYPE_VIDEO_OUTPUT`:
 *      - Prefers `V4L2_PIX_FMT_YUYV` (YUY2 4:2:2) for maximum compatibility with Chromium, Firefox,
 *        WebRTC, Zoom, Microsoft Teams, and OBS Studio on Linux (`exclusive_caps=1`).
 *      - Automatically falls back to `V4L2_PIX_FMT_RGB24` or `V4L2_PIX_FMT_BGR32` if requested/supported.
 *   3. Converts incoming `BGRA32` frames from the shared SIMD compositor on-the-fly into the negotiated
 *      V4L2 wire format and writes them to the character device via `write(fd, ...)`.
 *   4. Detects active consumer applications on Linux (`IsConsumerActive`) by inspecting `/proc/<pid>/fd/<n>`
 *      and `/sys/class/video4linux/videoN/` open reader references or `VIDIOC_G_FMT` state.
 */
class V4l2VirtualCameraSink final : public common::IVirtualCameraSink {
public:
    V4l2VirtualCameraSink() noexcept = default;
    ~V4l2VirtualCameraSink() override { Shutdown(); }

    bool Initialize(const common::VirtualCameraConfig& config) override;
    bool PushFrame(const common::FrameView& frame) override;
    void Shutdown() override;

    common::VirtualCameraState GetState() const noexcept override { return state_; }

    const char* GetBackendName() const noexcept override {
        return "Linux Video4Linux2 Virtual Camera Sink (v4l2loopback /dev/videoN)";
    }

    /**
     * @brief Returns the active `/dev/videoN` device path (e.g., `"/dev/video10"`).
     */
    const std::string& GetDeviceNodePath() const noexcept { return device_path_; }

    /**
     * @brief Checks whether any external Linux process (Chrome, Firefox, Zoom, OBS, etc.)
     *        currently holds an open file descriptor to the `v4l2loopback` device node.
     */
    bool IsConsumerActive(uint64_t timeout_ms = 2000) const noexcept;

    /**
     * @brief Scans `/dev/video0`..`/dev/video63` and returns the best matching `v4l2loopback` path.
     */
    static std::string FindV4l2LoopbackDevice(const std::string& preferred_card_name = "SmartCam");

private:
    void ConvertBgraToYuyv(
        const uint8_t* src_bgra,
        uint32_t width,
        uint32_t height,
        uint32_t src_stride,
        uint8_t* dst_yuyv) const noexcept;

    void ConvertBgraToRgb24(
        const uint8_t* src_bgra,
        uint32_t width,
        uint32_t height,
        uint32_t src_stride,
        uint8_t* dst_rgb24) const noexcept;

    common::VirtualCameraConfig config_{};
    common::VirtualCameraState  state_           = common::VirtualCameraState::UNINITIALIZED;
    int                         fd_              = -1;
    uint32_t                    negotiated_pix_  = 0;
    uint32_t                    negotiated_w_    = 0;
    uint32_t                    negotiated_h_    = 0;
    std::string                 device_path_     = "";
    std::vector<uint8_t>        tx_buffer_{};
    uint64_t                    frame_count_     = 0;
    mutable uint64_t            last_active_ms_  = 0;
};

}  // namespace linux_os
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_LINUX_V4L2_VIRTUAL_CAMERA_SINK_H_
