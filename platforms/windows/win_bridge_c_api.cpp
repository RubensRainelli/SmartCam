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

#define SMARTCAM_WIN_BRIDGE_EXPORTS 1
#include "win_bridge_c_api.h"
#include "capture/mf_camera_capture.h"
#include "drivers/media_foundation/mf_virtual_camera_sink.h"

#include <algorithm>
#include <cctype>
#include <cstring>
#include <new>
#include <string>

struct SmartCamWinCameraHandle_t {
    smartcam::platforms::windows::MfCameraCapture capture;
    smartcam::common::FrameBuffer                 scratch_frame;
};

struct SmartCamWinSinkHandle_t {
    smartcam::platforms::windows::MfVirtualCameraSink sink;
};

namespace {

void CopyStringSafe(const std::string& src, char* dst, uint32_t cap) noexcept {
    if (!dst || cap == 0) return;
    const size_t n = std::min<size_t>(src.size(), static_cast<size_t>(cap - 1));
    std::memcpy(dst, src.data(), n);
    dst[n] = '\0';
}

std::string ToUpperAscii(std::string s) {
    for (char& c : s) {
        c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
    }
    return s;
}

bool ContainsIrSignature(const std::string& friendly_name, const std::string& symbolic_link) {
    const std::string name_up = ToUpperAscii(friendly_name);
    const std::string sym_up  = ToUpperAscii(symbolic_link);

    if (name_up.find("INFRARED") != std::string::npos ||
        name_up.find("WINDOWS HELLO") != std::string::npos ||
        sym_up.find("INFRARED") != std::string::npos ||
        sym_up.find("WINDOWS HELLO") != std::string::npos ||
        sym_up.find("SENSOR_CAMERA") != std::string::npos) {
        return true;
    }

    // Check token-bounded "IR" in friendly_name or symbolic_link
    auto has_ir_token = [](const std::string& str) -> bool {
        size_t pos = 0;
        while ((pos = str.find("IR", pos)) != std::string::npos) {
            const bool left_boundary = (pos == 0) || !std::isalpha(static_cast<unsigned char>(str[pos - 1]));
            const bool right_boundary = (pos + 2 >= str.size()) || !std::isalpha(static_cast<unsigned char>(str[pos + 2]));
            if (left_boundary && right_boundary) {
                return true;
            }
            pos += 2;
        }
        return false;
    };

    return has_ir_token(name_up) || has_ir_token(sym_up);
}

}  // namespace

extern "C" {

SMARTCAM_WIN_API uint32_t smartcam_win_get_camera_count(void) {
    const auto devices = smartcam::platforms::windows::MfCameraCapture::EnumerateDevices();
    return static_cast<uint32_t>(devices.size());
}

SMARTCAM_WIN_API int smartcam_win_get_camera_info(
    uint32_t index,
    char* out_name,
    uint32_t name_cap,
    char* out_symlink,
    uint32_t sym_cap,
    int* out_is_infrared) {
    const auto devices = smartcam::platforms::windows::MfCameraCapture::EnumerateDevices();
    if (index >= devices.size()) {
        return 0;
    }

    const auto& dev = devices[index];
    CopyStringSafe(dev.friendly_name, out_name, name_cap);
    CopyStringSafe(dev.symbolic_link, out_symlink, sym_cap);

    if (out_is_infrared) {
        *out_is_infrared = ContainsIrSignature(dev.friendly_name, dev.symbolic_link) ? 1 : 0;
    }
    return 1;
}

SMARTCAM_WIN_API SmartCamWinCameraHandle smartcam_win_camera_create(void) {
    return new (std::nothrow) SmartCamWinCameraHandle_t();
}

SMARTCAM_WIN_API int smartcam_win_camera_open(
    SmartCamWinCameraHandle handle,
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

SMARTCAM_WIN_API int smartcam_win_camera_read_frame(
    SmartCamWinCameraHandle handle,
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

SMARTCAM_WIN_API void smartcam_win_camera_close(SmartCamWinCameraHandle handle) {
    if (!handle) return;
    handle->capture.Close();
}

SMARTCAM_WIN_API void smartcam_win_camera_destroy(SmartCamWinCameraHandle handle) {
    if (!handle) return;
    handle->capture.Close();
    delete handle;
}

SMARTCAM_WIN_API SmartCamWinSinkHandle smartcam_win_sink_create(void) {
    return new (std::nothrow) SmartCamWinSinkHandle_t();
}

SMARTCAM_WIN_API int smartcam_win_sink_init(
    SmartCamWinSinkHandle handle,
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

SMARTCAM_WIN_API int smartcam_win_sink_push_frame(
    SmartCamWinSinkHandle handle,
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

SMARTCAM_WIN_API int smartcam_win_sink_is_consumer_active(SmartCamWinSinkHandle handle) {
    if (!handle) return 0;
    return handle->sink.IsConsumerActive(2000) ? 1 : 0;
}

SMARTCAM_WIN_API void smartcam_win_sink_Destroy(SmartCamWinSinkHandle handle) {
    if (!handle) return;
    handle->sink.Shutdown();
    delete handle;
}

SMARTCAM_WIN_API void smartcam_win_sink_destroy(SmartCamWinSinkHandle handle) {
    smartcam_win_sink_Destroy(handle);
}

}  // extern "C"
