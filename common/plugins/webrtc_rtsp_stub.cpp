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

#include "iplugin_source.h"
#include "phone_camera_plugin.h"

namespace smartcam {
namespace common {
namespace plugins {

/**
 * @brief Reference implementation for the remote WebRTC / RTSP camera input plugin (`webrtc_rtsp_source`).
 *
 * Allows streaming video from remote WebRTC peers, RTSP IP cameras, or OBS RTSP/WHIP outputs
 * without modifying `common/engine/` or `platforms/`.
 */
class WebRtcRtspCameraStub final : public IPluginCameraSource {
public:
    WebRtcRtspCameraStub() = default;
    ~WebRtcRtspCameraStub() override = default;

    bool ConfigurePlugin(const PluginConnectionUri& uri_config) override {
        uri_config_ = uri_config;
        return true;
    }

    const char* GetPluginId() const noexcept override {
        return "webrtc_rtsp_source";
    }

    const char* GetPluginName() const noexcept override {
        return "SmartCam Remote WebRTC/RTSP Input Plugin [v1.0]";
    }

    void SetEnabled(bool enabled) noexcept override {
        enabled_ = enabled;
        if (!enabled_) {
            is_open_ = false;
        }
    }

    bool IsEnabled() const noexcept override {
        return enabled_;
    }

    bool Open(const CameraCaptureConfig& config) override {
        config_  = config;
        is_open_ = enabled_;
        return is_open_;
    }

    bool ReadFrame(FrameBuffer& /*out_frame*/, uint32_t /*timeout_ms*/) override {
        return false;
    }

    void Close() override {
        is_open_ = false;
    }

    bool IsOpen() const noexcept override {
        return is_open_;
    }

    CameraCaptureConfig GetActualConfig() const noexcept override {
        return config_;
    }

private:
    PluginConnectionUri uri_config_{};
    CameraCaptureConfig config_{};
    bool                enabled_ = false;
    bool                is_open_ = false;
};

const std::vector<PluginMetadata>& GetBuiltInPluginsCatalogue() {
    static const std::vector<PluginMetadata> kCatalogue = {
        PluginMetadata{
            "phone_camera_source",
            "📱 Fotocamera Smartphone (Wi-Fi / USB / IP Cam)",
            "1.0.0",
            "Rubens Rainelli",
            "Usa uno smartphone Android/iOS (IP Webcam, DroidCam, Iriun, HTTP MJPEG o USB Tethering) come webcam HD.",
            "camera_input",
            false,
            false,
            "http://192.168.1.100:8080/video"
        },
        PluginMetadata{
            "webrtc_rtsp_source",
            "🌐 Flusso Remoto WebRTC / RTSP (OBS / IP Cam)",
            "1.0.0",
            "Rubens Rainelli",
            "Riceve flussi video a bassa latenza da telecamere IP RTSP o peer WebRTC remoti sulla rete LAN.",
            "camera_input",
            false,
            false,
            "rtsp://192.168.1.120:8554/live"
        },
        PluginMetadata{
            "studio_color_enhancer",
            "🎨 Ottimizzatore Luce & Colore Studio (Auto-LUT & Denoise)",
            "1.0.0",
            "Rubens Rainelli",
            "Migliora automaticamente il contrasto, riduce il rumore in bassa luce e applica un tono caldo broadcast.",
            "video_filter",
            false,
            false,
            "warm_broadcast_lut"
        },
        PluginMetadata{
            "virtual_teleprompter_overlay",
            "📜 Teleprompter / Overlay Note in Sovrimpressione (Solo Anteprima o Stream)",
            "1.0.0",
            "Rubens Rainelli",
            "Mostra una fascia semitrasparente con indicatori di stato o note di regia per mantenere lo sguardo in camera.",
            "video_filter",
            false,
            false,
            "preview_only"
        }
    };
    return kCatalogue;
}

}  // namespace plugins
}  // namespace common
}  // namespace smartcam
