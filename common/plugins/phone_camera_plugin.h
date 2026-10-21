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

#ifndef SMARTCAM_COMMON_PLUGINS_PHONE_CAMERA_PLUGIN_H_
#define SMARTCAM_COMMON_PLUGINS_PHONE_CAMERA_PLUGIN_H_

#include <cstdint>
#include <mutex>
#include <string>
#include "iplugin_source.h"

namespace smartcam {
namespace common {
namespace plugins {

/**
 * @brief Connection state for the Smartphone Camera Plugin (`phone_camera_source`).
 */
enum class PhoneConnectionState : uint32_t {
    DISCONNECTED     = 0,  ///< Plugin disabled or closed
    PAIRING_STANDBY  = 1,  ///< Waiting for smartphone stream; renders HD Pairing Card Frame
    STREAMING_ACTIVE = 2   ///< Actively receiving BGRA32 video frames from the smartphone
};

/**
 * @brief Parsed network endpoint for Smartphone Wi-Fi / USB camera streaming.
 */
struct ParsedPhoneEndpoint {
    std::string scheme = "http";
    std::string host   = "192.168.1.100";
    uint16_t    port   = 8080;
    std::string path   = "/video";
    bool        is_usb = false;
};

/**
 * @brief Built-in plugin `phone_camera_source` ("📱 Fotocamera Smartphone Wi-Fi / USB").
 *
 * Allows using an Android or iOS smartphone (via IP Webcam, DroidCam, Iriun, RTSP server,
 * or USB Tethering / ADB port forwarding such as `http://192.168.1.100:8080/video`) as a
 * high-definition `BGRA32` video source for the SmartCam pipeline.
 *
 * Features:
 * - Automatic endpoint parsing (`http://`, `rtsp://`, `usb://` or `127.0.0.1` ADB tunnel).
 * - Built-in **Smartphone Pairing Card Frame Generator** (`BGRA32`, 64-byte SIMD aligned)
 *   displayed while waiting for the smartphone to connect, showing real-time visual telemetry.
 * - Thread-safe frame ingestion (`PushDecodedPhoneFrame`) for external or network decoders.
 * - Exposes a compliant `SmartCamPluginDescriptorV1` C ABI table for unified plugin management.
 */
class PhoneCameraPlugin final : public IPluginCameraSource {
public:
    PhoneCameraPlugin();
    ~PhoneCameraPlugin() override;

    // --- IPluginCameraSource ---
    bool ConfigurePlugin(const PluginConnectionUri& uri_config) override;
    const char* GetPluginId() const noexcept override;
    const char* GetPluginName() const noexcept override;
    void SetEnabled(bool enabled) noexcept override;
    bool IsEnabled() const noexcept override;

    // --- ICameraSource ---
    bool Open(const CameraCaptureConfig& config) override;
    bool ReadFrame(FrameBuffer& out_frame, uint32_t timeout_ms = 1000) override;
    void Close() override;
    bool IsOpen() const noexcept override;
    CameraCaptureConfig GetActualConfig() const noexcept override;

    // --- Smartphone Specific API ---
    /**
     * @brief Parses an endpoint URI (e.g. `"http://192.168.1.100:8080/video"`) into host, port, and path.
     */
    static ParsedPhoneEndpoint ParseEndpointUri(const std::string& uri);

    /**
     * @brief Updates the smartphone stream URL at runtime (e.g. from the SmartCam GUI).
     */
    bool SetEndpointUri(const std::string& endpoint_uri);

    /**
     * @brief Returns the currently configured smartphone endpoint URI.
     */
    std::string GetEndpointUri() const;

    /**
     * @brief Returns the current connection state (`DISCONNECTED`, `PAIRING_STANDBY`, `STREAMING_ACTIVE`).
     */
    PhoneConnectionState GetConnectionState() const noexcept;

    /**
     * @brief Pushes a decoded `BGRA32` smartphone frame into the plugin's double buffer.
     * Switches state to `STREAMING_ACTIVE`.
     */
    bool PushDecodedPhoneFrame(const uint8_t* bgra_data, uint32_t width, uint32_t height, uint32_t stride);

    /**
     * @brief Generates the interactive HD Smartphone Pairing / Test Card in `BGRA32` format
     * when waiting for the phone to connect over Wi-Fi or USB.
     */
    void RenderPairingCardFrame(FrameBuffer& out_frame);

private:
    mutable std::mutex   mutex_{};
    PluginConnectionUri  uri_config_{};
    ParsedPhoneEndpoint  parsed_endpoint_{};
    CameraCaptureConfig  config_{};
    FrameBuffer          latest_phone_frame_{};
    bool                 enabled_          = false;
    bool                 is_open_          = false;
    bool                 has_live_frame_   = false;
    PhoneConnectionState connection_state_ = PhoneConnectionState::DISCONNECTED;
    uint64_t             frame_counter_    = 0;
};

/**
 * @brief Returns the C ABI v1 plugin descriptor for the built-in `phone_camera_source` plugin.
 */
const SmartCamPluginDescriptorV1* GetPhoneCameraPluginDescriptorV1() noexcept;

}  // namespace plugins
}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_PLUGINS_PHONE_CAMERA_PLUGIN_H_
