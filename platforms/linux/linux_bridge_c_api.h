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

#ifndef SMARTCAM_PLATFORMS_LINUX_LINUX_BRIDGE_C_API_H_
#define SMARTCAM_PLATFORMS_LINUX_LINUX_BRIDGE_C_API_H_

#include <stdint.h>

#if defined(__GNUC__) && __GNUC__ >= 4
    #define SMARTCAM_LINUX_API __attribute__((visibility("default")))
#else
    #define SMARTCAM_LINUX_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SmartCamLinuxCameraHandle_t* SmartCamLinuxCameraHandle;
typedef struct SmartCamLinuxSinkHandle_t*   SmartCamLinuxSinkHandle;

/* ============================================================================
 * Native Linux C ABI (`smartcam_linux_*`)
 * ============================================================================ */

/**
 * @brief Returns the number of physical V4L2 video capture devices enumerated on Linux.
 */
SMARTCAM_LINUX_API uint32_t smartcam_linux_get_camera_count(void);

/**
 * @brief Retrieves UTF-8 friendly name, V4L2 device node / bus path, and Infrared (`IR`/`GREY`) flag
 *        for the hardware video capture device at `index`.
 * @return 1 on success, 0 if `index` is out of range.
 */
SMARTCAM_LINUX_API int smartcam_linux_get_camera_info(
    uint32_t index,
    char* out_name,
    uint32_t name_cap,
    char* out_symlink,
    uint32_t sym_cap,
    int* out_is_infrared);

/**
 * @brief Creates a native Linux V4L2 hardware camera capture instance.
 */
SMARTCAM_LINUX_API SmartCamLinuxCameraHandle smartcam_linux_camera_create(void);

/**
 * @brief Opens the V4L2 hardware camera at `index` with requested resolution and framerate.
 */
SMARTCAM_LINUX_API int smartcam_linux_camera_open(
    SmartCamLinuxCameraHandle handle,
    uint32_t index,
    uint32_t width,
    uint32_t height,
    uint32_t fps);

/**
 * @brief Reads the next decoded `BGRA32` frame from the open V4L2 camera.
 */
SMARTCAM_LINUX_API int smartcam_linux_camera_read_frame(
    SmartCamLinuxCameraHandle handle,
    uint8_t* dst_bgra,
    uint32_t dst_cap,
    uint32_t* out_actual_w,
    uint32_t* out_actual_h);

/**
 * @brief Closes the active V4L2 hardware camera stream.
 */
SMARTCAM_LINUX_API void smartcam_linux_camera_close(SmartCamLinuxCameraHandle handle);

/**
 * @brief Destroys the native Linux hardware camera capture instance.
 */
SMARTCAM_LINUX_API void smartcam_linux_camera_destroy(SmartCamLinuxCameraHandle handle);

/**
 * @brief Creates a native Linux `v4l2loopback` Virtual Camera Sink instance.
 */
SMARTCAM_LINUX_API SmartCamLinuxSinkHandle smartcam_linux_sink_create(void);

/**
 * @brief Initializes the `v4l2loopback` Virtual Camera Sink (`/dev/videoN`).
 */
SMARTCAM_LINUX_API int smartcam_linux_sink_init(
    SmartCamLinuxSinkHandle handle,
    const char* friendly_name,
    uint32_t width,
    uint32_t height,
    uint32_t fps);

/**
 * @brief Pushes a processed `BGRA32` frame to the `v4l2loopback` Virtual Camera Sink.
 */
SMARTCAM_LINUX_API int smartcam_linux_sink_push_frame(
    SmartCamLinuxSinkHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride);

/**
 * @brief Checks if any consumer application (Chrome, Firefox, Zoom, Teams, OBS) has the
 *        `v4l2loopback` device open (On-Demand "0% CPU Idle" mode).
 */
SMARTCAM_LINUX_API int smartcam_linux_sink_is_consumer_active(SmartCamLinuxSinkHandle handle);

/**
 * @brief Shuts down and destroys the `v4l2loopback` Virtual Camera Sink instance.
 */
SMARTCAM_LINUX_API void smartcam_linux_sink_Destroy(SmartCamLinuxSinkHandle handle);
SMARTCAM_LINUX_API void smartcam_linux_sink_destroy(SmartCamLinuxSinkHandle handle);

/* ============================================================================
 * Cross-Platform Unified FFI Aliases (`smartcam_win_*`)
 * Exposes identical symbol names so the Rust `engine_bridge.rs` pipeline links
 * and loads on Linux without any platform-specific symbol branching.
 * ============================================================================ */

SMARTCAM_LINUX_API uint32_t smartcam_win_get_camera_count(void);
SMARTCAM_LINUX_API int smartcam_win_get_camera_info(
    uint32_t index,
    char* out_name,
    uint32_t name_cap,
    char* out_symlink,
    uint32_t sym_cap,
    int* out_is_infrared);
SMARTCAM_LINUX_API SmartCamLinuxCameraHandle smartcam_win_camera_create(void);
SMARTCAM_LINUX_API int smartcam_win_camera_open(
    SmartCamLinuxCameraHandle handle,
    uint32_t index,
    uint32_t width,
    uint32_t height,
    uint32_t fps);
SMARTCAM_LINUX_API int smartcam_win_camera_read_frame(
    SmartCamLinuxCameraHandle handle,
    uint8_t* dst_bgra,
    uint32_t dst_cap,
    uint32_t* out_actual_w,
    uint32_t* out_actual_h);
SMARTCAM_LINUX_API void smartcam_win_camera_close(SmartCamLinuxCameraHandle handle);
SMARTCAM_LINUX_API void smartcam_win_camera_destroy(SmartCamLinuxCameraHandle handle);

SMARTCAM_LINUX_API SmartCamLinuxSinkHandle smartcam_win_sink_create(void);
SMARTCAM_LINUX_API int smartcam_win_sink_init(
    SmartCamLinuxSinkHandle handle,
    const char* friendly_name,
    uint32_t width,
    uint32_t height,
    uint32_t fps);
SMARTCAM_LINUX_API int smartcam_win_sink_push_frame(
    SmartCamLinuxSinkHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride);
SMARTCAM_LINUX_API int smartcam_win_sink_is_consumer_active(SmartCamLinuxSinkHandle handle);
SMARTCAM_LINUX_API void smartcam_win_sink_Destroy(SmartCamLinuxSinkHandle handle);
SMARTCAM_LINUX_API void smartcam_win_sink_destroy(SmartCamLinuxSinkHandle handle);

#ifdef __cplusplus
}
#endif

#endif  // SMARTCAM_PLATFORMS_LINUX_LINUX_BRIDGE_C_API_H_
