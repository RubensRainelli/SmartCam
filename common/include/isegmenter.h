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

#ifndef SMARTCAM_COMMON_ISEGMENTER_H_
#define SMARTCAM_COMMON_ISEGMENTER_H_

#include <cstdint>
#include <string>
#include "frame_buffer.h"

namespace smartcam {
namespace common {

/**
 * @brief Supported background segmentation modes.
 */
enum class SegmentationMode : uint32_t {
    PASSTHROUGH  = 0,  ///< No segmentation (full foreground opacity = 255)
    GREEN_SCREEN = 1,  ///< Ultra-low CPU (<2%) HSV Chroma Key with morphological cleanup & soft-edge blur
    AI_SELFIE    = 2   ///< Local AI Background Removal (ONNX Runtime / MediaPipe Selfie Segmentation for iGPU/CPU)
};

/**
 * @brief HSV Chroma Key parameters for Mode 1 (Green Screen).
 * Hue is normalized to [0..180] (OpenCV convention), Saturation and Value to [0..255].
 */
struct GreenScreenConfig {
    uint8_t hue_min           = 35;    ///< Lower bound of green hue [0..180]
    uint8_t hue_max           = 85;    ///< Upper bound of green hue [0..180]
    uint8_t sat_min           = 40;    ///< Minimum saturation to classify as screen [0..255]
    uint8_t sat_max           = 255;   ///< Maximum saturation [0..255]
    uint8_t val_min           = 40;    ///< Minimum brightness value [0..255]
    uint8_t val_max           = 255;   ///< Maximum brightness value [0..255]
    uint8_t morph_kernel_size = 3;     ///< Morphological opening/closing kernel size (0, 3, or 5)
    uint8_t blur_radius       = 2;     ///< Box/Gaussian soft-edge feathering radius in pixels
    float   spill_suppression = 0.35f; ///< Green spill reduction strength on foreground boundary [0.0..1.0]
};

/**
 * @brief AI Background Removal parameters for Mode 2 (No Green Screen, Laptop iGPU / CPU).
 */
struct AiSegmenterConfig {
    std::string model_path          = "models/selfie_segmentation.onnx"; ///< Path to ONNX / TFLite model
    uint32_t    input_width         = 256;                               ///< Model inference input width
    uint32_t    input_height        = 144;                               ///< Model inference input height
    float       confidence_thresh   = 0.50f;                             ///< Foreground probability threshold [0.0..1.0]
    float       temporal_smooth     = 0.25f;                             ///< EMA temporal mask smoothing factor [0.0..0.9]
    uint32_t    intra_op_threads    = 2;                                 ///< CPU threads for inference when on CPU EP
    bool        prefer_igpu_dml     = true;                              ///< Use DirectML / OpenVINO on integrated GPU if available
    int         engine_mode         = 0;                                 ///< 0 = Auto/Hybrid, 1 = Face/Silhouette Tracker Only, 2 = Reference Background Difference Priority
    float       bg_diff_tolerance   = 0.50f;                             ///< Spatial & chromatic tolerance for Reference Background Subtraction [0.0..1.0]
    int         silhouette_mode     = 0;                                 ///< 0 = Volto + Busto (Face + Upper Body), 1 = Solo Volto (Face Only)
    bool        multi_person_enabled = false;                            ///< false = Single primary user, true = Multi-person tracking (up to 4 people)
    bool        face_calibrated     = false;                             ///< True when 5-point guided facial calibration profile is active
    float       calib_skin_y        = 135.0f;                            ///< Calibrated user facial skin luminance Y
    float       calib_skin_cb       = 113.0f;                            ///< Calibrated user facial skin chrominance Cb
    float       calib_skin_cr       = 148.0f;                            ///< Calibrated user facial skin chrominance Cr
    float       calib_skin_cb_tol   = 15.0f;                             ///< Adaptive Cb tolerance for calibrated face
    float       calib_skin_cr_tol   = 17.0f;                             ///< Adaptive Cr tolerance for calibrated face
    float       calib_head_rx       = 0.135f;                            ///< Calibrated normalized horizontal head semi-axis
    float       calib_head_ry       = 0.250f;                            ///< Calibrated normalized vertical head semi-axis
    float       calib_chin_rel_y    = 0.94f;                             ///< Relative ratio between head center and chin tip
    float       calib_anchor_cx     = 0.485f;                            ///< Calibrated geometric head center X from 5 landmarks
    float       calib_anchor_cy     = 0.490f;                            ///< Calibrated geometric head center Y from 5 landmarks
    float       calib_landmarks[10] = {                                  ///< 5 calibrated landmarks [rx,ry, lx,ly, nx,ny, mx,my, cx,cy]
        0.538f, 0.469f, 0.426f, 0.459f, 0.484f, 0.524f, 0.484f, 0.627f, 0.491f, 0.728f
    };
};

/**
 * @brief Background replacement mode used by the compositor.
 */
enum class BackgroundMode : uint32_t {
    SOLID_COLOR = 0,  ///< Replace background with a solid RGB color
    IMAGE_OR_VIDEO = 1, ///< Replace background with a custom static image or video frame
    BLUR        = 2   ///< Blur the original background (Bokeh effect)
};

/**
 * @brief Abstract interface for foreground/background segmentation modules.
 *
 * All implementations must be 100% OS-independent and operate on `FrameView` structures.
 */
class ISegmenter {
public:
    virtual ~ISegmenter() = default;

    /**
     * @brief Computes a single-channel 8-bit foreground opacity mask (`PixelFormat::GRAY8`).
     *
     * Mask semantics:
     *   - 255 = 100% Foreground (person)
     *   -   0 = 100% Background (replaced)
     *   - 1..254 = Soft-edge semi-transparent boundary for smooth alpha blending
     *
     * @param input_frame Input video frame (`RGBA32` or `BGRA32`).
     * @param out_mask    Pre-allocated output mask (`GRAY8`) with identical width & height.
     * @return true on success, false on invalid dimensions or internal error.
     */
    virtual bool ComputeMask(const FrameView& input_frame, FrameView& out_mask) = 0;

    /**
     * @brief Returns the active segmentation mode.
     */
    virtual SegmentationMode GetMode() const noexcept = 0;

    /**
     * @brief Returns the implementation name (e.g., "HSV GreenScreen Segmenter", "ONNX Selfie Segmenter").
     */
    virtual const char* GetName() const noexcept = 0;
};

/**
 * @brief Abstract interface for SIMD-accelerated frame compositing (Alpha Blending).
 */
class ICompositor {
public:
    virtual ~ICompositor() = default;

    /**
     * @brief Alpha-blends `foreground` over `background` using `alpha_mask` (`GRAY8`) into `output`.
     *
     * Formula per pixel channel $c \in \{R, G, B\}$:
     * $$O_c = \frac{F_c \cdot \alpha + B_c \cdot (255 - \alpha)}{255}$$
     *
     * Implementations automatically dispatch to AVX-512, AVX2, or scalar fallback at runtime.
     */
    virtual bool Blend(const FrameView& foreground,
                       const FrameView& background,
                       const FrameView& alpha_mask,
                       FrameView& output) = 0;

    /**
     * @brief Returns the active SIMD instruction set name ("AVX-512", "AVX2", or "Scalar").
     */
    virtual const char* GetActiveSimdBackend() const noexcept = 0;
};

}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ISEGMENTER_H_
