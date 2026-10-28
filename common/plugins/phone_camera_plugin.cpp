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

#include "phone_camera_plugin.h"

#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <cstring>

namespace smartcam {
namespace common {
namespace plugins {

namespace {

inline uint64_t GetMonotonicTimestampNs() noexcept {
    const auto now = std::chrono::steady_clock::now().time_since_epoch();
    return static_cast<uint64_t>(
        std::chrono::duration_cast<std::chrono::nanoseconds>(now).count());
}

inline void DrawFilledRectBgra(
    uint8_t* data,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    uint32_t rx,
    uint32_t ry,
    uint32_t rw,
    uint32_t rh,
    uint8_t  b,
    uint8_t  g,
    uint8_t  r,
    uint8_t  a = 255) noexcept {
    if (!data || rx >= width || ry >= height) {
        return;
    }
    const uint32_t x_end = std::min(width, rx + rw);
    const uint32_t y_end = std::min(height, ry + rh);
    for (uint32_t y = ry; y < y_end; ++y) {
        uint8_t* row = data + static_cast<size_t>(y) * stride;
        for (uint32_t x = rx; x < x_end; ++x) {
            uint8_t* px = row + static_cast<size_t>(x) * 4;
            px[0] = b;
            px[1] = g;
            px[2] = r;
            px[3] = a;
        }
    }
}

inline void DrawBorderRectBgra(
    uint8_t* data,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    uint32_t rx,
    uint32_t ry,
    uint32_t rw,
    uint32_t rh,
    uint32_t thickness,
    uint8_t  b,
    uint8_t  g,
    uint8_t  r) noexcept {
    DrawFilledRectBgra(data, width, height, stride, rx, ry, rw, thickness, b, g, r);
    if (rh > thickness) {
        DrawFilledRectBgra(data, width, height, stride, rx, ry + rh - thickness, rw, thickness, b, g, r);
    }
    DrawFilledRectBgra(data, width, height, stride, rx, ry, thickness, rh, b, g, r);
    if (rw > thickness) {
        DrawFilledRectBgra(data, width, height, stride, rx + rw - thickness, ry, thickness, rh, b, g, r);
    }
}

}  // namespace

PhoneCameraPlugin::PhoneCameraPlugin() {
    uri_config_.protocol     = PluginProtocol::HTTP_MJPEG_PHONE;
    uri_config_.endpoint_uri = "http://192.168.1.100:8080/video";
    parsed_endpoint_         = ParseEndpointUri(uri_config_.endpoint_uri);
}

PhoneCameraPlugin::~PhoneCameraPlugin() {
    Close();
}

ParsedPhoneEndpoint PhoneCameraPlugin::ParseEndpointUri(const std::string& uri) {
    ParsedPhoneEndpoint out{};
    if (uri.empty()) {
        return out;
    }

    std::string work = uri;
    const auto scheme_pos = work.find("://");
    if (scheme_pos != std::string::npos) {
        out.scheme = work.substr(0, scheme_pos);
        work       = work.substr(scheme_pos + 3);
    }

    const auto slash_pos = work.find('/');
    std::string host_port = work;
    if (slash_pos != std::string::npos) {
        host_port = work.substr(0, slash_pos);
        out.path  = work.substr(slash_pos);
    } else {
        out.path = "/video";
    }

    const auto colon_pos = host_port.find(':');
    if (colon_pos != std::string::npos) {
        out.host = host_port.substr(0, colon_pos);
        const int parsed_port = std::atoi(host_port.substr(colon_pos + 1).c_str());
        if (parsed_port > 0 && parsed_port <= 65535) {
            out.port = static_cast<uint16_t>(parsed_port);
        }
    } else if (!host_port.empty()) {
        out.host = host_port;
        out.port = (out.scheme == "rtsp") ? 8554 : 8080;
    }

    out.is_usb = (out.scheme == "usb" || out.host == "127.0.0.1" || out.host == "localhost");
    return out;
}

bool PhoneCameraPlugin::ConfigurePlugin(const PluginConnectionUri& uri_config) {
    std::lock_guard<std::mutex> lock(mutex_);
    uri_config_      = uri_config;
    parsed_endpoint_ = ParseEndpointUri(uri_config_.endpoint_uri);
    return true;
}

bool PhoneCameraPlugin::SetEndpointUri(const std::string& endpoint_uri) {
    if (endpoint_uri.empty()) {
        return false;
    }
    std::lock_guard<std::mutex> lock(mutex_);
    uri_config_.endpoint_uri = endpoint_uri;
    parsed_endpoint_         = ParseEndpointUri(endpoint_uri);
    uri_config_.protocol     = parsed_endpoint_.is_usb
                                   ? PluginProtocol::USB_ADB_TETHER
                                   : PluginProtocol::HTTP_MJPEG_PHONE;
    return true;
}

std::string PhoneCameraPlugin::GetEndpointUri() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return uri_config_.endpoint_uri;
}

PhoneConnectionState PhoneCameraPlugin::GetConnectionState() const noexcept {
    std::lock_guard<std::mutex> lock(mutex_);
    return connection_state_;
}

const char* PhoneCameraPlugin::GetPluginId() const noexcept {
    return "phone_camera_source";
}

const char* PhoneCameraPlugin::GetPluginName() const noexcept {
    return "SmartCam Smartphone Camera Input Plugin (Wi-Fi / USB / IP Webcam) [v1.0]";
}

void PhoneCameraPlugin::SetEnabled(bool enabled) noexcept {
    std::lock_guard<std::mutex> lock(mutex_);
    enabled_ = enabled;
    if (!enabled_) {
        is_open_          = false;
        has_live_frame_   = false;
        connection_state_ = PhoneConnectionState::DISCONNECTED;
    }
}

bool PhoneCameraPlugin::IsEnabled() const noexcept {
    std::lock_guard<std::mutex> lock(mutex_);
    return enabled_;
}

bool PhoneCameraPlugin::Open(const CameraCaptureConfig& config) {
    std::lock_guard<std::mutex> lock(mutex_);
    config_               = config;
    config_.width         = (config.width > 0) ? config.width : 1280;
    config_.height        = (config.height > 0) ? config.height : 720;
    config_.target_fps    = (config.target_fps > 0) ? config.target_fps : 30;
    config_.output_format = PixelFormat::BGRA32;
    enabled_              = true;
    is_open_              = true;
    connection_state_     = has_live_frame_
                                ? PhoneConnectionState::STREAMING_ACTIVE
                                : PhoneConnectionState::PAIRING_STANDBY;
    return true;
}

bool PhoneCameraPlugin::PushDecodedPhoneFrame(
    const uint8_t* bgra_data,
    uint32_t       width,
    uint32_t       height,
    uint32_t       stride) {
    if (!bgra_data || width == 0 || height == 0 || stride < width * 4) {
        return false;
    }
    std::lock_guard<std::mutex> lock(mutex_);
    if (!latest_phone_frame_.Allocate(width, height, PixelFormat::BGRA32)) {
        return false;
    }

    uint8_t* dst = latest_phone_frame_.Data();
    const uint32_t dst_stride = latest_phone_frame_.Stride();
    const uint32_t copy_bytes = width * 4;
    for (uint32_t y = 0; y < height; ++y) {
        std::memcpy(
            dst + static_cast<size_t>(y) * dst_stride,
            bgra_data + static_cast<size_t>(y) * stride,
            copy_bytes);
    }

    has_live_frame_   = true;
    connection_state_ = PhoneConnectionState::STREAMING_ACTIVE;
    return true;
}

void PhoneCameraPlugin::RenderPairingCardFrame(FrameBuffer& out_frame) {
    const uint32_t w = (config_.width >= 320) ? config_.width : 1280;
    const uint32_t h = (config_.height >= 240) ? config_.height : 720;

    if (!out_frame.IsValid() || out_frame.Width() != w || out_frame.Height() != h ||
        out_frame.Format() != PixelFormat::BGRA32) {
        out_frame.Allocate(w, h, PixelFormat::BGRA32);
    }

    uint8_t* data = out_frame.Data();
    const uint32_t stride = out_frame.Stride();

    // 1. Studio Dark-Navy / Cyan Gradient Background
    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = data + static_cast<size_t>(y) * stride;
        const uint8_t grad_b = static_cast<uint8_t>(26 + (y * 28) / h);
        const uint8_t grad_g = static_cast<uint8_t>(16 + (y * 22) / h);
        const uint8_t grad_r = static_cast<uint8_t>(10 + (y * 12) / h);
        for (uint32_t x = 0; x < w; ++x) {
            uint8_t* px = row + static_cast<size_t>(x) * 4;
            px[0] = grad_b;
            px[1] = grad_g;
            px[2] = grad_r;
            px[3] = 255;
        }
    }

    // 2. Central Pairing Card Panel
    const uint32_t card_w = std::min(w - 40, std::max(280u, (w * 3) / 5));
    const uint32_t card_h = std::min(h - 40, std::max(180u, (h * 3) / 5));
    const uint32_t card_x = (w - card_w) / 2;
    const uint32_t card_y = (h - card_h) / 2;

    DrawFilledRectBgra(data, w, h, stride, card_x, card_y, card_w, card_h, 42, 28, 18);
    DrawBorderRectBgra(data, w, h, stride, card_x, card_y, card_w, card_h, 3, 235, 190, 40);

    // 3. Stylized Smartphone Silhouette in the center-left of the card
    const uint32_t phone_w = std::max(48u, card_w / 7);
    const uint32_t phone_h = std::max(84u, (card_h * 3) / 5);
    const uint32_t phone_x = card_x + card_w / 8;
    const uint32_t phone_y = card_y + (card_h - phone_h) / 2;

    DrawFilledRectBgra(data, w, h, stride, phone_x, phone_y, phone_w, phone_h, 24, 18, 12);
    DrawBorderRectBgra(data, w, h, stride, phone_x, phone_y, phone_w, phone_h, 3, 255, 210, 60);

    // Smartphone camera lens & speaker notch
    DrawFilledRectBgra(
        data, w, h, stride,
        phone_x + phone_w / 3, phone_y + 8, phone_w / 3, 4,
        120, 220, 40);

    // 4. Animated Signal / Scan Pulse Bar (indicates active polling on endpoint)
    const uint32_t bar_area_x = phone_x + phone_w + card_w / 12;
    const uint32_t bar_area_w = (card_x + card_w > bar_area_x + 24)
                                    ? (card_x + card_w - bar_area_x - 24)
                                    : 64;
    const uint32_t bar_y = card_y + card_h / 2 - 12;

    DrawFilledRectBgra(data, w, h, stride, bar_area_x, bar_y, bar_area_w, 18, 30, 20, 14);
    const uint32_t pulse_w = std::max(16u, bar_area_w / 4);
    const uint32_t max_offset = (bar_area_w > pulse_w) ? (bar_area_w - pulse_w) : 1;
    const uint32_t pulse_offset = static_cast<uint32_t>((frame_counter_ * 6) % max_offset);
    DrawFilledRectBgra(
        data, w, h, stride,
        bar_area_x + pulse_offset, bar_y, pulse_w, 18,
        140, 235, 45);

    // USB vs Wi-Fi status indicator pill
    const uint8_t pill_b = parsed_endpoint_.is_usb ? 255 : 130;
    const uint8_t pill_g = parsed_endpoint_.is_usb ? 180 : 230;
    const uint8_t pill_r = parsed_endpoint_.is_usb ? 50  : 40;
    DrawFilledRectBgra(
        data, w, h, stride,
        bar_area_x, card_y + card_h / 4, std::min(160u, bar_area_w), 14,
        pill_b, pill_g, pill_r);

    out_frame.View().timestamp_ns = GetMonotonicTimestampNs();
    out_frame.View().frame_index  = ++frame_counter_;
}

bool PhoneCameraPlugin::ReadFrame(FrameBuffer& out_frame, uint32_t /*timeout_ms*/) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!is_open_ || !enabled_) {
        return false;
    }

    if (has_live_frame_ && latest_phone_frame_.IsValid()) {
        out_frame = latest_phone_frame_;
        out_frame.View().timestamp_ns = GetMonotonicTimestampNs();
        out_frame.View().frame_index  = ++frame_counter_;
        return true;
    }

    // In standby/pairing mode, produce the HD Pairing Card Frame so the pipeline & preview stay live
    RenderPairingCardFrame(out_frame);
    return true;
}

void PhoneCameraPlugin::Close() {
    std::lock_guard<std::mutex> lock(mutex_);
    is_open_          = false;
    has_live_frame_   = false;
    connection_state_ = PhoneConnectionState::DISCONNECTED;
}

bool PhoneCameraPlugin::IsOpen() const noexcept {
    std::lock_guard<std::mutex> lock(mutex_);
    return is_open_;
}

CameraCaptureConfig PhoneCameraPlugin::GetActualConfig() const noexcept {
    std::lock_guard<std::mutex> lock(mutex_);
    return config_;
}

// ============================================================================
// C ABI v1 Bridge for `phone_camera_source`
// ============================================================================

namespace {

void* PhonePluginCInit() {
    return new (std::nothrow) PhoneCameraPlugin();
}

int32_t PhonePluginCOpen(
    void*       instance,
    uint32_t    width,
    uint32_t    height,
    uint32_t    fps,
    const char* endpoint_uri) {
    auto* plugin = static_cast<PhoneCameraPlugin*>(instance);
    if (!plugin) {
        return 0;
    }
    if (endpoint_uri && endpoint_uri[0] != '\0') {
        plugin->SetEndpointUri(endpoint_uri);
    }
    CameraCaptureConfig cfg{};
    cfg.width         = width;
    cfg.height        = height;
    cfg.target_fps    = fps;
    cfg.output_format = PixelFormat::BGRA32;
    return plugin->Open(cfg) ? 1 : 0;
}

int32_t PhonePluginCReadFrame(
    void*    instance,
    uint8_t* out_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    uint32_t timeout_ms) {
    auto* plugin = static_cast<PhoneCameraPlugin*>(instance);
    if (!plugin || !out_bgra || width == 0 || height == 0 || stride < width * 4) {
        return 0;
    }
    thread_local FrameBuffer temp_frame;
    if (!plugin->ReadFrame(temp_frame, timeout_ms) || !temp_frame.IsValid()) {
        return 0;
    }
    const uint32_t copy_w = std::min(width, temp_frame.Width());
    const uint32_t copy_h = std::min(height, temp_frame.Height());
    for (uint32_t y = 0; y < copy_h; ++y) {
        std::memcpy(
            out_bgra + static_cast<size_t>(y) * stride,
            temp_frame.Data() + static_cast<size_t>(y) * temp_frame.Stride(),
            static_cast<size_t>(copy_w) * 4);
    }
    return 1;
}

int32_t PhonePluginCSetParameter(void* instance, const char* key, const char* value) {
    auto* plugin = static_cast<PhoneCameraPlugin*>(instance);
    if (!plugin || !key || !value) {
        return 0;
    }
    if (std::strcmp(key, "endpoint_uri") == 0) {
        return plugin->SetEndpointUri(value) ? 1 : 0;
    }
    if (std::strcmp(key, "enabled") == 0) {
        plugin->SetEnabled(std::strcmp(value, "1") == 0 || std::strcmp(value, "true") == 0);
        return 1;
    }
    return 0;
}

void PhonePluginCClose(void* instance) {
    if (auto* plugin = static_cast<PhoneCameraPlugin*>(instance)) {
        plugin->Close();
    }
}

void PhonePluginCDestroy(void* instance) {
    delete static_cast<PhoneCameraPlugin*>(instance);
}

const SmartCamPluginDescriptorV1 kPhoneCameraDescriptorV1 = {
    SMARTCAM_PLUGIN_ABI_VERSION_V1,
    "phone_camera_source",
    "Fotocamera Smartphone (Wi-Fi / USB / IP Cam)",
    "1.0.0",
    "Rubens Rainelli",
    "Trasforma uno smartphone Android o iOS (IP Webcam, DroidCam, Iriun, HTTP MJPEG o USB Tethering) in sorgente video HD per SmartCam.",
    "camera_input",
    0u,  // Disabled by default; user toggles it on from the GUI or CLI
    &PhonePluginCInit,
    &PhonePluginCOpen,
    &PhonePluginCReadFrame,
    nullptr,  // camera_input plugin (does not run in-place filter)
    &PhonePluginCSetParameter,
    &PhonePluginCClose,
    &PhonePluginCDestroy
};

}  // namespace

const SmartCamPluginDescriptorV1* GetPhoneCameraPluginDescriptorV1() noexcept {
    return &kPhoneCameraDescriptorV1;
}

}  // namespace plugins
}  // namespace common
}  // namespace smartcam
