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

#include "compositor.h"
#include <algorithm>
#include <cstring>
#include <vector>

#ifndef SMARTCAM_ENABLE_AVX512_BUILD
    #define SMARTCAM_ENABLE_AVX512_BUILD 0
#endif

#if defined(__x86_64__) || defined(_M_X64) || defined(__i386__) || defined(_M_IX86)
    #define SMARTCAM_X86_SIMD 1
    #include <immintrin.h>
    #if defined(_MSC_VER)
        #include <intrin.h>
    #elif defined(__GNUC__) || defined(__clang__)
        #include <cpuid.h>
    #endif
#else
    #define SMARTCAM_X86_SIMD 0
#endif

namespace smartcam {
namespace common {
namespace engine {

namespace {

#if SMARTCAM_X86_SIMD
void QueryCpuid(int leaf, int subleaf, int regs[4]) noexcept {
#if defined(_MSC_VER)
    __cpuidex(regs, leaf, subleaf);
#elif defined(__GNUC__) || defined(__clang__)
    __cpuid_count(leaf, subleaf, regs[0], regs[1], regs[2], regs[3]);
#else
    regs[0] = regs[1] = regs[2] = regs[3] = 0;
#endif
}

uint64_t QueryXcr0() noexcept {
#if defined(_MSC_VER)
    return _xgetbv(0);
#elif defined(__GNUC__) || defined(__clang__)
    uint32_t eax = 0, edx = 0;
    __asm__ __volatile__("xgetbv" : "=a"(eax), "=d"(edx) : "c"(0));
    return (static_cast<uint64_t>(edx) << 32) | eax;
#else
    return 0;
#endif
}
#endif

}  // namespace

SimdTier SimdCompositor::DetectBestSimdTier() noexcept {
#if SMARTCAM_X86_SIMD
    int regs[4] = {0, 0, 0, 0};
    QueryCpuid(0, 0, regs);
    const int max_leaf = regs[0];
    if (max_leaf < 7) {
        return SimdTier::SCALAR;
    }

    QueryCpuid(1, 0, regs);
    const bool osxsave = (regs[2] & (1 << 27)) != 0;
    const bool avx     = (regs[2] & (1 << 28)) != 0;
    if (!osxsave || !avx) {
        return SimdTier::SCALAR;
    }

    const uint64_t xcr0 = QueryXcr0();
    const bool ymm_enabled = (xcr0 & 0x6ULL) == 0x6ULL;
    if (!ymm_enabled) {
        return SimdTier::SCALAR;
    }

    QueryCpuid(7, 0, regs);
    const bool has_avx2 = (regs[1] & (1 << 5)) != 0;
#if SMARTCAM_ENABLE_AVX512_BUILD
    const bool has_avx512f  = (regs[1] & (1 << 16)) != 0;
    const bool has_avx512bw = (regs[1] & (1 << 30)) != 0;
    const bool zmm_enabled  = (xcr0 & 0xE6ULL) == 0xE6ULL;

    if (has_avx512f && has_avx512bw && zmm_enabled) {
        return SimdTier::AVX512;
    }
#endif
    if (has_avx2) {
        return SimdTier::AVX2;
    }
#endif
    return SimdTier::SCALAR;
}

SimdCompositor::SimdCompositor(SimdTier requested_tier) noexcept {
    SetSimdTier(requested_tier);
}

void SimdCompositor::SetSimdTier(SimdTier requested_tier) noexcept {
    const SimdTier max_supported = DetectBestSimdTier();
    if (requested_tier == SimdTier::AUTO) {
        active_tier_ = max_supported;
    } else if (static_cast<uint32_t>(requested_tier) > static_cast<uint32_t>(max_supported)) {
        active_tier_ = max_supported;
    } else {
        active_tier_ = requested_tier;
    }
}

const char* SimdCompositor::GetActiveSimdBackend() const noexcept {
    switch (active_tier_) {
        case SimdTier::AVX512:
            return "AVX-512";
        case SimdTier::AVX2:
            return "AVX2";
        case SimdTier::SCALAR:
        default:
            return "Scalar";
    }
}

void SimdCompositor::BlendRowScalar(const uint8_t* fg_row,
                                    const uint8_t* bg_row,
                                    const uint8_t* mask_row,
                                    uint8_t* out_row,
                                    uint32_t width) noexcept {
    for (uint32_t x = 0; x < width; ++x) {
        // Map [0..255] alpha to [0..256] for exact bit-shift division: a_scale = a + (a >> 7)
        const uint32_t a     = mask_row[x];
        const uint32_t alpha = a + (a >> 7);
        const uint32_t inv_a = 256u - alpha;

        const size_t idx = static_cast<size_t>(x) * 4;
        out_row[idx + 0] = static_cast<uint8_t>((fg_row[idx + 0] * alpha + bg_row[idx + 0] * inv_a) >> 8);
        out_row[idx + 1] = static_cast<uint8_t>((fg_row[idx + 1] * alpha + bg_row[idx + 1] * inv_a) >> 8);
        out_row[idx + 2] = static_cast<uint8_t>((fg_row[idx + 2] * alpha + bg_row[idx + 2] * inv_a) >> 8);
        out_row[idx + 3] = 255;
    }
}

void SimdCompositor::BlendRowAVX2(const uint8_t* fg_row,
                                  const uint8_t* bg_row,
                                  const uint8_t* mask_row,
                                  uint8_t* out_row,
                                  uint32_t width) noexcept {
    uint32_t x = 0;
#if SMARTCAM_X86_SIMD
    const __m256i v_256       = _mm256_set1_epi16(256);
    const __m256i v_alpha_out = _mm256_set1_epi32(static_cast<int>(0xFF000000u));

    // Process 8 pixels (32 bytes of RGBA/BGRA) per iteration
    for (; x + 8 <= width; x += 8) {
        const size_t px_offset = static_cast<size_t>(x) * 4;

        // Load 8 pixels of foreground and background (32 bytes each)
        const __m256i fg_u8 = _mm256_loadu_si256(reinterpret_cast<const __m256i*>(fg_row + px_offset));
        const __m256i bg_u8 = _mm256_loadu_si256(reinterpret_cast<const __m256i*>(bg_row + px_offset));

        // Load 8 bytes of alpha mask and broadcast each alpha byte to 4 channels [a, a, a, a]
        int64_t mask8 = 0;
        std::memcpy(&mask8, mask_row + x, sizeof(int64_t));
        const __m128i m_raw = _mm_cvtsi64_si128(mask8);
        // Unpack each byte to 32-bit integer then replicate across 4 bytes
        const __m256i m_32  = _mm256_cvtepu8_epi32(m_raw);
        const __m256i a_u8  = _mm256_mullo_epi32(m_32, _mm256_set1_epi32(0x01010101));

        // Unpack low and high 128-bit lanes into 16-bit integers for precision blending
        const __m256i zero   = _mm256_setzero_si256();
        const __m256i fg_lo  = _mm256_unpacklo_epi8(fg_u8, zero);
        const __m256i fg_hi  = _mm256_unpackhi_epi8(fg_u8, zero);
        const __m256i bg_lo  = _mm256_unpacklo_epi8(bg_u8, zero);
        const __m256i bg_hi  = _mm256_unpackhi_epi8(bg_u8, zero);
        const __m256i a_lo_r = _mm256_unpacklo_epi8(a_u8, zero);
        const __m256i a_hi_r = _mm256_unpackhi_epi8(a_u8, zero);

        // Scale alpha from [0..255] to [0..256]: a_scaled = a + (a >> 7)
        const __m256i a_lo     = _mm256_add_epi16(a_lo_r, _mm256_srli_epi16(a_lo_r, 7));
        const __m256i a_hi     = _mm256_add_epi16(a_hi_r, _mm256_srli_epi16(a_hi_r, 7));
        const __m256i inv_a_lo = _mm256_sub_epi16(v_256, a_lo);
        const __m256i inv_a_hi = _mm256_sub_epi16(v_256, a_hi);

        // (fg * alpha + bg * inv_alpha) >> 8
        const __m256i res_lo = _mm256_srli_epi16(
            _mm256_add_epi16(_mm256_mullo_epi16(fg_lo, a_lo), _mm256_mullo_epi16(bg_lo, inv_a_lo)), 8);
        const __m256i res_hi = _mm256_srli_epi16(
            _mm256_add_epi16(_mm256_mullo_epi16(fg_hi, a_hi), _mm256_mullo_epi16(bg_hi, inv_a_hi)), 8);

        // Pack 16-bit channels back to 8-bit and force alpha channel to 255
        const __m256i packed = _mm256_or_si256(_mm256_packus_epi16(res_lo, res_hi), v_alpha_out);
        _mm256_storeu_si256(reinterpret_cast<__m256i*>(out_row + px_offset), packed);
    }
#endif
    if (x < width) {
        BlendRowScalar(fg_row + static_cast<size_t>(x) * 4,
                       bg_row + static_cast<size_t>(x) * 4,
                       mask_row + x,
                       out_row + static_cast<size_t>(x) * 4,
                       width - x);
    }
}

#if SMARTCAM_X86_SIMD && SMARTCAM_ENABLE_AVX512_BUILD && (defined(__GNUC__) || defined(__clang__))
__attribute__((target("avx512f,avx512bw")))
#endif
void SimdCompositor::BlendRowAVX512(const uint8_t* fg_row,
                                    const uint8_t* bg_row,
                                    const uint8_t* mask_row,
                                    uint8_t* out_row,
                                    uint32_t width) noexcept {
    uint32_t x = 0;
#if SMARTCAM_X86_SIMD && SMARTCAM_ENABLE_AVX512_BUILD
    const __m512i v_256       = _mm512_set1_epi16(256);
    const __m512i v_alpha_out = _mm512_set1_epi32(static_cast<int>(0xFF000000u));
    const __m512i zero        = _mm512_setzero_si512();

    // Process 16 pixels (64 bytes of RGBA/BGRA) per AVX-512 iteration
    for (; x + 16 <= width; x += 16) {
        const size_t px_offset = static_cast<size_t>(x) * 4;

        const __m512i fg_u8 = _mm512_loadu_si512(reinterpret_cast<const void*>(fg_row + px_offset));
        const __m512i bg_u8 = _mm512_loadu_si512(reinterpret_cast<const void*>(bg_row + px_offset));

        // Load 16 bytes of alpha mask and expand each byte to all 4 channels of its pixel
        const __m128i m_raw = _mm_loadu_si128(reinterpret_cast<const __m128i*>(mask_row + x));
        const __m512i m_32  = _mm512_cvtepu8_epi32(m_raw);
        const __m512i a_u8  = _mm512_mullo_epi32(m_32, _mm512_set1_epi32(0x01010101));

        const __m512i fg_lo  = _mm512_unpacklo_epi8(fg_u8, zero);
        const __m512i fg_hi  = _mm512_unpackhi_epi8(fg_u8, zero);
        const __m512i bg_lo  = _mm512_unpacklo_epi8(bg_u8, zero);
        const __m512i bg_hi  = _mm512_unpackhi_epi8(bg_u8, zero);
        const __m512i a_lo_r = _mm512_unpacklo_epi8(a_u8, zero);
        const __m512i a_hi_r = _mm512_unpackhi_epi8(a_u8, zero);

        const __m512i a_lo     = _mm512_add_epi16(a_lo_r, _mm512_srli_epi16(a_lo_r, 7));
        const __m512i a_hi     = _mm512_add_epi16(a_hi_r, _mm512_srli_epi16(a_hi_r, 7));
        const __m512i inv_a_lo = _mm512_sub_epi16(v_256, a_lo);
        const __m512i inv_a_hi = _mm512_sub_epi16(v_256, a_hi);

        const __m512i res_lo = _mm512_srli_epi16(
            _mm512_add_epi16(_mm512_mullo_epi16(fg_lo, a_lo), _mm512_mullo_epi16(bg_lo, inv_a_lo)), 8);
        const __m512i res_hi = _mm512_srli_epi16(
            _mm512_add_epi16(_mm512_mullo_epi16(fg_hi, a_hi), _mm512_mullo_epi16(bg_hi, inv_a_hi)), 8);

        const __m512i packed = _mm512_or_si512(_mm512_packus_epi16(res_lo, res_hi), v_alpha_out);
        _mm512_storeu_si512(reinterpret_cast<void*>(out_row + px_offset), packed);
    }
#endif
    if (x < width) {
        BlendRowAVX2(fg_row + static_cast<size_t>(x) * 4,
                     bg_row + static_cast<size_t>(x) * 4,
                     mask_row + x,
                     out_row + static_cast<size_t>(x) * 4,
                     width - x);
    }
}

bool SimdCompositor::Blend(const FrameView& foreground,
                           const FrameView& background,
                           const FrameView& alpha_mask,
                           FrameView& output) {
    if (!foreground.IsValid() || !background.IsValid() || !alpha_mask.IsValid() || !output.IsValid()) {
        return false;
    }
    if (foreground.width != background.width || foreground.height != background.height ||
        foreground.width != alpha_mask.width || foreground.height != alpha_mask.height ||
        foreground.width != output.width || foreground.height != output.height) {
        return false;
    }
    if (GetBytesPerPixel(foreground.format) != 4 ||
        GetBytesPerPixel(background.format) != 4 ||
        GetBytesPerPixel(output.format) != 4 ||
        alpha_mask.format != PixelFormat::GRAY8) {
        return false;
    }

    const uint32_t width  = foreground.width;
    const uint32_t height = foreground.height;

    for (uint32_t y = 0; y < height; ++y) {
        const uint8_t* fg_row   = foreground.data + static_cast<size_t>(y) * foreground.stride;
        const uint8_t* bg_row   = background.data + static_cast<size_t>(y) * background.stride;
        const uint8_t* mask_row = alpha_mask.data + static_cast<size_t>(y) * alpha_mask.stride;
        uint8_t*       out_row  = output.data + static_cast<size_t>(y) * output.stride;

        switch (active_tier_) {
            case SimdTier::AVX512:
                BlendRowAVX512(fg_row, bg_row, mask_row, out_row, width);
                break;
            case SimdTier::AVX2:
                BlendRowAVX2(fg_row, bg_row, mask_row, out_row, width);
                break;
            case SimdTier::SCALAR:
            default:
                BlendRowScalar(fg_row, bg_row, mask_row, out_row, width);
                break;
        }
    }

    output.timestamp_ns = foreground.timestamp_ns;
    output.frame_index  = foreground.frame_index;
    return true;
}

bool SimdCompositor::FillSolidBackground(FrameView& bg_frame, uint8_t r, uint8_t g, uint8_t b) noexcept {
    if (!bg_frame.IsValid() || GetBytesPerPixel(bg_frame.format) != 4) {
        return false;
    }

    const bool is_bgra = (bg_frame.format == PixelFormat::BGRA32);
    const uint8_t c0 = is_bgra ? b : r;
    const uint8_t c1 = g;
    const uint8_t c2 = is_bgra ? r : b;
    const uint32_t packed_px =
        static_cast<uint32_t>(c0) |
        (static_cast<uint32_t>(c1) << 8) |
        (static_cast<uint32_t>(c2) << 16) |
        (255u << 24);

    for (uint32_t y = 0; y < bg_frame.height; ++y) {
        uint32_t* row = reinterpret_cast<uint32_t*>(bg_frame.data + static_cast<size_t>(y) * bg_frame.stride);
        std::fill(row, row + bg_frame.width, packed_px);
    }
    return true;
}

bool SimdCompositor::GenerateBlurredBackground(const FrameView& src, FrameView& dst_bg, uint32_t radius) noexcept {
    if (!src.IsValid() || !dst_bg.IsValid() ||
        src.width != dst_bg.width || src.height != dst_bg.height ||
        GetBytesPerPixel(src.format) != 4 || src.format != dst_bg.format) {
        return false;
    }

    const uint32_t w = src.width;
    const uint32_t h = src.height;
    const int32_t r  = static_cast<int32_t>(std::clamp(radius, 1u, 64u));
    const uint32_t diam = static_cast<uint32_t>(2 * r + 1);

    std::vector<uint8_t> temp(static_cast<size_t>(w) * h * 4);

    // Horizontal O(1) sliding window pass
    for (uint32_t y = 0; y < h; ++y) {
        const uint8_t* src_row = src.data + static_cast<size_t>(y) * src.stride;
        uint8_t* tmp_row       = temp.data() + static_cast<size_t>(y) * w * 4;

        uint32_t sum0 = 0, sum1 = 0, sum2 = 0;
        for (int32_t k = -r; k <= r; ++k) {
            const uint32_t sx = static_cast<uint32_t>(std::clamp(k, 0, static_cast<int32_t>(w) - 1));
            sum0 += src_row[sx * 4 + 0];
            sum1 += src_row[sx * 4 + 1];
            sum2 += src_row[sx * 4 + 2];
        }

        for (uint32_t x = 0; x < w; ++x) {
            tmp_row[x * 4 + 0] = static_cast<uint8_t>(sum0 / diam);
            tmp_row[x * 4 + 1] = static_cast<uint8_t>(sum1 / diam);
            tmp_row[x * 4 + 2] = static_cast<uint8_t>(sum2 / diam);
            tmp_row[x * 4 + 3] = 255;

            const uint32_t left_x  = static_cast<uint32_t>(std::max(0, static_cast<int32_t>(x) - r));
            const uint32_t right_x = static_cast<uint32_t>(std::min(static_cast<int32_t>(w) - 1, static_cast<int32_t>(x) + r + 1));

            sum0 += src_row[right_x * 4 + 0] - src_row[left_x * 4 + 0];
            sum1 += src_row[right_x * 4 + 1] - src_row[left_x * 4 + 1];
            sum2 += src_row[right_x * 4 + 2] - src_row[left_x * 4 + 2];
        }
    }

    // Vertical O(1) sliding window pass
    for (uint32_t x = 0; x < w; ++x) {
        uint32_t sum0 = 0, sum1 = 0, sum2 = 0;
        for (int32_t k = -r; k <= r; ++k) {
            const uint32_t sy = static_cast<uint32_t>(std::clamp(k, 0, static_cast<int32_t>(h) - 1));
            const uint8_t* p  = temp.data() + (static_cast<size_t>(sy) * w + x) * 4;
            sum0 += p[0];
            sum1 += p[1];
            sum2 += p[2];
        }

        for (uint32_t y = 0; y < h; ++y) {
            uint8_t* dst_px = dst_bg.data + static_cast<size_t>(y) * dst_bg.stride + static_cast<size_t>(x) * 4;
            dst_px[0] = static_cast<uint8_t>(sum0 / diam);
            dst_px[1] = static_cast<uint8_t>(sum1 / diam);
            dst_px[2] = static_cast<uint8_t>(sum2 / diam);
            dst_px[3] = 255;

            const uint32_t top_y = static_cast<uint32_t>(std::max(0, static_cast<int32_t>(y) - r));
            const uint32_t bot_y = static_cast<uint32_t>(std::min(static_cast<int32_t>(h) - 1, static_cast<int32_t>(y) + r + 1));

            const uint8_t* p_top = temp.data() + (static_cast<size_t>(top_y) * w + x) * 4;
            const uint8_t* p_bot = temp.data() + (static_cast<size_t>(bot_y) * w + x) * 4;

            sum0 += p_bot[0] - p_top[0];
            sum1 += p_bot[1] - p_top[1];
            sum2 += p_bot[2] - p_top[2];
        }
    }

    return true;
}

}  // namespace engine
}  // namespace common
}  // namespace smartcam
