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

#ifndef SMARTCAM_COMMON_ICAMERA_SOURCE_H_
#define SMARTCAM_COMMON_ICAMERA_SOURCE_H_

#include <cstdint>
#include <string>
#include "frame_buffer.h"

namespace smartcam {
namespace common {

/**
 * @brief Hardware or remote camera capture configuration.
 */
struct CameraCaptureConfig {
    int32_t     device_index  = 0;                   ///< Hardware camera index (0 = default webcam)
    std::string device_id     = "";                  ///< Optional OS symbolic link / unique hardware ID
    uint32_t    width         = 1920;                ///< Requested capture width (e.g. 1920)
    uint32_t    height        = 1080;                ///< Requested capture height (e.g. 1080)
    uint32_t    target_fps    = 30;                  ///< Requested framerate (e.g. 30 or 60)
    uint32_t    bit_depth     = 64;                  ///< Source bit depth: 64 (`BGRA64` 16-bit/ch Deep Color) or 32 (`BGRA32` 8-bit/ch)
    PixelFormat output_format = PixelFormat::BGRA32; ///< Desired decoded pixel format (`BGRA32` or `BGRA64`)
};

/**
 * @brief Abstract platform-agnostic contract for video frame acquisition.
 * Implemented by native OS capture modules (`platforms/windows/capture/`) and plugins (`common/plugins/`).
 */
class ICameraSource {
public:
    virtual ~ICameraSource() = default;

    /**
     * @brief Opens the hardware or remote camera device with the requested parameters.
     */
    virtual bool Open(const CameraCaptureConfig& config) = 0;

    /**
     * @brief Captures the next decoded video frame into `out_frame`.
     * @param out_frame Buffer updated with the latest camera frame.
     * @param timeout_ms Maximum wait time in milliseconds.
     * @return true if a new frame was acquired, false on timeout or device disconnect.
     */
    virtual bool ReadFrame(FrameBuffer& out_frame, uint32_t timeout_ms = 1000) = 0;

    /**
     * @brief Releases the camera device.
     */
    virtual void Close() = 0;

    /**
     * @brief Returns true if the camera source is currently open and capturing.
     */
    virtual bool IsOpen() const noexcept = 0;

    /**
     * @brief Returns the actual negotiated capture configuration.
     */
    virtual CameraCaptureConfig GetActualConfig() const noexcept = 0;
};

}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ICAMERA_SOURCE_H_
