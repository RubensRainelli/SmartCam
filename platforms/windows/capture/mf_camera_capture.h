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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_CAPTURE_MF_CAMERA_CAPTURE_H_
#define SMARTCAM_PLATFORMS_WINDOWS_CAPTURE_MF_CAMERA_CAPTURE_H_

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <wrl/client.h>
#include <string>
#include <vector>

#include "../../../common/include/icamera_source.h"

namespace smartcam {
namespace platforms {
namespace windows {

/**
 * @brief Metadata for an enumerated Windows Media Foundation hardware video capture device.
 */
struct MfCameraDeviceInfo {
    uint32_t    index         = 0;
    std::string friendly_name = "";
    std::string symbolic_link = "";
};

/**
 * @brief Native Windows 11 Media Foundation hardware webcam capture (`IMFSourceReader`).
 *
 * Supports device selection by index or symbolic link (`device_id`), target resolution
 * (`width`, `height`), and target framerate (`target_fps`), decoding into `BGRA32` (`MFVideoFormat_RGB32`).
 */
class MfCameraCapture final : public common::ICameraSource {
public:
    MfCameraCapture() noexcept = default;
    ~MfCameraCapture() override { Close(); }

    /**
     * @brief Lists all hardware video capture devices connected to the Windows system.
     */
    static std::vector<MfCameraDeviceInfo> EnumerateDevices();

    bool Open(const common::CameraCaptureConfig& config) override;
    bool ReadFrame(common::FrameBuffer& out_frame, uint32_t timeout_ms = 1000) override;
    void Close() override;

    bool IsOpen() const noexcept override { return is_open_; }
    common::CameraCaptureConfig GetActualConfig() const noexcept override { return actual_config_; }

private:
    Microsoft::WRL::ComPtr<IMFSourceReader> reader_;
    common::CameraCaptureConfig             actual_config_{};
    uint64_t                                frame_counter_ = 0;
    bool                                    mf_started_    = false;
    bool                                    is_open_       = false;
};

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_CAPTURE_MF_CAMERA_CAPTURE_H_
