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

#ifndef SMARTCAM_COMMON_ENGINE_CROP_SCALER_H_
#define SMARTCAM_COMMON_ENGINE_CROP_SCALER_H_

#include <cstdint>
#include "../../include/frame_buffer.h"

namespace smartcam {
namespace common {
namespace engine {

/**
 * @brief Axis-aligned rectangle describing a crop region inside a frame.
 */
struct CropRect {
    uint32_t x      = 0;
    uint32_t y      = 0;
    uint32_t width  = 0;
    uint32_t height = 0;
};

/**
 * @brief High-performance, distortion-free Aspect Ratio Crop & Scaler.
 *
 * Strictly platform-agnostic (zero OS headers). Uses 16.16 fixed-point bilinear interpolation
 * and fast scanline memcpy when target dimensions match the cropped region (e.g., 1920x1080 -> 1440x1080 4:3).
 */
class CropScaler {
public:
    CropScaler() noexcept = default;
    ~CropScaler() noexcept = default;

    /**
     * @brief Computes the largest centered crop rectangle inside `(src_width, src_height)`
     *        that matches the requested aspect ratio `aspect_w : aspect_h` without geometric distortion.
     */
    static CropRect ComputeCenterCropRect(uint32_t src_width,
                                          uint32_t src_height,
                                          uint32_t aspect_w,
                                          uint32_t aspect_h) noexcept;

    /**
     * @brief Resolves standard `AspectRatioMode` enum into numerator/denominator pair.
     */
    static void ResolveAspectRatio(AspectRatioMode mode,
                                   uint32_t src_width,
                                   uint32_t src_height,
                                   uint32_t custom_w,
                                   uint32_t custom_h,
                                   uint32_t& out_aspect_w,
                                   uint32_t& out_aspect_h) noexcept;

    /**
     * @brief Transforms `src` into `dst` applying either `CENTER_CROP` or `LETTERBOX` strategy.
     *
     * Supports `RGBA32`, `BGRA32`, and `GRAY8` formats.
     * If the center-cropped region has the exact same resolution as `dst` (e.g. 1920x1080 -> 1440x1080),
     * performs a zero-interpolation direct row copy for near-zero CPU overhead.
     */
    static bool Process(const FrameView& src,
                        FrameView& dst,
                        AspectRatioMode aspect_mode = AspectRatioMode::RATIO_4_3,
                        CropStrategy strategy = CropStrategy::CENTER_CROP,
                        uint32_t custom_aspect_w = 4,
                        uint32_t custom_aspect_h = 3) noexcept;

    /**
     * @brief Resizes a single-channel `GRAY8` mask or 4-channel `RGBA32`/`BGRA32` ROI using 16.16 fixed-point bilinear filtering.
     */
    static bool ResizeBilinearROI(const FrameView& src,
                                  const CropRect& src_roi,
                                  FrameView& dst,
                                  const CropRect& dst_roi) noexcept;
};

}  // namespace engine
}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ENGINE_CROP_SCALER_H_
