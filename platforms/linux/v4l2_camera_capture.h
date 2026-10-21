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

#ifndef SMARTCAM_PLATFORMS_LINUX_V4L2_CAMERA_CAPTURE_H_
#define SMARTCAM_PLATFORMS_LINUX_V4L2_CAMERA_CAPTURE_H_

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "../../common/include/icamera_source.h"

namespace smartcam {
namespace platforms {
namespace linux_os {

/**
 * @brief Metadata for an enumerated Linux Video4Linux2 (`V4L2`) hardware video capture device.
 */
struct V4l2CameraDeviceInfo {
    uint32_t    index         = 0;      ///< Logical index in EnumerateDevices()
    std::string friendly_name = "";     ///< Human-readable V4L2 card name (e.g. "Integrated Camera: Integrated C")
    std::string symbolic_link = "";     ///< Device node & bus_info (e.g. "/dev/video0 (usb-0000:00:14.0-8)")
    std::string dev_node      = "";     ///< Character device path (e.g. "/dev/video0")
    bool        is_infrared   = false;  ///< True if IR / Grey-only / Windows Hello / Howdy sensor
};

/**
 * @brief Native Linux Video4Linux2 (`V4L2`) hardware webcam capture (`ICameraSource`).
 *
 * Uses zero-copy kernel ring buffers mapped via `V4L2_MEMORY_MMAP` (`VIDIOC_REQBUFS`,
 * `VIDIOC_QUERYBUF`, `VIDIOC_QBUF`, `VIDIOC_DQBUF`, `VIDIOC_STREAMON`) and converts
 * captured frames (`YUYV`, `NV12`, `RGB24`, `BGR24`, `BGR32`, `GREY`) into 64-byte
 * SIMD-aligned `BGRA32` (`common::FrameBuffer`).
 */
class V4l2CameraCapture final : public common::ICameraSource {
public:
    V4l2CameraCapture() noexcept = default;
    ~V4l2CameraCapture() override { Close(); }

    /**
     * @brief Enumerates all physical V4L2 video capture devices (`/dev/video*`),
     *        automatically filtering out `v4l2loopback` virtual sinks and metadata-only nodes,
     *        and flagging Infrared (`IR` / `GREY`) sensors.
     */
    static std::vector<V4l2CameraDeviceInfo> EnumerateDevices();

    bool Open(const common::CameraCaptureConfig& config) override;
    bool ReadFrame(common::FrameBuffer& out_frame, uint32_t timeout_ms = 1000) override;
    void Close() override;

    bool IsOpen() const noexcept override { return is_open_; }
    common::CameraCaptureConfig GetActualConfig() const noexcept override { return actual_config_; }

private:
    struct MmapBuffer {
        void*  start  = nullptr;
        size_t length = 0;
    };

    void DecodeToBgra32(
        const uint8_t* src,
        size_t src_bytes,
        common::FrameBuffer& out_frame) const noexcept;

    int                         fd_             = -1;
    std::vector<MmapBuffer>     buffers_{};
    common::CameraCaptureConfig actual_config_{};
    uint32_t                    v4l2_pixfmt_    = 0;
    uint32_t                    bytes_per_line_ = 0;
    uint64_t                    frame_counter_  = 0;
    bool                        streaming_      = false;
    bool                        is_open_        = false;
};

}  // namespace linux_os
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_LINUX_V4L2_CAMERA_CAPTURE_H_
