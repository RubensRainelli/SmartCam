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

#define SMARTCAM_ENGINE_EXPORTS 1
#include "c_api.h"
#include "compositor/compositor.h"
#include "crop_scaler/crop_scaler.h"
#include "gpu/gpu_accelerator.h"
#include "segmentation/ai_onnx_segmenter.h"
#include "segmentation/green_screen_segmenter.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstring>
#include <new>
#include <vector>

#if defined(_WIN32)
    #ifndef WIN32_LEAN_AND_MEAN
        #define WIN32_LEAN_AND_MEAN
    #endif
    #ifndef NOMINMAX
        #define NOMINMAX
    #endif
    #include <windows.h>
#else
    #include <time.h>
#endif

namespace {

double GetCurrentThreadCpuTimeMs() noexcept {
#if defined(_WIN32)
    FILETIME create_time{}, exit_time{}, kernel_time{}, user_time{};
    if (::GetThreadTimes(::GetCurrentThread(), &create_time, &exit_time, &kernel_time, &user_time)) {
        ULARGE_INTEGER k{}, u{};
        k.LowPart  = kernel_time.dwLowDateTime;
        k.HighPart = kernel_time.dwHighDateTime;
        u.LowPart  = user_time.dwLowDateTime;
        u.HighPart = user_time.dwHighDateTime;
        // FILETIME is in 100-nanosecond intervals (10,000 intervals = 1 ms)
        return static_cast<double>(k.QuadPart + u.QuadPart) * 1e-4;
    }
    return 0.0;
#else
    struct timespec ts{};
    if (::clock_gettime(CLOCK_THREAD_CPUTIME_ID, &ts) == 0) {
        return static_cast<double>(ts.tv_sec) * 1000.0 + static_cast<double>(ts.tv_nsec) * 1e-6;
    }
    return 0.0;
#endif
}

constexpr uint32_t kFastGridW = 64;
constexpr uint32_t kFastGridH = 36;

void CopyStringSafe(char* dst, uint32_t cap, const char* src) noexcept {
    if (!dst || cap == 0) return;
    if (!src) {
        dst[0] = '\0';
        return;
    }
    const size_t len = std::strlen(src);
    const size_t copy_len = std::min(len, static_cast<size_t>(cap - 1));
    std::memcpy(dst, src, copy_len);
    dst[copy_len] = '\0';
}

smartcam::common::engine::SimdTier MapPolicySimdMode(uint32_t simd_mode) noexcept {
    using smartcam::common::engine::SimdTier;
    // simd_mode: 0=Auto, 1=AVX512, 2=AVX2, 3=Scalar (No AVX)
    switch (simd_mode) {
        case 1:  return SimdTier::AVX512;
        case 2:  return SimdTier::AVX2;
        case 3:  return SimdTier::SCALAR;
        case 0:
        default: return SimdTier::AUTO;
    }
}

smartcam::common::engine::ComputeBackend MapPolicyComputeBackend(uint32_t compute_backend) noexcept {
    using smartcam::common::engine::ComputeBackend;
    // compute_backend: 0=Auto, 1=CPU, 2=Vulkan, 3=OpenCL, 4=CUDA, 5=ROCm
    switch (compute_backend) {
        case 1:  return ComputeBackend::CPU;
        case 2:  return ComputeBackend::VULKAN;
        case 3:  return ComputeBackend::OPENCL;
        case 4:  return ComputeBackend::CUDA;
        case 5:  return ComputeBackend::ROCM;
        case 0:
        default: return ComputeBackend::AUTO;
    }
}

void DownsampleBgraToFastGrid(const uint8_t* src_bgra,
                              uint32_t width,
                              uint32_t height,
                              uint32_t stride,
                              std::vector<uint8_t>& out_grid) noexcept {
    out_grid.resize(static_cast<size_t>(kFastGridW) * kFastGridH * 4);
    for (uint32_t gy = 0; gy < kFastGridH; ++gy) {
        const uint32_t sy = ((gy * 2 + 1) * height) / (kFastGridH * 2);
        const uint8_t* src_row = src_bgra + static_cast<size_t>(std::min(sy, height - 1)) * stride;
        uint8_t* dst_row = out_grid.data() + static_cast<size_t>(gy) * kFastGridW * 4;
        for (uint32_t gx = 0; gx < kFastGridW; ++gx) {
            const uint32_t sx = ((gx * 2 + 1) * width) / (kFastGridW * 2);
            const size_t s_off = static_cast<size_t>(std::min(sx, width - 1)) * 4;
            const size_t d_off = static_cast<size_t>(gx) * 4;
            dst_row[d_off + 0] = src_row[s_off + 0];
            dst_row[d_off + 1] = src_row[s_off + 1];
            dst_row[d_off + 2] = src_row[s_off + 2];
            dst_row[d_off + 3] = 255;
        }
    }
}

void WriteFeatheredSpanRow(uint8_t* mask_row,
                           uint32_t width,
                           int inner_left,
                           int inner_right,
                           int feather_px) noexcept {
    if (inner_left > inner_right || inner_right < 0 || inner_left >= static_cast<int>(width)) {
        std::memset(mask_row, 0, width);
        return;
    }

    const int f = std::max(1, feather_px);
    const int outer_left  = std::max(0, inner_left - f);
    const int core_left   = std::clamp(inner_left + f, 0, static_cast<int>(width));
    const int core_right  = std::clamp(inner_right - f, core_left, static_cast<int>(width));
    const int outer_right = std::min(static_cast<int>(width), inner_right + f);

    if (outer_left > 0) {
        std::memset(mask_row, 0, static_cast<size_t>(outer_left));
    }

    const int left_span = core_left - outer_left;
    for (int x = outer_left; x < core_left; ++x) {
        const uint32_t num = static_cast<uint32_t>(x - outer_left + 1) * 255u;
        mask_row[x] = static_cast<uint8_t>(num / static_cast<uint32_t>(left_span + 1));
    }

    if (core_right > core_left) {
        std::memset(mask_row + core_left, 255, static_cast<size_t>(core_right - core_left));
    }

    const int right_span = outer_right - core_right;
    for (int x = core_right; x < outer_right; ++x) {
        const uint32_t rem = static_cast<uint32_t>(outer_right - x) * 255u;
        mask_row[x] = static_cast<uint8_t>(rem / static_cast<uint32_t>(right_span + 1));
    }

    if (outer_right < static_cast<int>(width)) {
        std::memset(mask_row + outer_right, 0, static_cast<size_t>(width - outer_right));
    }
}

bool ComputeUltraFastNonAiMask(const uint8_t* src_bgra,
                               uint32_t width,
                               uint32_t height,
                               uint32_t src_stride,
                               uint8_t* out_mask_gray8,
                               uint32_t mask_stride,
                               const std::vector<uint8_t>& fast_ref_bg_grid,
                               bool has_ref_bg,
                               float bg_diff_tolerance,
                               const smartcam::common::AiSegmenterConfig& ai_cfg,
                               float& inout_cx,
                               float& inout_cy) noexcept {
    if (!src_bgra || !out_mask_gray8 || width == 0 || height == 0) {
        return false;
    }

    const int feather_px = std::max(2, static_cast<int>(width / 96));
    const bool face_only_mode = (ai_cfg.silhouette_mode == 1);

    // Path A: Ultra-fast 64x36 Reference Background Difference (<0.12 ms)
    if (has_ref_bg && fast_ref_bg_grid.size() == static_cast<size_t>(kFastGridW) * kFastGridH * 4) {
        const int thresh = static_cast<int>(22.0f + std::clamp(bg_diff_tolerance, 0.0f, 1.0f) * 36.0f);
        int row_l[kFastGridH];
        int row_r[kFastGridH];
        float sum_wx = 0.0f, sum_wy = 0.0f, sum_w = 0.0f;

        for (uint32_t gy = 0; gy < kFastGridH; ++gy) {
            row_l[gy] = -1;
            row_r[gy] = -2;
            const uint32_t sy = ((gy * 2 + 1) * height) / (kFastGridH * 2);
            const uint8_t* src_row = src_bgra + static_cast<size_t>(std::min(sy, height - 1)) * src_stride;
            const uint8_t* ref_row = fast_ref_bg_grid.data() + static_cast<size_t>(gy) * kFastGridW * 4;

            for (uint32_t gx = 0; gx < kFastGridW; ++gx) {
                const uint32_t sx = ((gx * 2 + 1) * width) / (kFastGridW * 2);
                const size_t s_off = static_cast<size_t>(std::min(sx, width - 1)) * 4;
                const size_t r_off = static_cast<size_t>(gx) * 4;

                const int db = std::abs(static_cast<int>(src_row[s_off + 0]) - static_cast<int>(ref_row[r_off + 0]));
                const int dg = std::abs(static_cast<int>(src_row[s_off + 1]) - static_cast<int>(ref_row[r_off + 1]));
                const int dr = std::abs(static_cast<int>(src_row[s_off + 2]) - static_cast<int>(ref_row[r_off + 2]));
                const int diff = (dr + dg + db);

                if (diff > thresh * 3) {
                    if (row_l[gy] < 0) row_l[gy] = static_cast<int>(gx);
                    row_r[gy] = static_cast<int>(gx);
                    sum_wx += static_cast<float>(gx) / static_cast<float>(kFastGridW);
                    sum_wy += static_cast<float>(gy) / static_cast<float>(kFastGridH);
                    sum_w  += 1.0f;
                }
            }
        }

        if (sum_w >= 4.0f) {
            inout_cx = std::clamp(sum_wx / sum_w, 0.10f, 0.90f);
            inout_cy = std::clamp(sum_wy / sum_w, 0.15f, 0.85f);

            for (uint32_t y = 0; y < height; ++y) {
                uint8_t* m_row = out_mask_gray8 + static_cast<size_t>(y) * mask_stride;
                const uint32_t gy = std::min((y * kFastGridH) / height, kFastGridH - 1);
                if (row_l[gy] < 0 || row_r[gy] < row_l[gy]) {
                    std::memset(m_row, 0, width);
                } else {
                    const int il = (row_l[gy] * static_cast<int>(width)) / static_cast<int>(kFastGridW);
                    const int ir = ((row_r[gy] + 1) * static_cast<int>(width)) / static_cast<int>(kFastGridW) - 1;
                    WriteFeatheredSpanRow(m_row, width, il, ir, feather_px);
                }
            }
            return true;
        }
    }

    // Path B: Ultra-fast 32x18 Facial Skin Centroid + Analytical Silhouette Span (<0.08 ms)
    constexpr uint32_t kCoarseW = 32;
    constexpr uint32_t kCoarseH = 18;
    float sum_wx = 0.0f, sum_wy = 0.0f, sum_w = 0.0f;
    float sum_wx_L = 0.0f, sum_wy_L = 0.0f, sum_w_L = 0.0f;
    float sum_wx_R = 0.0f, sum_wy_R = 0.0f, sum_w_R = 0.0f;

    for (uint32_t gy = 2; gy < kCoarseH; ++gy) {
        const uint32_t sy = (gy * height) / kCoarseH;
        const uint8_t* src_row = src_bgra + static_cast<size_t>(sy) * src_stride;
        const float ny = (static_cast<float>(gy) + 0.5f) / static_cast<float>(kCoarseH);
        // Restrict head centroid search to upper-middle face region (0.15 .. 0.58)
        if (ny < 0.15f || ny > 0.58f) continue;

        // Sample left and right border pixels on the same row to ignore wooden stairs/walls extending from border
        const int bL = src_row[4 + 0], gL = src_row[4 + 1], rL = src_row[4 + 2];
        const size_t r_off = static_cast<size_t>(width - 2) * 4;
        const int bR = src_row[r_off + 0], gR = src_row[r_off + 1], rR = src_row[r_off + 2];

        for (uint32_t gx = 2; gx + 2 < kCoarseW; ++gx) {
            const uint32_t sx = (gx * width) / kCoarseW;
            const size_t off = static_cast<size_t>(sx) * 4;
            const int b = src_row[off + 0];
            const int g = src_row[off + 1];
            const int r = src_row[off + 2];

            // Skip chroma green pixels
            if (g > 135 && g > r + 35 && g > b + 35) continue;

            // Reject pixels matching left/right room border on this row (e.g. wooden stairs)
            const int diff_L = std::abs(r - rL) + std::abs(g - gL) + std::abs(b - bL);
            const int diff_R = std::abs(r - rR) + std::abs(g - gR) + std::abs(b - bR);
            if (diff_L < 38 || diff_R < 38) continue;

            // True peach/rose facial skin check (rejects yellow hoodies where R-G is small vs G-B, and orange wood)
            const int rb = r - b;
            const int rg = r - g;
            if (r > 85 && g > 45 && b > 25 && rb >= 18 && rg >= (rb * 24) / 100 && rg <= (rb * 85) / 100 && rb * 2 <= r) {
                const float w = static_cast<float>(rb);
                const float nx = (static_cast<float>(gx) + 0.5f) / static_cast<float>(kCoarseW);
                sum_wx += nx * w;
                sum_wy += ny * w;
                sum_w  += w;
                if (nx < 0.50f) {
                    sum_wx_L += nx * w; sum_wy_L += ny * w; sum_w_L += w;
                } else {
                    sum_wx_R += nx * w; sum_wy_R += ny * w; sum_w_R += w;
                }
            }
        }
    }

    if (ai_cfg.face_calibrated && ai_cfg.calib_anchor_cx > 0.05f && ai_cfg.calib_anchor_cy > 0.05f) {
        // Anchor directly around the 5-point calibrated head center and avoid background wood pull
        const float cal_cx = std::clamp(ai_cfg.calib_anchor_cx, 0.12f, 0.88f);
        const float cal_cy = std::clamp(ai_cfg.calib_anchor_cy, 0.14f, 0.82f);
        if (sum_w > 25.0f) {
            const float raw_cx = std::clamp(sum_wx / sum_w, 0.15f, 0.85f);
            const float raw_cy = std::clamp(sum_wy / sum_w, 0.20f, 0.65f);
            if (std::hypot(raw_cx - cal_cx, raw_cy - cal_cy) <= 0.14f) {
                inout_cx = 0.65f * cal_cx + 0.35f * raw_cx;
                inout_cy = 0.65f * cal_cy + 0.35f * raw_cy;
            } else {
                inout_cx = cal_cx;
                inout_cy = cal_cy;
            }
        } else {
            inout_cx = cal_cx;
            inout_cy = cal_cy;
        }
    } else if (sum_w > 20.0f) {
        const float target_cx = std::clamp(sum_wx / sum_w, 0.15f, 0.85f);
        const float target_cy = std::clamp(sum_wy / sum_w, 0.20f, 0.58f);
        const float jump = std::hypot(target_cx - inout_cx, target_cy - inout_cy);
        const float alpha = (jump > 0.04f) ? 1.0f : 0.70f;
        inout_cx = (1.0f - alpha) * inout_cx + alpha * target_cx;
        inout_cy = (1.0f - alpha) * inout_cy + alpha * target_cy;
    }

    const float cx       = inout_cx;
    const float head_cy  = inout_cy;
    const float head_rx  = ai_cfg.face_calibrated ? std::clamp(ai_cfg.calib_head_rx, 0.065f, 0.28f) : 0.125f;
    const float head_ry  = ai_cfg.face_calibrated ? std::clamp(ai_cfg.calib_head_ry, 0.110f, 0.38f) : 0.215f;
    const float torso_rx = 0.21f;
    const float torso_ry = 0.28f;
    const float inv_h    = 1.0f / static_cast<float>(height);
    const float fw       = static_cast<float>(width);

    const bool has_two_persons = ai_cfg.multi_person_enabled && (sum_w_L > 20.0f) && (sum_w_R > 20.0f) &&
                                 (std::abs((sum_wx_R / sum_w_R) - (sum_wx_L / sum_w_L)) >= 2.8f * head_rx);
    const float cx_L = has_two_persons ? std::clamp(sum_wx_L / sum_w_L, 0.12f, 0.48f) : cx;
    const float cy_L = has_two_persons ? std::clamp(sum_wy_L / sum_w_L, 0.18f, 0.58f) : head_cy;
    const float cx_R = has_two_persons ? std::clamp(sum_wx_R / sum_w_R, 0.52f, 0.88f) : cx;
    const float cy_R = has_two_persons ? std::clamp(sum_wy_R / sum_w_R, 0.18f, 0.58f) : head_cy;

    auto compute_half_w = [&](float p_cy, float ny) noexcept -> float {
        float half_w_norm = 0.0f;
        const float dy_h = (ny - p_cy) / head_ry;
        if (std::abs(dy_h) < 1.0f) {
            half_w_norm = std::max(half_w_norm, head_rx * std::sqrt(1.0f - dy_h * dy_h));
        }
        if (!face_only_mode && ny >= (p_cy + 0.75f * head_ry)) {
            const float gate = std::clamp((ny - (p_cy + 0.75f * head_ry)) / 0.08f, 0.45f, 1.0f);
            const float dy_t = (ny - (p_cy + 0.34f)) / torso_ry;
            if (std::abs(dy_t) < 1.0f) {
                half_w_norm = std::max(half_w_norm, (torso_rx * gate) * std::sqrt(1.0f - dy_t * dy_t));
            }
        }
        return half_w_norm;
    };

    for (uint32_t y = 0; y < height; ++y) {
        uint8_t* m_row = out_mask_gray8 + static_cast<size_t>(y) * mask_stride;
        const float ny = (static_cast<float>(y) + 0.5f) * inv_h;

        if (!has_two_persons) {
            const float half_w_norm = compute_half_w(head_cy, ny);
            if (half_w_norm <= 0.002f) {
                std::memset(m_row, 0, width);
            } else {
                const int il = static_cast<int>((cx - half_w_norm) * fw);
                const int ir = static_cast<int>((cx + half_w_norm) * fw);
                WriteFeatheredSpanRow(m_row, width, il, ir, feather_px);
            }
        } else {
            std::memset(m_row, 0, width);
            const float hw_L = compute_half_w(cy_L, ny);
            const float hw_R = compute_half_w(cy_R, ny);
            const uint32_t mid_x = width / 2;
            if (hw_L > 0.002f) {
                const int il = static_cast<int>((cx_L - hw_L) * fw);
                const int ir = static_cast<int>((cx_L + hw_L) * fw);
                WriteFeatheredSpanRow(m_row, mid_x, il, std::min(ir, static_cast<int>(mid_x) - 1), feather_px);
            }
            if (hw_R > 0.002f) {
                const int il = static_cast<int>((cx_R - hw_R) * fw) - static_cast<int>(mid_x);
                const int ir = static_cast<int>((cx_R + hw_R) * fw) - static_cast<int>(mid_x);
                WriteFeatheredSpanRow(m_row + mid_x, width - mid_x, std::max(0, il), ir, feather_px);
            }
        }
    }

    return true;
}

}  // namespace

struct SmartCamEngineHandle_t {
    smartcam::common::engine::SimdCompositor       compositor;
    smartcam::common::engine::GreenScreenSegmenter green_segmenter;
    smartcam::common::engine::AiOnnxSegmenter      ai_segmenter;
    smartcam::common::engine::GpuAccelerator       gpu_accel;
    uint32_t                                       compute_backend_policy = 1;  // Default CPU on legacy create
    uint32_t                                       simd_mode_policy       = 0;  // 0 = Auto
    bool                                           ai_enabled             = true;
    std::vector<uint8_t>                           fast_ref_bg_grid{};
    float                                          fast_tracked_cx        = 0.50f;
    float                                          fast_tracked_cy        = 0.42f;

    explicit SmartCamEngineHandle_t(smartcam::common::engine::SimdTier tier)
        : compositor(tier),
          green_segmenter(),
          ai_segmenter(),
          gpu_accel(smartcam::common::engine::ComputeBackend::CPU, tier) {}
};

extern "C" {

SMARTCAM_API SmartCamEngineHandle smartcam_engine_create(uint32_t simd_tier) {
    using smartcam::common::engine::SimdTier;
    SimdTier tier = SimdTier::AUTO;
    if (simd_tier == 1) tier = SimdTier::SCALAR;
    else if (simd_tier == 2) tier = SimdTier::AVX2;
    else if (simd_tier == 3) tier = SimdTier::AVX512;
    return new (std::nothrow) SmartCamEngineHandle_t(tier);
}

SMARTCAM_API void smartcam_engine_destroy(SmartCamEngineHandle handle) {
    delete handle;
}

SMARTCAM_API const char* smartcam_engine_get_simd_backend(SmartCamEngineHandle handle) {
    if (!handle) return "Uninitialized";
    return handle->compositor.GetActiveSimdBackend();
}

SMARTCAM_API void smartcam_engine_configure_greenscreen(
    SmartCamEngineHandle handle,
    uint8_t hue_min,
    uint8_t hue_max,
    uint8_t sat_min,
    uint8_t sat_max,
    uint8_t val_min,
    uint8_t val_max,
    uint8_t morph_kernel_size,
    uint8_t blur_radius,
    float spill_suppression) {
    if (!handle) return;
    smartcam::common::GreenScreenConfig cfg;
    cfg.hue_min           = hue_min;
    cfg.hue_max           = hue_max;
    cfg.sat_min           = sat_min;
    cfg.sat_max           = sat_max;
    cfg.val_min           = val_min;
    cfg.val_max           = val_max;
    cfg.morph_kernel_size = morph_kernel_size;
    cfg.blur_radius       = blur_radius;
    cfg.spill_suppression = spill_suppression;
    handle->green_segmenter.UpdateConfig(cfg);
}

SMARTCAM_API int smartcam_engine_autocalibrate_greenscreen(
    SmartCamEngineHandle handle,
    const uint8_t* frame_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    int32_t sample_x,
    int32_t sample_y,
    uint8_t* out_hue_min,
    uint8_t* out_hue_max,
    uint8_t* out_sat_min,
    uint8_t* out_val_min) {
    if (!handle || !frame_bgra) return 0;
    using namespace smartcam::common;

    FrameView frame{const_cast<uint8_t*>(frame_bgra), width, height, stride, PixelFormat::BGRA32, 0, 0};
    return handle->green_segmenter.AutoCalibrate(
        frame, sample_x, sample_y, out_hue_min, out_hue_max, out_sat_min, out_val_min) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_crop_scale_bgra(
    const uint8_t* src_bgra,
    uint32_t src_width,
    uint32_t src_height,
    uint32_t src_stride,
    uint8_t* dst_bgra,
    uint32_t dst_width,
    uint32_t dst_height,
    uint32_t dst_stride,
    uint32_t aspect_w,
    uint32_t aspect_h,
    uint32_t strategy) {
    using namespace smartcam::common;
    using namespace smartcam::common::engine;

    FrameView src{const_cast<uint8_t*>(src_bgra), src_width, src_height, src_stride, PixelFormat::BGRA32, 0, 0};
    FrameView dst{dst_bgra, dst_width, dst_height, dst_stride, PixelFormat::BGRA32, 0, 0};

    CropStrategy strat = (strategy == 1) ? CropStrategy::LETTERBOX : CropStrategy::CENTER_CROP;
    return CropScaler::Process(src, dst, AspectRatioMode::CUSTOM, strat, aspect_w, aspect_h) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_compute_greenscreen_mask(
    SmartCamEngineHandle handle,
    uint8_t* inout_fg_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t fg_stride,
    uint8_t* out_mask_gray8,
    uint32_t mask_stride) {
    if (!handle) return 0;
    using namespace smartcam::common;

    FrameView fg{inout_fg_bgra, width, height, fg_stride, PixelFormat::BGRA32, 0, 0};
    FrameView mask{out_mask_gray8, width, height, mask_stride, PixelFormat::GRAY8, 0, 0};

    if (!handle->green_segmenter.ComputeMask(fg, mask)) {
        return 0;
    }
    handle->green_segmenter.SuppressGreenSpill(fg, mask);
    return 1;
}

SMARTCAM_API int smartcam_engine_compute_ai_mask(
    SmartCamEngineHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t src_stride,
    uint8_t* out_mask_gray8,
    uint32_t mask_stride) {
    if (!handle || !src_bgra || !out_mask_gray8) return 0;
    using namespace smartcam::common;

    const uint32_t eff_src_stride  = (src_stride >= width * 4) ? src_stride : (width * 4);
    const uint32_t eff_mask_stride = (mask_stride >= width) ? mask_stride : width;

    // Fast Non-AI path (<0.2 ms) when ai_enabled == 0
    if (!handle->ai_enabled) {
        return ComputeUltraFastNonAiMask(
            src_bgra,
            width,
            height,
            eff_src_stride,
            out_mask_gray8,
            eff_mask_stride,
            handle->fast_ref_bg_grid,
            handle->ai_segmenter.HasReferenceBackground(),
            handle->ai_segmenter.GetBgDiffTolerance(),
            handle->ai_segmenter.GetConfig(),
            handle->fast_tracked_cx,
            handle->fast_tracked_cy) ? 1 : 0;
    }

    FrameView src{const_cast<uint8_t*>(src_bgra), width, height, eff_src_stride, PixelFormat::BGRA32, 0, 0};
    FrameView mask{out_mask_gray8, width, height, eff_mask_stride, PixelFormat::GRAY8, 0, 0};
    return handle->ai_segmenter.ComputeMask(src, mask) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_postprocess_ai_mask(
    SmartCamEngineHandle handle,
    const float* prob_map,
    uint32_t map_width,
    uint32_t map_height,
    uint8_t* out_mask_gray8,
    uint32_t out_width,
    uint32_t out_height,
    uint32_t out_stride) {
    if (!handle) return 0;
    using namespace smartcam::common;
    using smartcam::common::engine::ComputeBackend;

    FrameView mask{out_mask_gray8, out_width, out_height, out_stride, PixelFormat::GRAY8, 0, 0};
    if (handle->gpu_accel.GetActiveBackend() != ComputeBackend::CPU) {
        return handle->gpu_accel.AccelerateAiPostProcess(prob_map, map_width, map_height, mask) ? 1 : 0;
    }
    return handle->ai_segmenter.PostProcessProbabilityMap(prob_map, map_width, map_height, mask) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_fill_solid_bg(
    uint8_t* dst_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t dst_stride,
    uint8_t r,
    uint8_t g,
    uint8_t b) {
    if (!dst_bgra) return 0;
    using namespace smartcam::common;
    using namespace smartcam::common::engine;

    FrameView dst{dst_bgra, width, height, dst_stride, PixelFormat::BGRA32, 0, 0};
    return SimdCompositor::FillSolidBackground(dst, r, g, b) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_generate_blurred_bg(
    const uint8_t* src_bgra,
    uint8_t* dst_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    uint32_t blur_radius) {
    if (!src_bgra || !dst_bgra) return 0;
    using namespace smartcam::common;
    using namespace smartcam::common::engine;

    FrameView src{const_cast<uint8_t*>(src_bgra), width, height, stride, PixelFormat::BGRA32, 0, 0};
    FrameView dst{dst_bgra, width, height, stride, PixelFormat::BGRA32, 0, 0};
    return SimdCompositor::GenerateBlurredBackground(src, dst, blur_radius) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_blend_bgra(
    SmartCamEngineHandle handle,
    const uint8_t* fg_bgra,
    const uint8_t* bg_bgra,
    const uint8_t* mask_gray8,
    uint8_t* out_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t rgba_stride,
    uint32_t mask_stride) {
    if (!handle) return 0;
    using namespace smartcam::common;
    using smartcam::common::engine::ComputeBackend;

    FrameView fg{const_cast<uint8_t*>(fg_bgra), width, height, rgba_stride, PixelFormat::BGRA32, 0, 0};
    FrameView bg{const_cast<uint8_t*>(bg_bgra), width, height, rgba_stride, PixelFormat::BGRA32, 0, 0};
    FrameView mask{const_cast<uint8_t*>(mask_gray8), width, height, mask_stride, PixelFormat::GRAY8, 0, 0};
    FrameView out{out_bgra, width, height, rgba_stride, PixelFormat::BGRA32, 0, 0};

    if (handle->gpu_accel.GetActiveBackend() != ComputeBackend::CPU) {
        return handle->gpu_accel.AccelerateBlendBgra(fg, bg, mask, out) ? 1 : 0;
    }
    return handle->compositor.Blend(fg, bg, mask, out) ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_set_reference_background(
    SmartCamEngineHandle handle,
    const uint8_t* bg_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride) {
    if (!handle || !bg_bgra || width == 0 || height == 0) return 0;
    using namespace smartcam::common;

    const uint32_t row_stride = (stride >= width * 4) ? stride : (width * 4);
    DownsampleBgraToFastGrid(bg_bgra, width, height, row_stride, handle->fast_ref_bg_grid);
    FrameView bg{const_cast<uint8_t*>(bg_bgra), width, height, row_stride, PixelFormat::BGRA32, 0, 0};
    return handle->ai_segmenter.SetReferenceBackground(bg) ? 1 : 0;
}

SMARTCAM_API void smartcam_engine_clear_reference_background(SmartCamEngineHandle handle) {
    if (!handle) return;
    handle->fast_ref_bg_grid.clear();
    handle->ai_segmenter.ClearReferenceBackground();
}

SMARTCAM_API int smartcam_engine_has_reference_background(SmartCamEngineHandle handle) {
    if (!handle) return 0;
    return handle->ai_segmenter.HasReferenceBackground() ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_get_subject_center(
    SmartCamEngineHandle handle,
    float* out_cx,
    float* out_cy) {
    if (!handle) return 0;
    if (!handle->ai_enabled) {
        if (out_cx) *out_cx = handle->fast_tracked_cx;
        if (out_cy) *out_cy = handle->fast_tracked_cy;
        return 1;
    }
    handle->ai_segmenter.GetTrackedSubjectCenter(out_cx, out_cy);
    return 1;
}

SMARTCAM_API int smartcam_engine_set_ai_mode_and_tolerance(
    SmartCamEngineHandle handle,
    int engine_mode,
    float bg_diff_tolerance) {
    if (!handle) return 0;
    handle->ai_segmenter.SetAiModeAndTolerance(engine_mode, bg_diff_tolerance);
    return 1;
}

SMARTCAM_API int smartcam_engine_set_hardware_policy(
    SmartCamEngineHandle handle,
    uint32_t compute_backend,
    uint32_t simd_mode,
    int ai_enabled) {
    if (!handle) return 0;
    using smartcam::common::engine::ComputeBackend;
    using smartcam::common::engine::SimdTier;

    handle->compute_backend_policy = compute_backend;
    handle->simd_mode_policy       = simd_mode;
    handle->ai_enabled             = (ai_enabled != 0);

    const SimdTier target_simd = MapPolicySimdMode(simd_mode);
    handle->compositor.SetSimdTier(target_simd);
    handle->gpu_accel.SetSimdTier(target_simd);

    const ComputeBackend target_backend = MapPolicyComputeBackend(compute_backend);
    if (!handle->gpu_accel.SetActiveBackend(target_backend)) {
        // Safely fall back to CPU SIMD if the requested GPU backend is unavailable on this host
        handle->gpu_accel.SetActiveBackend(ComputeBackend::CPU);
    }

    return 1;
}

SMARTCAM_API int smartcam_engine_get_compute_device_count(SmartCamEngineHandle handle) {
    if (handle) {
        return static_cast<int>(handle->gpu_accel.GetDeviceCount());
    }
    static smartcam::common::engine::GpuAccelerator s_default_probe;
    return static_cast<int>(s_default_probe.GetDeviceCount());
}

SMARTCAM_API int smartcam_engine_get_compute_device_info(
    SmartCamEngineHandle handle,
    int index,
    uint32_t* out_backend_id,
    char* out_backend_name,
    uint32_t backend_name_cap,
    char* out_device_name,
    uint32_t device_name_cap,
    uint64_t* out_vram_mb,
    int* out_available,
    int* out_is_igpu) {
    if (index < 0) return 0;

    smartcam::common::ComputeDeviceInfo info{};
    bool ok = false;
    if (handle) {
        ok = handle->gpu_accel.GetDeviceInfoByIndex(static_cast<uint32_t>(index), &info);
    } else {
        static smartcam::common::engine::GpuAccelerator s_default_probe;
        ok = s_default_probe.GetDeviceInfoByIndex(static_cast<uint32_t>(index), &info);
    }
    if (!ok) return 0;

    if (out_backend_id) {
        *out_backend_id = static_cast<uint32_t>(info.backend);
    }
    CopyStringSafe(out_backend_name, backend_name_cap, info.backend_name);
    CopyStringSafe(out_device_name, device_name_cap, info.device_name);
    if (out_vram_mb) {
        *out_vram_mb = info.vram_bytes / (1024ULL * 1024ULL);
    }
    if (out_available) {
        *out_available = info.available ? 1 : 0;
    }
    if (out_is_igpu) {
        *out_is_igpu = info.is_integrated_gpu ? 1 : 0;
    }
    return 1;
}

SMARTCAM_API const char* smartcam_engine_get_active_compute_backend(SmartCamEngineHandle handle) {
    if (!handle) return "Uninitialized";
    return handle->gpu_accel.GetActiveBackendName();
}

SMARTCAM_API const char* smartcam_engine_get_active_compute_device_name(SmartCamEngineHandle handle) {
    if (!handle) return "Uninitialized";
    return handle->gpu_accel.GetActiveDeviceName();
}

SMARTCAM_API int smartcam_engine_is_ai_enabled(SmartCamEngineHandle handle) {
    if (!handle) return 0;
    return handle->ai_enabled ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_calibrate_face_landmarks(
    SmartCamEngineHandle handle,
    const uint8_t* frame_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    float left_eye_x,
    float left_eye_y,
    float right_eye_x,
    float right_eye_y,
    float nose_tip_x,
    float nose_tip_y,
    float lips_x,
    float lips_y,
    float chin_x,
    float chin_y,
    float* out_skin_cb,
    float* out_skin_cr,
    float* out_head_rx,
    float* out_head_ry) {
    if (!handle || !frame_bgra || width == 0 || height == 0) return 0;
    using namespace smartcam::common;
    const uint32_t eff_stride = (stride >= width * 4) ? stride : (width * 4);
    FrameView snap{const_cast<uint8_t*>(frame_bgra), width, height, eff_stride, PixelFormat::BGRA32, 0, 0};
    const bool ok = handle->ai_segmenter.CalibrateFaceFromLandmarks(
        snap,
        left_eye_x, left_eye_y,
        right_eye_x, right_eye_y,
        nose_tip_x, nose_tip_y,
        lips_x, lips_y,
        chin_x, chin_y,
        out_skin_cb, out_skin_cr,
        out_head_rx, out_head_ry);
    if (ok) {
        handle->ai_segmenter.GetTrackedSubjectCenter(&handle->fast_tracked_cx, &handle->fast_tracked_cy);
    }
    return ok ? 1 : 0;
}

SMARTCAM_API int smartcam_engine_set_face_calibration_profile(
    SmartCamEngineHandle handle,
    int enabled,
    float skin_cb,
    float skin_cr,
    float head_rx,
    float head_ry) {
    if (!handle) return 0;
    handle->ai_segmenter.SetFaceCalibrationProfile(enabled != 0, skin_cb, skin_cr, head_rx, head_ry);
    handle->ai_segmenter.GetTrackedSubjectCenter(&handle->fast_tracked_cx, &handle->fast_tracked_cy);
    return 1;
}

SMARTCAM_API int smartcam_engine_set_face_calibration_landmarks(
    SmartCamEngineHandle handle,
    const float* landmarks_10) {
    if (!handle || !landmarks_10) return 0;
    handle->ai_segmenter.SetFaceCalibrationLandmarks(landmarks_10);
    handle->ai_segmenter.GetTrackedSubjectCenter(&handle->fast_tracked_cx, &handle->fast_tracked_cy);
    return 1;
}

SMARTCAM_API int smartcam_engine_set_silhouette_mode(
    SmartCamEngineHandle handle,
    int silhouette_mode) {
    if (!handle) return 0;
    handle->ai_segmenter.SetSilhouetteMode(silhouette_mode);
    return 1;
}

SMARTCAM_API int smartcam_engine_get_silhouette_mode(
    SmartCamEngineHandle handle) {
    if (!handle) return 0;
    return handle->ai_segmenter.GetSilhouetteMode();
}

SMARTCAM_API int smartcam_engine_set_multi_person_mode(
    SmartCamEngineHandle handle,
    int multi_person_enabled) {
    if (!handle) return 0;
    handle->ai_segmenter.SetMultiPersonMode(multi_person_enabled != 0);
    return 1;
}

SMARTCAM_API int smartcam_engine_benchmark_profile(
    SmartCamEngineHandle handle,
    uint32_t compute_backend,
    uint32_t simd_mode,
    int ai_enabled,
    uint32_t width,
    uint32_t height,
    uint32_t iterations,
    float* out_avg_frame_ms,
    float* out_cpu_time_ms) {
    if (!handle) return 0;
    using namespace smartcam::common;
    using namespace smartcam::common::engine;

    const SimdTier max_cpu_simd = SimdCompositor::DetectBestSimdTier();
    if (compute_backend == 1) {
        if (simd_mode == 1 && max_cpu_simd < SimdTier::AVX512) return 0;
        if (simd_mode == 2 && max_cpu_simd < SimdTier::AVX2) return 0;
    } else if (compute_backend >= 2 && compute_backend <= 5) {
        const ComputeBackend target_b = MapPolicyComputeBackend(compute_backend);
        if (!handle->gpu_accel.IsBackendAvailable(target_b)) {
            return 0;
        }
    }

    const uint32_t bench_w    = std::clamp(width, 160u, 3840u);
    const uint32_t bench_h    = std::clamp(height, 120u, 2160u);
    const uint32_t bench_iter = std::clamp(iterations, 1u, 60u);

    const uint32_t saved_backend = handle->compute_backend_policy;
    const uint32_t saved_simd    = handle->simd_mode_policy;
    const int      saved_ai      = handle->ai_enabled ? 1 : 0;

    smartcam_engine_set_hardware_policy(handle, compute_backend, simd_mode, ai_enabled);

    FrameBuffer src_buf(bench_w, bench_h, PixelFormat::BGRA32);
    FrameBuffer fg_buf(bench_w, bench_h, PixelFormat::BGRA32);
    FrameBuffer bg_buf(bench_w, bench_h, PixelFormat::BGRA32);
    FrameBuffer mask_buf(bench_w, bench_h, PixelFormat::GRAY8);
    FrameBuffer out_buf(bench_w, bench_h, PixelFormat::BGRA32);

    // Populate synthetic portrait test frame
    for (uint32_t y = 0; y < bench_h; ++y) {
        uint8_t* s_row = src_buf.Data() + static_cast<size_t>(y) * src_buf.Stride();
        const float ny = static_cast<float>(y) / static_cast<float>(bench_h);
        for (uint32_t x = 0; x < bench_w; ++x) {
            const float nx = static_cast<float>(x) / static_cast<float>(bench_w);
            const float dx = (nx - 0.50f) / 0.11f;
            const float dy = (ny - 0.38f) / 0.18f;
            if (dx * dx + dy * dy <= 1.0f) {
                s_row[x * 4 + 0] = 125;
                s_row[x * 4 + 1] = 155;
                s_row[x * 4 + 2] = 215;
                s_row[x * 4 + 3] = 255;
            } else {
                s_row[x * 4 + 0] = 95;
                s_row[x * 4 + 1] = 70;
                s_row[x * 4 + 2] = 55;
                s_row[x * 4 + 3] = 255;
            }
        }
    }

    auto run_single_frame = [&]() {
        smartcam_engine_crop_scale_bgra(
            src_buf.Data(), bench_w, bench_h, src_buf.Stride(),
            fg_buf.Data(), bench_w, bench_h, fg_buf.Stride(),
            16, 9, 0);
        if (handle->gpu_accel.GetActiveBackend() != ComputeBackend::CPU) {
            FrameView fg_v = fg_buf.View();
            FrameView bg_v = bg_buf.View();
            handle->gpu_accel.AccelerateBlurBgra(fg_v, bg_v, 4);
        } else {
            smartcam_engine_generate_blurred_bg(fg_buf.Data(), bg_buf.Data(), bench_w, bench_h, fg_buf.Stride(), 4);
        }
        smartcam_engine_compute_ai_mask(
            handle, fg_buf.Data(), bench_w, bench_h, fg_buf.Stride(),
            mask_buf.Data(), mask_buf.Stride());
        smartcam_engine_blend_bgra(
            handle, fg_buf.Data(), bg_buf.Data(), mask_buf.Data(), out_buf.Data(),
            bench_w, bench_h, fg_buf.Stride(), mask_buf.Stride());
    };

    // Warm-up iteration
    run_single_frame();

    const auto   t0_wall = std::chrono::steady_clock::now();
    const double t0_cpu  = GetCurrentThreadCpuTimeMs();

    for (uint32_t it = 0; it < bench_iter; ++it) {
        run_single_frame();
    }

    const double t1_cpu  = GetCurrentThreadCpuTimeMs();
    const auto   t1_wall = std::chrono::steady_clock::now();

    const double total_wall_ms = std::chrono::duration<double, std::milli>(t1_wall - t0_wall).count();
    const double total_cpu_ms  = std::max(0.0, t1_cpu - t0_cpu);

    const float avg_wall_ms = static_cast<float>(std::max(0.01, total_wall_ms / static_cast<double>(bench_iter)));
    const float avg_cpu_ms  = static_cast<float>(std::max(0.01, total_cpu_ms / static_cast<double>(bench_iter)));

    if (out_avg_frame_ms) *out_avg_frame_ms = avg_wall_ms;
    if (out_cpu_time_ms)  *out_cpu_time_ms  = avg_cpu_ms;

    // Restore original hardware policy
    smartcam_engine_set_hardware_policy(handle, saved_backend, saved_simd, saved_ai);
    return 1;
}

}  // extern "C"
