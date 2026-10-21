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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_WIN_BRIDGE_C_API_H_
#define SMARTCAM_PLATFORMS_WINDOWS_WIN_BRIDGE_C_API_H_

#include <stdint.h>

#if defined(_WIN32) || defined(__CYGWIN__)
    #if defined(SMARTCAM_WIN_BRIDGE_STATIC)
        #define SMARTCAM_WIN_API
    #elif defined(SMARTCAM_WIN_BRIDGE_EXPORTS) || defined(SMARTCAM_ENGINE_EXPORTS)
        #define SMARTCAM_WIN_API __declspec(dllexport)
    #else
        #define SMARTCAM_WIN_API __declspec(dllimport)
    #endif
#else
    #define SMARTCAM_WIN_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SmartCamWinCameraHandle_t* SmartCamWinCameraHandle;
typedef struct SmartCamWinSinkHandle_t*   SmartCamWinSinkHandle;

/**
 * @brief Returns the number of hardware video capture devices enumerated via Media Foundation.
 */
SMARTCAM_WIN_API uint32_t smartcam_win_get_camera_count(void);

/**
 * @brief Retrieves UTF-8 friendly name, symbolic link, and Infrared (IR / Windows Hello) flag
 *        for the hardware video capture device at `index`.
 * @return 1 on success, 0 if `index` is out of range.
 */
SMARTCAM_WIN_API int smartcam_win_get_camera_info(
    uint32_t index,
    char* out_name,
    uint32_t name_cap,
    char* out_symlink,
    uint32_t sym_cap,
    int* out_is_infrared);

/**
 * @brief Creates a native Windows Media Foundation hardware camera capture instance.
 */
SMARTCAM_WIN_API SmartCamWinCameraHandle smartcam_win_camera_create(void);

/**
 * @brief Opens the hardware camera at `index` with requested resolution and framerate.
 */
SMARTCAM_WIN_API int smartcam_win_camera_open(
    SmartCamWinCameraHandle handle,
    uint32_t index,
    uint32_t width,
    uint32_t height,
    uint32_t fps);

/**
 * @brief Reads the next decoded `BGRA32` frame from the open hardware camera.
 */
SMARTCAM_WIN_API int smartcam_win_camera_read_frame(
    SmartCamWinCameraHandle handle,
    uint8_t* dst_bgra,
    uint32_t dst_cap,
    uint32_t* out_actual_w,
    uint32_t* out_actual_h);

/**
 * @brief Closes the active Media Foundation hardware camera stream.
 */
SMARTCAM_WIN_API void smartcam_win_camera_close(SmartCamWinCameraHandle handle);

/**
 * @brief Destroys the native hardware camera capture instance.
 */
SMARTCAM_WIN_API void smartcam_win_camera_destroy(SmartCamWinCameraHandle handle);

/**
 * @brief Creates a native Windows 11 Media Foundation Virtual Camera Sink + Shared Memory IPC producer.
 */
SMARTCAM_WIN_API SmartCamWinSinkHandle smartcam_win_sink_create(void);

/**
 * @brief Initializes the Virtual Camera Sink and Shared Memory IPC channel.
 */
SMARTCAM_WIN_API int smartcam_win_sink_init(
    SmartCamWinSinkHandle handle,
    const char* friendly_name,
    uint32_t width,
    uint32_t height,
    uint32_t fps);

/**
 * @brief Pushes a processed `BGRA32` frame to the Virtual Camera Sink.
 */
SMARTCAM_WIN_API int smartcam_win_sink_push_frame(
    SmartCamWinSinkHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride);

/**
 * @brief Checks if any consumer application (Zoom, Teams, Chrome, etc.) has requested frames
 *        from the Virtual Camera within the last 2 seconds (On-Demand "Set & Forget" mode).
 */
SMARTCAM_WIN_API int smartcam_win_sink_is_consumer_active(SmartCamWinSinkHandle handle);

/**
 * @brief Shuts down and destroys the Virtual Camera Sink instance.
 */
SMARTCAM_WIN_API void smartcam_win_sink_Destroy(SmartCamWinSinkHandle handle);
SMARTCAM_WIN_API void smartcam_win_sink_destroy(SmartCamWinSinkHandle handle);

#ifdef __cplusplus
}
#endif

#endif  // SMARTCAM_PLATFORMS_WINDOWS_WIN_BRIDGE_C_API_H_
