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

#ifndef SMARTCAM_COMMON_ENGINE_COMPOSITOR_H_
#define SMARTCAM_COMMON_ENGINE_COMPOSITOR_H_

#include <cstdint>
#include "../../include/frame_buffer.h"
#include "../../include/isegmenter.h"

namespace smartcam {
namespace common {
namespace engine {

/**
 * @brief SIMD instruction set tier used by `SimdCompositor`.
 */
enum class SimdTier : uint32_t {
    AUTO   = 0,  ///< Automatically select best available CPU instruction set at runtime
    SCALAR = 1,  ///< Portable C++ scalar loop
    AVX2   = 2,  ///< 256-bit AVX2 vectorization (8 RGBA/BGRA pixels per cycle)
    AVX512 = 3   ///< 512-bit AVX-512F/BW vectorization (16 RGBA/BGRA pixels per cycle)
};

/**
 * @brief High-throughput Alpha Compositor with runtime CPUID dispatch for AVX-512 and AVX2.
 *
 * Strictly platform-agnostic (zero OS-specific headers). Designed to maximize throughput
 * on modern laptop CPUs with integrated graphics.
 */
class SimdCompositor final : public ICompositor {
public:
    explicit SimdCompositor(SimdTier requested_tier = SimdTier::AUTO) noexcept;
    ~SimdCompositor() override = default;

    /**
     * @brief Queries CPUID + XGETBV to detect hardware & OS support for AVX2 and AVX-512BW.
     */
    static SimdTier DetectBestSimdTier() noexcept;

    /**
     * @brief Overrides the active SIMD tier (automatically clamped to hardware capability).
     */
    void SetSimdTier(SimdTier requested_tier) noexcept;

    /**
     * @brief Returns currently active SIMD tier enum.
     */
    SimdTier GetSimdTier() const noexcept { return active_tier_; }

    /**
     * @brief Returns human-readable active SIMD backend name ("AVX-512", "AVX2", or "Scalar").
     */
    const char* GetActiveSimdBackend() const noexcept override;

    /**
     * @brief Alpha-blends `foreground` over `background` using `alpha_mask` (`GRAY8`) into `output`.
     */
    bool Blend(const FrameView& foreground,
               const FrameView& background,
               const FrameView& alpha_mask,
               FrameView& output) override;

    /**
     * @brief Fills a `BGRA32` or `RGBA32` frame buffer with a solid color.
     */
    static bool FillSolidBackground(FrameView& bg_frame, uint8_t r, uint8_t g, uint8_t b) noexcept;

    /**
     * @brief Generates a fast separable box-blurred background (Bokeh effect) from `src` into `dst_bg`.
     */
    static bool GenerateBlurredBackground(const FrameView& src, FrameView& dst_bg, uint32_t radius = 12) noexcept;

private:
    static void BlendRowScalar(const uint8_t* fg_row,
                               const uint8_t* bg_row,
                               const uint8_t* mask_row,
                               uint8_t* out_row,
                               uint32_t width) noexcept;

    static void BlendRowAVX2(const uint8_t* fg_row,
                             const uint8_t* bg_row,
                             const uint8_t* mask_row,
                             uint8_t* out_row,
                             uint32_t width) noexcept;

    static void BlendRowAVX512(const uint8_t* fg_row,
                               const uint8_t* bg_row,
                               const uint8_t* mask_row,
                               uint8_t* out_row,
                               uint32_t width) noexcept;

    SimdTier active_tier_ = SimdTier::SCALAR;
};

}  // namespace engine
}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ENGINE_COMPOSITOR_H_
