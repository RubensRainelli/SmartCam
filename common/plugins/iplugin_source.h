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

#ifndef SMARTCAM_COMMON_PLUGINS_IPLUGIN_SOURCE_H_
#define SMARTCAM_COMMON_PLUGINS_IPLUGIN_SOURCE_H_

#include <cstdint>
#include <string>
#include <vector>
#include "../include/icamera_source.h"

/**
 * @brief Universal C ABI version for external SmartCam dynamic plugins (`.dll` / `.so`).
 */
#define SMARTCAM_PLUGIN_ABI_VERSION_V1 1u

/**
 * @brief Exported symbol name that SmartCam's PluginRegistry queries via `GetProcAddress` / `dlsym`.
 */
#define SMARTCAM_PLUGIN_QUERY_SYMBOL "smartcam_plugin_query_v1"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Universal C ABI descriptor v1 exported by external `.dll` / `.so` plugins in `plugins/`.
 *
 * Any C, C++, or Rust (`cdylib`) shared library placed in the `plugins/` folder can expose
 * `extern "C" const SmartCamPluginDescriptorV1* smartcam_plugin_query_v1(void)` to be
 * automatically discovered, listed in the SmartCam UI, and toggled on/off by the user.
 *
 * Memory, Architecture & Bit-Depth Contract:
 * - Native 64-bit (`x86_64`) ABI across Windows 11 (`PE32+`) and Linux (`ELF64`).
 * - Supports both 32-bit SDR (`PixelFormat::BGRA32`, 4 bytes/pixel: 8-bit B, G, R, A) and
 *   64-bit Deep Color (`PixelFormat::BGRA64`, 8 bytes/pixel: 16-bit B16, G16, R16, A16; indicated when `stride == width * 8`).
 * - Buffers passed by SmartCam's engine are 64-byte aligned (`FrameBuffer::kSimdAlignment = 64`)
 *   for zero-copy AVX2 / AVX-512 SIMD execution.
 */
typedef struct SmartCamPluginDescriptorV1 {
    uint32_t    abi_version;         ///< Must be `SMARTCAM_PLUGIN_ABI_VERSION_V1` (1)
    const char* id;                  ///< Unique snake_case identifier (e.g., `"phone_camera_source"`)
    const char* name;                ///< Human-readable title displayed in the UI
    const char* version;             ///< Semantic version string (e.g., `"1.0.0"`)
    const char* author;              ///< Plugin author / organization
    const char* description;         ///< Brief explanation of plugin capabilities
    const char* category;            ///< `"camera_input"`, `"video_filter"`, or `"segmentation"`
    uint32_t    enabled_by_default;  ///< `1` if active by default, `0` if opt-in by user

    /**
     * @brief Allocates and initializes a new plugin instance context (may return nullptr for stateless plugins).
     */
    void* (*init)(void);

    /**
     * @brief Opens a camera stream or prepares filter resources for the target resolution/FPS.
     * @return 1 on success, 0 on failure.
     */
    int32_t (*open)(
        void*       instance,
        uint32_t    width,
        uint32_t    height,
        uint32_t    fps,
        const char* endpoint_uri);

    /**
     * @brief [For `"camera_input"` plugins] Acquires the next decoded `BGRA32` frame.
     * @param out_bgra 64-byte aligned destination buffer of size `stride * height`.
     * @return 1 if a valid frame was written, 0 on timeout/disconnect.
     */
    int32_t (*read_frame)(
        void*    instance,
        uint8_t* out_bgra,
        uint32_t width,
        uint32_t height,
        uint32_t stride,
        uint32_t timeout_ms);

    /**
     * @brief [For `"video_filter"` / `"segmentation"` plugins] Modifies the live frame or alpha mask in-place.
     * @param inout_bgra 64-byte aligned `BGRA32` frame buffer (`stride * height` bytes).
     * @param inout_mask_gray8 Optional 64-byte aligned `GRAY8` alpha mask (`width * height` bytes, may be nullptr).
     * @return 1 if processed, 0 if skipped.
     */
    int32_t (*process_frame)(
        void*    instance,
        uint8_t* inout_bgra,
        uint8_t* inout_mask_gray8,
        uint32_t width,
        uint32_t height,
        uint32_t stride);

    /**
     * @brief Configures a runtime key/value parameter (e.g., `"endpoint_uri"`, `"strength"`, `"overlay_text"`).
     * @return 1 if accepted, 0 if unknown key.
     */
    int32_t (*set_parameter)(
        void*       instance,
        const char* key,
        const char* value);

    /**
     * @brief Stops active streaming or releases session resources.
     */
    void (*close)(void* instance);

    /**
     * @brief Destroys the plugin instance created by `init()`.
     */
    void (*destroy)(void* instance);
} SmartCamPluginDescriptorV1;

/**
 * @brief Function pointer signature for dynamic plugin discovery (`smartcam_plugin_query_v1`).
 */
typedef const SmartCamPluginDescriptorV1* (*SmartCamPluginQueryFn)(void);

#ifdef __cplusplus
}  // extern "C"
#endif

namespace smartcam {
namespace common {
namespace plugins {

/**
 * @brief Plugin functional category within the SmartCam video pipeline.
 */
enum class PluginCategory : uint32_t {
    CAMERA_INPUT = 0,  ///< Hardware, smartphone, or network camera frame producer (`"camera_input"`)
    VIDEO_FILTER = 1,  ///< In-place color grading, LUT, denoise, or HUD overlay (`"video_filter"`)
    SEGMENTATION = 2   ///< Custom foreground/background alpha matte generator (`"segmentation"`)
};

/**
 * @brief Plugin transport protocol descriptor for remote/smartphone/networked camera inputs.
 */
enum class PluginProtocol : uint32_t {
    WEBRTC_PEER      = 0,  ///< Low-latency WebRTC video track receiver (smartphone / remote browser camera)
    RTSP_STREAM      = 1,  ///< RTSP / RTP network IP camera stream (`rtsp://...`)
    NDI_STREAM       = 2,  ///< Network Device Interface LAN stream
    CUSTOM_IPC       = 3,  ///< Custom shared-memory or socket plugin
    HTTP_MJPEG_PHONE = 4,  ///< Smartphone Wi-Fi HTTP MJPEG / JPEG snapshot stream (IP Webcam, DroidCam, Iriun)
    USB_ADB_TETHER   = 5   ///< Smartphone USB Tethering / ADB port-forwarded local stream
};

/**
 * @brief Metadata structure describing a built-in or external SmartCam plugin.
 */
struct PluginMetadata {
    std::string    id;
    std::string    name;
    std::string    version;
    std::string    author;
    std::string    description;
    std::string    category;            ///< `"camera_input"`, `"video_filter"`, or `"segmentation"`
    bool           enabled_by_default = false;
    bool           enabled            = false;
    std::string    default_endpoint;
};

/**
 * @brief Connection descriptor for remote and smartphone plugin camera sources.
 */
struct PluginConnectionUri {
    PluginProtocol protocol       = PluginProtocol::HTTP_MJPEG_PHONE;
    std::string    endpoint_uri   = "http://192.168.1.100:8080/video";
    std::string    auth_token     = "";
    uint32_t       jitter_buf_ms  = 25;
    bool           hardware_accel = true;
};

/**
 * @brief Extensible C++ plugin contract for remote or smartphone camera sources.
 *
 * Inherits from `ICameraSource` so any plugin source can be hot-swapped into the
 * SmartCam pipeline transparently without modifying the segmentation or virtual camera sink layers.
 */
class IPluginCameraSource : public ICameraSource {
public:
    ~IPluginCameraSource() override = default;

    /**
     * @brief Configures network/smartphone signaling parameters prior to calling `Open()`.
     */
    virtual bool ConfigurePlugin(const PluginConnectionUri& uri_config) = 0;

    /**
     * @brief Returns the unique plugin ID (e.g. `"phone_camera_source"`).
     */
    virtual const char* GetPluginId() const noexcept = 0;

    /**
     * @brief Returns plugin display name and version string.
     */
    virtual const char* GetPluginName() const noexcept = 0;

    /**
     * @brief Enables or disables the plugin at runtime according to user selection.
     */
    virtual void SetEnabled(bool enabled) noexcept = 0;

    /**
     * @brief Returns true if the user has enabled this plugin.
     */
    virtual bool IsEnabled() const noexcept = 0;
};

/**
 * @brief Returns the canonical metadata list for the 4 built-in SmartCam plugins:
 *   1. `phone_camera_source`          — 📱 Fotocamera Smartphone (Wi-Fi / USB / IP Cam)
 *   2. `webrtc_rtsp_source`           — 🌐 Flusso Remoto WebRTC / RTSP (OBS / IP Cam)
 *   3. `studio_color_enhancer`        — 🎨 Ottimizzatore Luce & Colore Studio (Auto-LUT & Denoise)
 *   4. `virtual_teleprompter_overlay` — 📜 Teleprompter / Overlay Note in Sovrimpressione (Solo Anteprima o Stream)
 */
const std::vector<PluginMetadata>& GetBuiltInPluginsCatalogue();

}  // namespace plugins
}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_PLUGINS_IPLUGIN_SOURCE_H_
