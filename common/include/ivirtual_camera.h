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

#ifndef SMARTCAM_COMMON_IVIRTUAL_CAMERA_H_
#define SMARTCAM_COMMON_IVIRTUAL_CAMERA_H_

#include <cstdint>
#include <string>
#include "frame_buffer.h"

namespace smartcam {
namespace common {

/**
 * @brief Configuration parameters for initializing an OS-level Virtual Camera Sink.
 */
struct VirtualCameraConfig {
    std::string friendly_name = "SmartCam";                 ///< Device name visible in browsers/Teams/OBS
    uint32_t    width         = 1280;                       ///< Output width in pixels
    uint32_t    height        = 720;                        ///< Output height in pixels
    uint32_t    fps_num       = 30;                         ///< Target framerate numerator
    uint32_t    fps_den       = 1;                          ///< Target framerate denominator
    PixelFormat pixel_format  = PixelFormat::BGRA32;        ///< Expected input frame format pushed by pipeline
};

/**
 * @brief Operational state of the virtual camera sink.
 */
enum class VirtualCameraState : uint32_t {
    UNINITIALIZED = 0,
    READY         = 1,
    STREAMING     = 2,
    ERROR_STATE   = 3,
    STOPPED       = 4
};

/**
 * @brief Abstract platform-agnostic contract for an OS Virtual Camera Sink.
 *
 * Implementations reside in `platforms/<os>/drivers/`:
 * - Windows 11: Media Foundation Virtual Camera (`MFCreateVirtualCamera` + Shared Memory IPC Sink)
 * - Linux:      `v4l2loopback` device writer (`/dev/videoX`)
 * - macOS:      CoreMediaIO Camera Extension / DAL Sink
 */
class IVirtualCameraSink {
public:
    virtual ~IVirtualCameraSink() = default;

    /**
     * @brief Initializes and registers the runtime virtual camera stream at standard user privileges.
     * @param config Desired output stream configuration.
     * @return true if the virtual camera sink is ready to accept frames, false otherwise.
     */
    virtual bool Initialize(const VirtualCameraConfig& config) = 0;

    /**
     * @brief Pushes a processed video frame to the virtual camera output stream.
     * @param frame Read-only view of the composited frame matching configured dimensions.
     * @return true if frame was dispatched to the OS sink/shared memory, false on error.
     */
    virtual bool PushFrame(const FrameView& frame) = 0;

    /**
     * @brief Stops streaming and releases runtime OS handles.
     */
    virtual void Shutdown() = 0;

    /**
     * @brief Returns current state of the virtual camera sink.
     */
    virtual VirtualCameraState GetState() const noexcept = 0;

    /**
     * @brief Returns human-readable description of the active backend (e.g. "Windows Media Foundation Virtual Camera").
     */
    virtual const char* GetBackendName() const noexcept = 0;
};

}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_IVIRTUAL_CAMERA_H_
