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

#ifndef SMARTCAM_COMMON_ENGINE_GREEN_SCREEN_SEGMENTER_H_
#define SMARTCAM_COMMON_ENGINE_GREEN_SCREEN_SEGMENTER_H_

#include <cstdint>
#include <vector>
#include "../../include/frame_buffer.h"
#include "../../include/isegmenter.h"

namespace smartcam {
namespace common {
namespace engine {

/**
 * @brief Mode 1: Ultra-low CPU (<2%) HSV Chroma Key (Green Screen) Segmenter.
 *
 * Pipeline stages:
 *   1. Integer branch-minimized RGB/BGR -> HSV conversion (Hue in [0..180], Sat/Val in [0..255]).
 *   2. Min/Max HSV thresholding with smooth transition ramp at boundary tolerances.
 *   3. Separable morphological Opening (Erosion + Dilation) to eliminate chroma noise speckles.
 *   4. Separable O(1)-per-pixel 2-pass Box/Gaussian approximation blur for feathered soft edges.
 *   5. Optional in-place Green Spill Suppression on semi-transparent boundary pixels.
 */
class GreenScreenSegmenter final : public ISegmenter {
public:
    explicit GreenScreenSegmenter(const GreenScreenConfig& config = GreenScreenConfig{}) noexcept;
    ~GreenScreenSegmenter() override = default;

    void UpdateConfig(const GreenScreenConfig& config) noexcept { config_ = config; }
    const GreenScreenConfig& GetConfig() const noexcept { return config_; }

    /**
     * @brief Computes foreground opacity mask (`out_mask`, `GRAY8`) from `input_frame` (`RGBA32`/`BGRA32`).
     */
    bool ComputeMask(const FrameView& input_frame, FrameView& out_mask) override;

    /**
     * @brief Suppresses green screen color spill on foreground edge pixels in-place.
     */
    void SuppressGreenSpill(FrameView& frame, const FrameView& mask) const noexcept;

    /**
     * @brief Auto-calibrates HSV Green Screen thresholds by sampling either a patch around
     *        `(sample_x, sample_y)` (when `sample_x >= 0 && sample_y >= 0`) or the upper-left
     *        and upper-right background corner regions (when `sample_x < 0`).
     *        Automatically updates `config_` and writes computed thresholds to optional outputs.
     */
    bool AutoCalibrate(const FrameView& input_frame,
                       int32_t sample_x,
                       int32_t sample_y,
                       uint8_t* out_hue_min = nullptr,
                       uint8_t* out_hue_max = nullptr,
                       uint8_t* out_sat_min = nullptr,
                       uint8_t* out_val_min = nullptr) noexcept;

    SegmentationMode GetMode() const noexcept override { return SegmentationMode::GREEN_SCREEN; }
    const char* GetName() const noexcept override { return "HSV GreenScreen Segmenter (Integer FastPath)"; }

    /**
     * @brief Converts a single RGB triplet to OpenCV-compatible HSV (`H in [0..180]`, `S, V in [0..255]`).
     */
    static inline void RgbToHsvFast(uint8_t r, uint8_t g, uint8_t b,
                                    uint8_t& out_h, uint8_t& out_s, uint8_t& out_v) noexcept {
        const uint8_t cmax  = (r > g) ? ((r > b) ? r : b) : ((g > b) ? g : b);
        const uint8_t cmin  = (r < g) ? ((r < b) ? r : b) : ((g < b) ? g : b);
        const uint8_t delta = cmax - cmin;

        out_v = cmax;
        if (cmax == 0 || delta == 0) {
            out_s = 0;
            out_h = 0;
            return;
        }

        out_s = static_cast<uint8_t>((static_cast<uint32_t>(delta) * 255u) / cmax);

        int32_t hue = 0;
        if (cmax == r) {
            hue = (30 * (static_cast<int32_t>(g) - static_cast<int32_t>(b))) / static_cast<int32_t>(delta);
            if (hue < 0) hue += 180;
        } else if (cmax == g) {
            hue = 60 + (30 * (static_cast<int32_t>(b) - static_cast<int32_t>(r))) / static_cast<int32_t>(delta);
        } else {
            hue = 120 + (30 * (static_cast<int32_t>(r) - static_cast<int32_t>(g))) / static_cast<int32_t>(delta);
        }

        out_h = static_cast<uint8_t>(hue < 0 ? 0 : (hue > 180 ? 180 : hue));
    }

private:
    void MorphologicalOpen(uint8_t* mask, uint32_t width, uint32_t height, uint32_t stride, uint8_t radius);
    void SeparableSoftBlur(uint8_t* mask, uint32_t width, uint32_t height, uint32_t stride, uint8_t radius);

    GreenScreenConfig    config_{};
    std::vector<uint8_t> scratch_a_;
    std::vector<uint8_t> scratch_b_;
};

}  // namespace engine
}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ENGINE_GREEN_SCREEN_SEGMENTER_H_
