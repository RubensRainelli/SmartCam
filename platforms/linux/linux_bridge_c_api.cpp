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

#include "linux_bridge_c_api.h"
#include "v4l2_camera_capture.h"
#include "v4l2_virtual_camera_sink.h"

#include <algorithm>
#include <cstring>
#include <new>
#include <string>

struct SmartCamLinuxCameraHandle_t {
    smartcam::platforms::linux_os::V4l2CameraCapture capture;
    smartcam::common::FrameBuffer                    scratch_frame;
};

struct SmartCamLinuxSinkHandle_t {
    smartcam::platforms::linux_os::V4l2VirtualCameraSink sink;
};

namespace {

void CopyStringSafe(const std::string& src, char* dst, uint32_t cap) noexcept {
    if (!dst || cap == 0) return;
    const size_t n = std::min<size_t>(src.size(), static_cast<size_t>(cap - 1));
    std::memcpy(dst, src.data(), n);
    dst[n] = '\0';
}

}  // namespace

extern "C" {

/* ============================================================================
 * Native Linux C ABI Implementation (`smartcam_linux_*`)
 * ============================================================================ */

SMARTCAM_LINUX_API uint32_t smartcam_linux_get_camera_count(void) {
    const auto devices = smartcam::platforms::linux_os::V4l2CameraCapture::EnumerateDevices();
    return static_cast<uint32_t>(devices.size());
}

SMARTCAM_LINUX_API int smartcam_linux_get_camera_info(
    uint32_t index,
    char* out_name,
    uint32_t name_cap,
    char* out_symlink,
    uint32_t sym_cap,
    int* out_is_infrared) {
    const auto devices = smartcam::platforms::linux_os::V4l2CameraCapture::EnumerateDevices();
    if (index >= devices.size()) {
        return 0;
    }

    const auto& dev = devices[index];
    CopyStringSafe(dev.friendly_name, out_name, name_cap);
    CopyStringSafe(dev.symbolic_link, out_symlink, sym_cap);

    if (out_is_infrared) {
        *out_is_infrared = dev.is_infrared ? 1 : 0;
    }
    return 1;
}

SMARTCAM_LINUX_API SmartCamLinuxCameraHandle smartcam_linux_camera_create(void) {
    return new (std::nothrow) SmartCamLinuxCameraHandle_t();
}

SMARTCAM_LINUX_API int smartcam_linux_camera_open(
    SmartCamLinuxCameraHandle handle,
    uint32_t index,
    uint32_t width,
    uint32_t height,
    uint32_t fps) {
    if (!handle) return 0;
    smartcam::common::CameraCaptureConfig cfg;
    cfg.device_index = static_cast<int32_t>(index);
    cfg.width        = (width > 0) ? width : 1280;
    cfg.height       = (height > 0) ? height : 720;
    cfg.target_fps   = (fps > 0) ? fps : 30;
    return handle->capture.Open(cfg) ? 1 : 0;
}

SMARTCAM_LINUX_API int smartcam_linux_camera_read_frame(
    SmartCamLinuxCameraHandle handle,
    uint8_t* dst_bgra,
    uint32_t dst_cap,
    uint32_t* out_actual_w,
    uint32_t* out_actual_h) {
    if (!handle || !dst_bgra) return 0;
    if (!handle->capture.ReadFrame(handle->scratch_frame)) {
        return 0;
    }

    const uint32_t w = handle->scratch_frame.Width();
    const uint32_t h = handle->scratch_frame.Height();
    const size_t needed = static_cast<size_t>(w) * h * 4;
    if (out_actual_w) *out_actual_w = w;
    if (out_actual_h) *out_actual_h = h;

    if (needed == 0 || needed > static_cast<size_t>(dst_cap)) {
        return 0;
    }

    std::memcpy(dst_bgra, handle->scratch_frame.Data(), needed);
    return 1;
}

SMARTCAM_LINUX_API void smartcam_linux_camera_close(SmartCamLinuxCameraHandle handle) {
    if (!handle) return;
    handle->capture.Close();
}

SMARTCAM_LINUX_API void smartcam_linux_camera_destroy(SmartCamLinuxCameraHandle handle) {
    if (!handle) return;
    handle->capture.Close();
    delete handle;
}

SMARTCAM_LINUX_API SmartCamLinuxSinkHandle smartcam_linux_sink_create(void) {
    return new (std::nothrow) SmartCamLinuxSinkHandle_t();
}

SMARTCAM_LINUX_API int smartcam_linux_sink_init(
    SmartCamLinuxSinkHandle handle,
    const char* friendly_name,
    uint32_t width,
    uint32_t height,
    uint32_t fps) {
    if (!handle) return 0;
    smartcam::common::VirtualCameraConfig cfg;
    cfg.friendly_name = (friendly_name && *friendly_name) ? friendly_name : "SmartCam";
    cfg.width         = (width > 0) ? width : 1440;
    cfg.height        = (height > 0) ? height : 1080;
    cfg.fps_num       = (fps > 0) ? fps : 30;
    cfg.fps_den       = 1;
    return handle->sink.Initialize(cfg) ? 1 : 0;
}

SMARTCAM_LINUX_API int smartcam_linux_sink_push_frame(
    SmartCamLinuxSinkHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride) {
    if (!handle || !src_bgra || width == 0 || height == 0) return 0;
    const uint32_t row_stride = (stride >= width * 4) ? stride : (width * 4);
    smartcam::common::FrameView view{
        const_cast<uint8_t*>(src_bgra),
        width,
        height,
        row_stride,
        smartcam::common::PixelFormat::BGRA32,
        0,
        0
    };
    return handle->sink.PushFrame(view) ? 1 : 0;
}

SMARTCAM_LINUX_API int smartcam_linux_sink_is_consumer_active(SmartCamLinuxSinkHandle handle) {
    if (!handle) return 0;
    return handle->sink.IsConsumerActive(2000) ? 1 : 0;
}

SMARTCAM_LINUX_API void smartcam_linux_sink_Destroy(SmartCamLinuxSinkHandle handle) {
    if (!handle) return;
    handle->sink.Shutdown();
    delete handle;
}

SMARTCAM_LINUX_API void smartcam_linux_sink_destroy(SmartCamLinuxSinkHandle handle) {
    smartcam_linux_sink_Destroy(handle);
}

/* ============================================================================
 * Cross-Platform Unified FFI Aliases (`smartcam_win_*` -> `smartcam_linux_*`)
 * ============================================================================ */

SMARTCAM_LINUX_API uint32_t smartcam_win_get_camera_count(void) {
    return smartcam_linux_get_camera_count();
}

SMARTCAM_LINUX_API int smartcam_win_get_camera_info(
    uint32_t index,
    char* out_name,
    uint32_t name_cap,
    char* out_symlink,
    uint32_t sym_cap,
    int* out_is_infrared) {
    return smartcam_linux_get_camera_info(
        index, out_name, name_cap, out_symlink, sym_cap, out_is_infrared);
}

SMARTCAM_LINUX_API SmartCamLinuxCameraHandle smartcam_win_camera_create(void) {
    return smartcam_linux_camera_create();
}

SMARTCAM_LINUX_API int smartcam_win_camera_open(
    SmartCamLinuxCameraHandle handle,
    uint32_t index,
    uint32_t width,
    uint32_t height,
    uint32_t fps) {
    return smartcam_linux_camera_open(handle, index, width, height, fps);
}

SMARTCAM_LINUX_API int smartcam_win_camera_read_frame(
    SmartCamLinuxCameraHandle handle,
    uint8_t* dst_bgra,
    uint32_t dst_cap,
    uint32_t* out_actual_w,
    uint32_t* out_actual_h) {
    return smartcam_linux_camera_read_frame(handle, dst_bgra, dst_cap, out_actual_w, out_actual_h);
}

SMARTCAM_LINUX_API void smartcam_win_camera_close(SmartCamLinuxCameraHandle handle) {
    smartcam_linux_camera_close(handle);
}

SMARTCAM_LINUX_API void smartcam_win_camera_destroy(SmartCamLinuxCameraHandle handle) {
    smartcam_linux_camera_destroy(handle);
}

SMARTCAM_LINUX_API SmartCamLinuxSinkHandle smartcam_win_sink_create(void) {
    return smartcam_linux_sink_create();
}

SMARTCAM_LINUX_API int smartcam_win_sink_init(
    SmartCamLinuxSinkHandle handle,
    const char* friendly_name,
    uint32_t width,
    uint32_t height,
    uint32_t fps) {
    return smartcam_linux_sink_init(handle, friendly_name, width, height, fps);
}

SMARTCAM_LINUX_API int smartcam_win_sink_push_frame(
    SmartCamLinuxSinkHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride) {
    return smartcam_linux_sink_push_frame(handle, src_bgra, width, height, stride);
}

SMARTCAM_LINUX_API int smartcam_win_sink_is_consumer_active(SmartCamLinuxSinkHandle handle) {
    return smartcam_linux_sink_is_consumer_active(handle);
}

SMARTCAM_LINUX_API void smartcam_win_sink_Destroy(SmartCamLinuxSinkHandle handle) {
    smartcam_linux_sink_Destroy(handle);
}

SMARTCAM_LINUX_API void smartcam_win_sink_destroy(SmartCamLinuxSinkHandle handle) {
    smartcam_linux_sink_destroy(handle);
}

}  // extern "C"
