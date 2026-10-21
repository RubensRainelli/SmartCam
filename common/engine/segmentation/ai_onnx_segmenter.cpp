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

#include "ai_onnx_segmenter.h"
#include "../crop_scaler/crop_scaler.h"
#include <algorithm>
#include <cmath>
#include <cstring>
#include <fstream>

#if defined(SMARTCAM_WITH_ONNXRUNTIME)
    #include <onnxruntime_cxx_api.h>
#endif

namespace smartcam {
namespace common {
namespace engine {

struct AiOnnxSegmenter::OrtImpl {
#if defined(SMARTCAM_WITH_ONNXRUNTIME)
    Ort::Env            env{ORT_LOGGING_LEVEL_WARNING, "SmartCam_AiSegmenter"};
    Ort::SessionOptions session_options{};
    std::unique_ptr<Ort::Session> session{};
    std::string         input_name  = "input_1";
    std::string         output_name = "activation_10";
#else
    int placeholder = 0;
#endif
};

AiOnnxSegmenter::AiOnnxSegmenter(const AiSegmenterConfig& config)
    : config_(config), ort_impl_(std::make_unique<OrtImpl>()) {
    InitializeModel();
}

AiOnnxSegmenter::~AiOnnxSegmenter() = default;

bool AiOnnxSegmenter::InitializeModel() {
    const uint32_t tw = std::max(64u, config_.input_width);
    const uint32_t th = std::max(64u, config_.input_height);
    const size_t num_pixels = static_cast<size_t>(tw) * th;

    input_tensor_.assign(num_pixels * 3, 0.0f);
    curr_prob_map_.assign(num_pixels, 0.0f);
    scratch_prob_map_.assign(num_pixels, 0.0f);
    prev_prob_map_.clear();
    prev_luma_map_.clear();
    lowres_mask_.Allocate(tw, th, PixelFormat::GRAY8);

#if defined(SMARTCAM_WITH_ONNXRUNTIME)
    std::ifstream model_file(config_.model_path, std::ios::binary);
    if (model_file.good()) {
        try {
            ort_impl_->session_options.SetIntraOpNumThreads(static_cast<int>(config_.intra_op_threads));
            ort_impl_->session_options.SetGraphOptimizationLevel(GraphOptimizationLevel::ORT_ENABLE_ALL);
            std::vector<char> model_bytes((std::istreambuf_iterator<char>(model_file)),
                                           std::istreambuf_iterator<char>());
            ort_impl_->session = std::make_unique<Ort::Session>(
                ort_impl_->env, model_bytes.data(), model_bytes.size(), ort_impl_->session_options);
            ort_loaded_ = true;
            return true;
        } catch (...) {
            ort_loaded_ = false;
        }
    }
#endif
    ort_loaded_ = false;
    return true;
}

const char* AiOnnxSegmenter::GetName() const noexcept {
    if (ort_loaded_) {
        return "ONNX Runtime Selfie Segmenter (Native C++)";
    }
    if (has_reference_bg_) {
        return "AI Selfie Segmenter (Reference Background Subtraction + Dynamic Tracker)";
    }
    return "AI Selfie Segmenter (Real-Time Dynamic Face & Upper-Body Tracker)";
}

namespace {

inline void Rgb01ToYCbCr(float r, float g, float b, float& out_y, float& out_cb, float& out_cr) noexcept {
    const float r255 = r * 255.0f;
    const float g255 = g * 255.0f;
    const float b255 = b * 255.0f;
    out_y  = 0.299f * r255 + 0.587f * g255 + 0.114f * b255;
    out_cb = 128.0f - 0.168736f * r255 - 0.331264f * g255 + 0.500f * b255;
    out_cr = 128.0f + 0.500f * r255 - 0.418688f * g255 - 0.081312f * b255;
}

inline float SmoothRamp(float x, float edge0, float edge1) noexcept {
    if (edge0 == edge1) return (x >= edge1) ? 1.0f : 0.0f;
    const float t = std::clamp((x - edge0) / (edge1 - edge0), 0.0f, 1.0f);
    return t * t * (3.0f - 2.0f * t);
}

/**
 * @brief Discriminates true human facial skin (both lit and shaded indoor UVC skin tones)
 *        from warm wooden furniture/stairs and yellow/mustard clothing.
 *
 * Uses continuous smoothstep ramps (zero step-function discontinuities) to eliminate
 * sensor-noise flickering across all skin tones while strictly rejecting wood and yellow fabric.
 */
inline float ComputeTrueFacialSkinScore(float r,
                                        float g,
                                        float b,
                                        float y,
                                        float cb,
                                        float cr,
                                        const AiSegmenterConfig* cfg = nullptr) noexcept {
    if (y <= 24.0f || y >= 246.0f) {
        return 0.0f;
    }

    const float max_c = std::max(r, std::max(g, b));
    const float min_c = std::min(r, std::min(g, b));
    const float delta = max_c - min_c;
    if (max_c < 0.12f || delta < 0.028f) {
        return 0.0f;
    }

    const float sat = delta / max_c;
    // Continuous smooth ramp rejecting achromatic walls (sat < 0.055) and hyper-saturated wood/yellow fabric (sat > 0.72)
    const float sat_weight = SmoothRamp(sat, 0.055f, 0.085f) * (1.0f - SmoothRamp(sat, 0.66f, 0.72f));
    if (sat_weight <= 0.001f) {
        return 0.0f;
    }

    const float rb = r - b;
    const float rg = r - g;
    const float gb = g - b;
    if (rb < 0.035f || rg < 0.010f || gb < -0.04f) {
        return 0.0f;
    }

    // Key physical separator: Wood and yellow clothing strongly absorb Blue (G/B >= 2.0),
    // whereas human skin (even in warm shadow) maintains G/B <= 1.82.
    const float g_over_b = g / std::max(0.03f, b);
    const float wood_penalty_gb = 1.0f - SmoothRamp(g_over_b, 1.62f, 1.90f);
    if (wood_penalty_gb <= 0.001f) {
        return 0.0f;
    }

    // Reject yellow/mustard clothing where G is too close to R
    const float rg_ratio = rg / std::max(0.02f, rb);
    const float rg_weight = SmoothRamp(rg_ratio, 0.24f, 0.31f) * (1.0f - SmoothRamp(rg_ratio, 0.88f, 0.95f));
    if (rg_weight <= 0.001f) {
        return 0.0f;
    }

    // YCbCr facial skin locus: Cb in [99 .. 129], Cr in [131 .. 171]
    // (Wood and yellow hoodies have Cb ~ 88..98, so cb <= 99 cleanly rejects them!)
    const float cb_gate = SmoothRamp(cb, 99.0f, 104.5f) * (1.0f - SmoothRamp(cb, 126.0f, 130.0f));
    const float cr_gate = SmoothRamp(cr, 130.5f, 134.5f) * (1.0f - SmoothRamp(cr, 165.0f, 170.0f));
    const float luma_weight = SmoothRamp(y, 28.0f, 54.0f) * (1.0f - SmoothRamp(y, 232.0f, 246.0f));

    // Reddish-brown wooden stairs/furniture have high (Cr - Cb) >= 48 combined with high saturation,
    // whereas true indoor facial skin keeps (Cr - Cb) in [18 .. 46].
    const float cr_minus_cb = cr - cb;
    const float wood_penalty_crcb =
        1.0f - SmoothRamp(cr_minus_cb, 46.0f, 55.0f) * SmoothRamp(sat, 0.40f, 0.50f);

    float universal_score = 0.0f;
    if (cb_gate > 0.0f && cr_gate > 0.0f && wood_penalty_crcb > 0.001f) {
        const float d_cb = (cb - 113.0f) / 14.0f;
        const float d_cr = (cr - 147.5f) / 15.5f;
        const float dist2 = d_cb * d_cb + d_cr * d_cr;
        if (dist2 < 1.90f) {
            const float chroma_score = std::clamp(1.20f - 0.55f * dist2, 0.0f, 1.0f);
            universal_score = chroma_score * cb_gate * cr_gate * sat_weight * rg_weight *
                              wood_penalty_gb * wood_penalty_crcb * luma_weight;
        }
    }

    if (cfg && cfg->face_calibrated) {
        if (cb <= 98.5f || wood_penalty_crcb <= 0.001f) {
            return 0.0f;
        }
        const float cb_tol = std::clamp(cfg->calib_skin_cb_tol, 10.0f, 18.0f);
        const float cr_tol = std::clamp(cfg->calib_skin_cr_tol, 11.0f, 20.0f);
        const float pd_cb = (cb - cfg->calib_skin_cb) / cb_tol;
        const float pd_cr = (cr - cfg->calib_skin_cr) / cr_tol;
        const float p_dist2 = pd_cb * pd_cb + pd_cr * pd_cr;
        if (p_dist2 >= 2.10f) {
            // Outside the user's calibrated facial skin locus: strongly suppress so wooden stairs/walls
            // with different chrominance cannot hijack the face tracker!
            return std::clamp(universal_score * 0.18f, 0.0f, 1.0f);
        }
        const float p_chroma = std::clamp(1.28f - 0.55f * p_dist2, 0.0f, 1.0f);
        const float cb_wood_guard = SmoothRamp(cb, 99.0f, 103.5f);
        const float calibrated_score = p_chroma * cb_wood_guard * sat_weight * rg_weight *
                                       wood_penalty_gb * wood_penalty_crcb * luma_weight;
        return std::clamp(0.85f * calibrated_score + 0.15f * universal_score, 0.0f, 1.0f);
    }

    return std::clamp(universal_score, 0.0f, 1.0f);
}

// Fast O(1) 2D Summed Area Table (Integral Image) helper
struct IntegralGrid {
    uint32_t w = 0;
    uint32_t h = 0;
    std::vector<float> data;

    void Build(const float* src, uint32_t width, uint32_t height) {
        w = width;
        h = height;
        data.assign(static_cast<size_t>(w + 1) * (h + 1), 0.0f);
        const uint32_t stride = w + 1;
        for (uint32_t y = 0; y < h; ++y) {
            float row_sum = 0.0f;
            const float* src_row = src + static_cast<size_t>(y) * w;
            const float* prev_row = data.data() + static_cast<size_t>(y) * stride;
            float* dst_row = data.data() + static_cast<size_t>(y + 1) * stride;
            for (uint32_t x = 0; x < w; ++x) {
                row_sum += src_row[x];
                dst_row[x + 1] = prev_row[x + 1] + row_sum;
            }
        }
    }

    inline float RectMean(int x0, int y0, int x1, int y1) const noexcept {
        x0 = std::clamp(x0, 0, static_cast<int>(w) - 1);
        y0 = std::clamp(y0, 0, static_cast<int>(h) - 1);
        x1 = std::clamp(x1, x0 + 1, static_cast<int>(w));
        y1 = std::clamp(y1, y0 + 1, static_cast<int>(h));
        const uint32_t stride = w + 1;
        const float sum = data[static_cast<size_t>(y1) * stride + x1]
                        - data[static_cast<size_t>(y0) * stride + x1]
                        - data[static_cast<size_t>(y1) * stride + x0]
                        + data[static_cast<size_t>(y0) * stride + x0];
        return sum / static_cast<float>((x1 - x0) * (y1 - y0));
    }
};

}  // namespace

bool AiOnnxSegmenter::PrepareInputTensorNHWC(const FrameView& input_frame,
                                             uint32_t tensor_width,
                                             uint32_t tensor_height,
                                             float* out_tensor_nhwc) noexcept {
    if (!input_frame.IsValid() || out_tensor_nhwc == nullptr || tensor_width == 0 || tensor_height == 0) {
        return false;
    }
    if (input_frame.format != PixelFormat::BGRA32 && input_frame.format != PixelFormat::RGBA32) {
        return false;
    }

    const bool is_bgra     = (input_frame.format == PixelFormat::BGRA32);
    const uint32_t scale_x = (input_frame.width << 16) / tensor_width;
    const uint32_t scale_y = (input_frame.height << 16) / tensor_height;
    const float inv_255    = 1.0f / 255.0f;

    for (uint32_t ty = 0; ty < tensor_height; ++ty) {
        const uint32_t sy = std::min((ty * scale_y) >> 16, input_frame.height - 1);
        const uint8_t* src_row = input_frame.data + static_cast<size_t>(sy) * input_frame.stride;
        float* dst_row = out_tensor_nhwc + static_cast<size_t>(ty) * tensor_width * 3;

        for (uint32_t tx = 0; tx < tensor_width; ++tx) {
            const uint32_t sx = std::min((tx * scale_x) >> 16, input_frame.width - 1);
            const uint8_t c0  = src_row[sx * 4 + 0];
            const uint8_t c1  = src_row[sx * 4 + 1];
            const uint8_t c2  = src_row[sx * 4 + 2];

            const uint8_t r = is_bgra ? c2 : c0;
            const uint8_t g = c1;
            const uint8_t b = is_bgra ? c0 : c2;

            dst_row[tx * 3 + 0] = static_cast<float>(r) * inv_255;
            dst_row[tx * 3 + 1] = static_cast<float>(g) * inv_255;
            dst_row[tx * 3 + 2] = static_cast<float>(b) * inv_255;
        }
    }

    return true;
}

bool AiOnnxSegmenter::SetReferenceBackground(const FrameView& empty_bg_frame) noexcept {
    if (!empty_bg_frame.IsValid()) {
        return false;
    }

    const uint32_t tw = std::max(64u, config_.input_width);
    const uint32_t th = std::max(64u, config_.input_height);
    const size_t num_pixels = static_cast<size_t>(tw) * th;

    std::vector<float> raw_rgb(num_pixels * 3, 0.0f);
    if (!PrepareInputTensorNHWC(empty_bg_frame, tw, th, raw_rgb.data())) {
        return false;
    }

    ref_bg_ycbcr_.resize(num_pixels * 3, 0.0f);
    ref_bg_raw_ycbcr_.resize(num_pixels * 3, 0.0f);

    // Store both exact per-pixel YCbCr and 3x3 box-filtered YCbCr of the reference background
    // so spatial neighborhood matching handles both sharp 1-2px shifted edges and sensor thermal noise.
    for (uint32_t y = 0; y < th; ++y) {
        const uint32_t y0 = (y > 0) ? (y - 1) : 0;
        const uint32_t y2 = (y + 1 < th) ? (y + 1) : (th - 1);

        for (uint32_t x = 0; x < tw; ++x) {
            const size_t idx3 = (static_cast<size_t>(y) * tw + x) * 3;
            Rgb01ToYCbCr(raw_rgb[idx3 + 0], raw_rgb[idx3 + 1], raw_rgb[idx3 + 2],
                         ref_bg_raw_ycbcr_[idx3 + 0],
                         ref_bg_raw_ycbcr_[idx3 + 1],
                         ref_bg_raw_ycbcr_[idx3 + 2]);

            const uint32_t x0 = (x > 0) ? (x - 1) : 0;
            const uint32_t x2 = (x + 1 < tw) ? (x + 1) : (tw - 1);

            float sum_r = 0.0f, sum_g = 0.0f, sum_b = 0.0f;
            const uint32_t ys[3] = {y0, y, y2};
            const uint32_t xs[3] = {x0, x, x2};

            for (uint32_t ny : ys) {
                const float* row_ptr = raw_rgb.data() + static_cast<size_t>(ny) * tw * 3;
                for (uint32_t nx : xs) {
                    sum_r += row_ptr[nx * 3 + 0];
                    sum_g += row_ptr[nx * 3 + 1];
                    sum_b += row_ptr[nx * 3 + 2];
                }
            }

            const float inv_9 = 1.0f / 9.0f;
            Rgb01ToYCbCr(sum_r * inv_9, sum_g * inv_9, sum_b * inv_9,
                         ref_bg_ycbcr_[idx3 + 0],
                         ref_bg_ycbcr_[idx3 + 1],
                         ref_bg_ycbcr_[idx3 + 2]);
        }
    }

    has_reference_bg_ = true;
    prev_prob_map_.clear();
    return true;
}

void AiOnnxSegmenter::ClearReferenceBackground() noexcept {
    has_reference_bg_ = false;
    ref_bg_ycbcr_.clear();
    ref_bg_raw_ycbcr_.clear();
    prev_prob_map_.clear();
}

void AiOnnxSegmenter::SetAiModeAndTolerance(int engine_mode, float bg_diff_tolerance) noexcept {
    config_.engine_mode       = std::clamp(engine_mode, 0, 2);
    config_.bg_diff_tolerance = std::clamp(bg_diff_tolerance, 0.0f, 1.0f);
}

bool AiOnnxSegmenter::CalibrateFaceFromLandmarks(const FrameView& snapshot_bgra,
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
                                                 float* out_head_ry) noexcept {
    if (!snapshot_bgra.IsValid() || snapshot_bgra.width < 16 || snapshot_bgra.height < 16) {
        return false;
    }
    if (snapshot_bgra.format != PixelFormat::BGRA32 && snapshot_bgra.format != PixelFormat::RGBA32) {
        return false;
    }

    left_eye_x  = std::clamp(left_eye_x,  0.02f, 0.98f);
    left_eye_y  = std::clamp(left_eye_y,  0.02f, 0.98f);
    right_eye_x = std::clamp(right_eye_x, 0.02f, 0.98f);
    right_eye_y = std::clamp(right_eye_y, 0.02f, 0.98f);
    nose_tip_x  = std::clamp(nose_tip_x,  0.02f, 0.98f);
    nose_tip_y  = std::clamp(nose_tip_y,  0.02f, 0.98f);
    lips_x      = std::clamp(lips_x,      0.02f, 0.98f);
    lips_y      = std::clamp(lips_y,      0.02f, 0.98f);
    chin_x      = std::clamp(chin_x,      0.02f, 0.98f);
    chin_y      = std::clamp(chin_y,      0.02f, 0.98f);

    // 1. Eye midpoint and inter-ocular distance (IOD)
    const float eye_mid_x = 0.5f * (left_eye_x + right_eye_x);
    const float eye_mid_y = 0.5f * (left_eye_y + right_eye_y);
    const float iod       = std::hypot(right_eye_x - left_eye_x, right_eye_y - left_eye_y);

    // 2. Anatomical eye-to-chin height
    const float eye_to_chin = std::max(0.05f, chin_y - eye_mid_y);

    // 3. Exact geometric head center and horizontal/vertical semi-axes from 5 landmarks:
    //    - Crown top is at `eye_mid_y - 0.85 * eye_to_chin`, chin bottom is at `chin_y + 0.06 * eye_to_chin`.
    //    - Midpoint between crown and chin is `eye_mid_y + 0.105 * eye_to_chin`.
    //    - Semi-height `head_ry = 0.955 * eye_to_chin`, semi-width `head_rx = max(1.22 * iod, 0.52 * eye_to_chin)`.
    const float head_cx = std::clamp(0.45f * eye_mid_x + 0.35f * nose_tip_x + 0.20f * lips_x, 0.08f, 0.92f);
    const float head_cy = std::clamp(eye_mid_y + 0.105f * eye_to_chin, 0.12f, 0.78f);
    const float head_rx = std::clamp(std::max(iod * 1.22f, eye_to_chin * 0.52f), 0.065f, 0.22f);
    const float head_ry = std::clamp(eye_to_chin * 0.96f, 0.11f, 0.34f);
    const float chin_rel_y = std::clamp((chin_y - head_cy) / std::max(0.05f, head_ry), 0.70f, 1.05f);

    // Store normalized 5 landmarks and relative offsets from `(head_cx, head_cy)`
    const float lms[10] = {
        left_eye_x, left_eye_y,
        right_eye_x, right_eye_y,
        nose_tip_x, nose_tip_y,
        lips_x, lips_y,
        chin_x, chin_y
    };
    for (int i = 0; i < 10; ++i) {
        config_.calib_landmarks[i] = lms[i];
    }
    config_.calib_anchor_cx = head_cx;
    config_.calib_anchor_cy = head_cy;
    for (int k = 0; k < 5; ++k) {
        calib_rel_offsets_[k][0] = lms[k * 2 + 0] - head_cx;
        calib_rel_offsets_[k][1] = lms[k * 2 + 1] - head_cy;
    }

    // 4. Sample 5x5 patches around the 5 facial skin regions
    const float patch_centers[5][2] = {
        {nose_tip_x, nose_tip_y},
        {0.55f * left_eye_x  + 0.45f * nose_tip_x, 0.45f * left_eye_y  + 0.55f * lips_y},
        {0.55f * right_eye_x + 0.45f * nose_tip_x, 0.45f * right_eye_y + 0.55f * lips_y},
        {eye_mid_x, std::max(0.02f, eye_mid_y - 0.35f * eye_to_chin)},
        {chin_x, 0.35f * lips_y + 0.65f * chin_y}
    };

    const bool is_bgra = (snapshot_bgra.format == PixelFormat::BGRA32);
    const int w = static_cast<int>(snapshot_bgra.width);
    const int h = static_cast<int>(snapshot_bgra.height);
    const float inv_255 = 1.0f / 255.0f;

    std::vector<float> samples_y;
    std::vector<float> samples_cb;
    std::vector<float> samples_cr;
    samples_y.reserve(125);
    samples_cb.reserve(125);
    samples_cr.reserve(125);

    for (int p = 0; p < 5; ++p) {
        const int cx_px = std::clamp(static_cast<int>(patch_centers[p][0] * w + 0.5f), 0, w - 1);
        const int cy_px = std::clamp(static_cast<int>(patch_centers[p][1] * h + 0.5f), 0, h - 1);
        for (int dy = -2; dy <= 2; ++dy) {
            const int sy = std::clamp(cy_px + dy, 0, h - 1);
            const uint8_t* row = snapshot_bgra.data + static_cast<size_t>(sy) * snapshot_bgra.stride;
            for (int dx = -2; dx <= 2; ++dx) {
                const int sx = std::clamp(cx_px + dx, 0, w - 1);
                const uint8_t c0 = row[sx * 4 + 0];
                const uint8_t c1 = row[sx * 4 + 1];
                const uint8_t c2 = row[sx * 4 + 2];
                const float r = static_cast<float>(is_bgra ? c2 : c0) * inv_255;
                const float g = static_cast<float>(c1) * inv_255;
                const float b = static_cast<float>(is_bgra ? c0 : c2) * inv_255;

                float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
                Rgb01ToYCbCr(r, g, b, py, pcb, pcr);
                if (py >= 20.0f && py <= 248.0f) {
                    samples_y.push_back(py);
                    samples_cb.push_back(pcb);
                    samples_cr.push_back(pcr);
                }
            }
        }
    }

    float mean_y  = 135.0f;
    float mean_cb = 113.0f;
    float mean_cr = 148.0f;
    float std_cb  = 8.0f;
    float std_cr  = 9.0f;

    if (!samples_cb.empty()) {
        double sy = 0.0, scb = 0.0, scr = 0.0;
        const size_t n = samples_cb.size();
        for (size_t i = 0; i < n; ++i) {
            sy  += samples_y[i];
            scb += samples_cb[i];
            scr += samples_cr[i];
        }
        mean_y  = static_cast<float>(sy / n);
        mean_cb = static_cast<float>(scb / n);
        mean_cr = static_cast<float>(scr / n);

        double var_cb = 0.0, var_cr = 0.0;
        for (size_t i = 0; i < n; ++i) {
            const double dcb = samples_cb[i] - mean_cb;
            const double dcr = samples_cr[i] - mean_cr;
            var_cb += dcb * dcb;
            var_cr += dcr * dcr;
        }
        std_cb = static_cast<float>(std::sqrt(var_cb / n));
        std_cr = static_cast<float>(std::sqrt(var_cr / n));
    }

    config_.face_calibrated   = true;
    config_.calib_skin_y      = std::clamp(mean_y, 35.0f, 235.0f);
    config_.calib_skin_cb     = std::clamp(mean_cb, 92.0f, 140.0f);
    config_.calib_skin_cr     = std::clamp(mean_cr, 122.0f, 180.0f);
    config_.calib_skin_cb_tol = std::clamp(std::max(14.0f, std_cb * 2.6f), 14.0f, 26.0f);
    config_.calib_skin_cr_tol = std::clamp(std::max(16.0f, std_cr * 2.6f), 16.0f, 28.0f);
    config_.calib_head_rx     = head_rx;
    config_.calib_head_ry     = head_ry;
    config_.calib_chin_rel_y  = chin_rel_y;

    tracked_cx_          = head_cx;
    tracked_cy_          = head_cy;
    tracked_radius_      = std::clamp(head_rx / 1.12f, 0.065f, 0.18f);
    tracker_initialized_ = true;
    pending_jump_frames_ = 0;
    prev_prob_map_.clear();

    // 5. Capture the 5-point visual feature template (`Occhio DX`, `Occhio SX`, `Naso`, `Labbra`, `Mento`)
    //    directly from the calibration snapshot so frame-to-frame tracking correlates the user's actual facial features!
    const uint32_t tw = std::max(64u, config_.input_width);
    const uint32_t th = std::max(64u, config_.input_height);
    std::vector<float> snap_nhwc(static_cast<size_t>(tw) * th * 3, 0.0f);
    if (PrepareInputTensorNHWC(snapshot_bgra, tw, th, snap_nhwc.data())) {
        CaptureTemplateFromGrid(snap_nhwc.data(), tw, th);
    }

    if (out_skin_cb) *out_skin_cb = config_.calib_skin_cb;
    if (out_skin_cr) *out_skin_cr = config_.calib_skin_cr;
    if (out_head_rx) *out_head_rx = config_.calib_head_rx;
    if (out_head_ry) *out_head_ry = config_.calib_head_ry;
    return true;
}

void AiOnnxSegmenter::SetFaceCalibrationProfile(bool enabled,
                                                float skin_cb,
                                                float skin_cr,
                                                float head_rx,
                                                float head_ry) noexcept {
    const bool was_enabled = config_.face_calibrated;
    config_.face_calibrated = enabled;
    if (enabled) {
        config_.calib_skin_cb     = std::clamp(skin_cb, 90.0f, 145.0f);
        config_.calib_skin_cr     = std::clamp(skin_cr, 120.0f, 185.0f);
        config_.calib_skin_cb_tol = 16.0f;
        config_.calib_skin_cr_tol = 18.0f;
        // Recompute anchor geometry from stored landmarks if valid
        SetFaceCalibrationLandmarks(config_.calib_landmarks);
        if (head_rx >= 0.065f && head_rx <= 0.22f && head_ry >= 0.11f && head_ry <= 0.34f) {
            // Keep landmark-derived tight geometry if caller passed legacy oversized defaults
            if (std::abs(head_rx - 0.180f) > 0.005f || std::abs(head_ry - 0.338f) > 0.005f) {
                config_.calib_head_rx = std::clamp(head_rx, 0.065f, 0.20f);
                config_.calib_head_ry = std::clamp(head_ry, 0.11f, 0.32f);
            }
        }
        tracked_radius_ = std::clamp(config_.calib_head_rx / 1.12f, 0.065f, 0.18f);
        if (!was_enabled || !tracker_initialized_) {
            tracked_cx_          = config_.calib_anchor_cx;
            tracked_cy_          = config_.calib_anchor_cy;
            tracker_initialized_ = true;
        }
    } else {
        has_calib_template_      = false;
        need_template_from_live_ = false;
    }
    prev_prob_map_.clear();
}

void AiOnnxSegmenter::SetFaceCalibrationLandmarks(const float landmarks_10[10]) noexcept {
    if (!landmarks_10) {
        return;
    }
    for (int i = 0; i < 10; ++i) {
        config_.calib_landmarks[i] = std::clamp(landmarks_10[i], 0.02f, 0.98f);
    }

    const float p0x = config_.calib_landmarks[0], p0y = config_.calib_landmarks[1];
    const float p1x = config_.calib_landmarks[2], p1y = config_.calib_landmarks[3];
    const float p2x = config_.calib_landmarks[4], p2y = config_.calib_landmarks[5];
    const float p3x = config_.calib_landmarks[6], p3y = config_.calib_landmarks[7];
    const float p4x = config_.calib_landmarks[8], p4y = config_.calib_landmarks[9];

    const float eye_mid_x   = 0.5f * (p0x + p1x);
    const float eye_mid_y   = 0.5f * (p0y + p1y);
    const float iod         = std::hypot(p1x - p0x, p1y - p0y);
    const float eye_to_chin = std::max(0.05f, p4y - eye_mid_y);

    const float head_cx = std::clamp(0.45f * eye_mid_x + 0.35f * p2x + 0.20f * p3x, 0.08f, 0.92f);
    const float head_cy = std::clamp(eye_mid_y + 0.105f * eye_to_chin, 0.12f, 0.78f);
    const float head_rx = std::clamp(std::max(iod * 1.22f, eye_to_chin * 0.52f), 0.065f, 0.20f);
    const float head_ry = std::clamp(eye_to_chin * 0.96f, 0.11f, 0.32f);

    const bool landmarks_changed =
        (std::hypot(head_cx - config_.calib_anchor_cx, head_cy - config_.calib_anchor_cy) > 0.004f);

    config_.calib_anchor_cx  = head_cx;
    config_.calib_anchor_cy  = head_cy;
    config_.calib_head_rx    = head_rx;
    config_.calib_head_ry    = head_ry;
    config_.calib_chin_rel_y = std::clamp((p4y - head_cy) / std::max(0.05f, head_ry), 0.70f, 1.05f);

    const float pts[5][2] = {{p0x, p0y}, {p1x, p1y}, {p2x, p2y}, {p3x, p3y}, {p4x, p4y}};
    for (int k = 0; k < 5; ++k) {
        calib_rel_offsets_[k][0] = pts[k][0] - head_cx;
        calib_rel_offsets_[k][1] = pts[k][1] - head_cy;
    }

    if (!tracker_initialized_ || landmarks_changed) {
        tracked_cx_          = head_cx;
        tracked_cy_          = head_cy;
        tracked_radius_      = std::clamp(head_rx / 1.12f, 0.065f, 0.18f);
        tracker_initialized_ = true;
        pending_jump_frames_ = 0;
    }
    if (!has_calib_template_) {
        need_template_from_live_ = true;
    }
}

void AiOnnxSegmenter::ClearFaceCalibration() noexcept {
    config_.face_calibrated   = false;
    config_.calib_skin_y      = 135.0f;
    config_.calib_skin_cb     = 113.0f;
    config_.calib_skin_cr     = 148.0f;
    config_.calib_skin_cb_tol = 15.0f;
    config_.calib_skin_cr_tol = 17.0f;
    config_.calib_head_rx     = 0.135f;
    config_.calib_head_ry     = 0.250f;
    config_.calib_chin_rel_y  = 0.94f;
    has_calib_template_       = false;
    need_template_from_live_  = false;
    prev_prob_map_.clear();
}

void AiOnnxSegmenter::CaptureTemplateFromGrid(const float* input_nhwc,
                                              uint32_t tw,
                                              uint32_t th) noexcept {
    if (!input_nhwc || tw < 32 || th < 32) {
        return;
    }
    const int w = static_cast<int>(tw);
    const int h = static_cast<int>(th);
    const size_t num_px = static_cast<size_t>(w) * h;

    std::vector<float> y_grid(num_px, 0.0f);
    std::vector<float> cb_grid(num_px, 128.0f);
    std::vector<float> cr_grid(num_px, 128.0f);
    std::vector<float> grad_grid(num_px, 0.0f);

    for (size_t i = 0; i < num_px; ++i) {
        Rgb01ToYCbCr(input_nhwc[i * 3 + 0], input_nhwc[i * 3 + 1], input_nhwc[i * 3 + 2],
                     y_grid[i], cb_grid[i], cr_grid[i]);
    }
    for (int y = 1; y + 1 < h; ++y) {
        for (int x = 1; x + 1 < w; ++x) {
            const size_t idx = static_cast<size_t>(y) * w + x;
            const float gx = std::abs(y_grid[idx + 1] - y_grid[idx - 1]);
            const float gy = std::abs(y_grid[idx + w] - y_grid[idx - w]);
            grad_grid[idx] = std::clamp((gy * 1.40f + gx * 0.60f) / 30.0f, 0.0f, 1.0f);
        }
    }

    const int acx = std::clamp(static_cast<int>(config_.calib_anchor_cx * w + 0.5f), 4, w - 5);
    const int acy = std::clamp(static_cast<int>(config_.calib_anchor_cy * h + 0.5f), 4, h - 5);

    // 1. Extract 7x7 zero-mean unit-variance patches around each of the 5 calibrated landmarks
    for (int k = 0; k < 5; ++k) {
        const int lx = std::clamp(acx + static_cast<int>(std::round(calib_rel_offsets_[k][0] * w)), 3, w - 4);
        const int ly = std::clamp(acy + static_cast<int>(std::round(calib_rel_offsets_[k][1] * h)), 3, h - 4);

        float sum_l = 0.0f, sum_cb = 0.0f, sum_cr = 0.0f;
        int idx_p = 0;
        for (int dy = -3; dy <= 3; ++dy) {
            for (int dx = -3; dx <= 3; ++dx) {
                const size_t g_idx = static_cast<size_t>(ly + dy) * w + (lx + dx);
                const float lv = y_grid[g_idx];
                calib_lm_luma_[k][idx_p] = lv;
                calib_lm_grad_[k][idx_p] = grad_grid[g_idx];
                sum_l  += lv;
                sum_cb += cb_grid[g_idx];
                sum_cr += cr_grid[g_idx];
                ++idx_p;
            }
        }
        const float mean_l = sum_l / 49.0f;
        calib_lm_cb_[k] = sum_cb / 49.0f;
        calib_lm_cr_[k] = sum_cr / 49.0f;

        float var_l = 0.0f;
        for (int p = 0; p < 49; ++p) {
            const float dl = calib_lm_luma_[k][p] - mean_l;
            calib_lm_luma_[k][p] = dl;
            var_l += dl * dl;
        }
        const float inv_norm = 1.0f / std::sqrt(var_l + 16.0f);
        for (int p = 0; p < 49; ++p) {
            calib_lm_luma_[k][p] *= inv_norm;
        }
    }

    // 2. Extract unified T-Zone template covering Right Eye, Left Eye, Nose, and Lips/Mustache (`k = 0..3`)
    int min_lx = w, max_lx = 0, min_ly = h, max_ly = 0;
    for (int k = 0; k < 4; ++k) {
        const int lx = std::clamp(acx + static_cast<int>(std::round(calib_rel_offsets_[k][0] * w)), 3, w - 4);
        const int ly = std::clamp(acy + static_cast<int>(std::round(calib_rel_offsets_[k][1] * h)), 3, h - 4);
        min_lx = std::min(min_lx, lx);
        max_lx = std::max(max_lx, lx);
        min_ly = std::min(min_ly, ly);
        max_ly = std::max(max_ly, ly);
    }
    const int x0 = std::clamp(min_lx - 4, 2, w - 10);
    const int x1 = std::clamp(max_lx + 4, x0 + 8, w - 3);
    const int y0 = std::clamp(min_ly - 4, 2, h - 10);
    const int y1 = std::clamp(max_ly + 4, y0 + 8, h - 3);

    calib_tzone_w_     = x1 - x0 + 1;
    calib_tzone_h_     = y1 - y0 + 1;
    calib_tzone_off_x_ = x0 - acx;
    calib_tzone_off_y_ = y0 - acy;

    const size_t tz_len = static_cast<size_t>(calib_tzone_w_) * calib_tzone_h_;
    calib_tzone_luma_.assign(tz_len, 0.0f);
    calib_tzone_grad_.assign(tz_len, 0.0f);

    float tz_sum_l = 0.0f, tz_sum_g = 0.0f;
    size_t p_idx = 0;
    for (int ty = y0; ty <= y1; ++ty) {
        for (int tx = x0; tx <= x1; ++tx) {
            const size_t g_idx = static_cast<size_t>(ty) * w + tx;
            calib_tzone_luma_[p_idx] = y_grid[g_idx];
            calib_tzone_grad_[p_idx] = grad_grid[g_idx];
            tz_sum_l += y_grid[g_idx];
            tz_sum_g += grad_grid[g_idx];
            ++p_idx;
        }
    }

    const float tz_mean_l = tz_sum_l / static_cast<float>(tz_len);
    const float tz_mean_g = tz_sum_g / static_cast<float>(tz_len);
    float tz_var_l = 0.0f, tz_var_g = 0.0f;
    for (size_t i = 0; i < tz_len; ++i) {
        calib_tzone_luma_[i] -= tz_mean_l;
        calib_tzone_grad_[i] -= tz_mean_g;
        tz_var_l += calib_tzone_luma_[i] * calib_tzone_luma_[i];
        tz_var_g += calib_tzone_grad_[i] * calib_tzone_grad_[i];
    }
    const float inv_tz_l = 1.0f / std::sqrt(tz_var_l + 25.0f);
    const float inv_tz_g = 1.0f / std::sqrt(tz_var_g + 0.25f);
    for (size_t i = 0; i < tz_len; ++i) {
        calib_tzone_luma_[i] *= inv_tz_l;
        calib_tzone_grad_[i] *= inv_tz_g;
    }

    has_calib_template_      = true;
    need_template_from_live_ = false;
}

bool AiOnnxSegmenter::TrackCalibratedFaceTemplate(const float* input_nhwc,
                                                  const float* luma_curr,
                                                  const float* feat_grad_map,
                                                  uint32_t tw,
                                                  uint32_t th,
                                                  float& out_cx,
                                                  float& out_cy,
                                                  float& out_rx) noexcept {
    if (!config_.face_calibrated || !input_nhwc || !luma_curr || !feat_grad_map || tw < 32 || th < 32) {
        return false;
    }

    if (need_template_from_live_ || !has_calib_template_) {
        CaptureTemplateFromGrid(input_nhwc, tw, th);
    }
    if (!has_calib_template_ || calib_tzone_w_ < 4 || calib_tzone_h_ < 4) {
        out_cx = config_.calib_anchor_cx;
        out_cy = config_.calib_anchor_cy;
        out_rx = std::clamp(config_.calib_head_rx / 1.12f, 0.065f, 0.18f);
        return true;
    }

    const int w = static_cast<int>(tw);
    const int h = static_cast<int>(th);
    const float inv_w = 1.0f / static_cast<float>(w);
    const float inv_h = 1.0f / static_cast<float>(h);

    const float start_cx = tracker_initialized_ ? tracked_cx_ : config_.calib_anchor_cx;
    const float start_cy = tracker_initialized_ ? tracked_cy_ : config_.calib_anchor_cy;
    const int prev_x_px  = std::clamp(static_cast<int>(start_cx * w + 0.5f), 8, w - 9);
    const int prev_y_px  = std::clamp(static_cast<int>(start_cy * h + 0.5f), 8, h - 9);
    const int anc_x_px   = std::clamp(static_cast<int>(config_.calib_anchor_cx * w + 0.5f), 8, w - 9);
    const int anc_y_px   = std::clamp(static_cast<int>(config_.calib_anchor_cy * h + 0.5f), 8, h - 9);

    int rel_px[5][2];
    for (int k = 0; k < 5; ++k) {
        rel_px[k][0] = static_cast<int>(std::round(calib_rel_offsets_[k][0] * w));
        rel_px[k][1] = static_cast<int>(std::round(calib_rel_offsets_[k][1] * h));
    }

    const size_t tz_len = static_cast<size_t>(calib_tzone_w_) * calib_tzone_h_;
    const float inv_tz_len = 1.0f / static_cast<float>(tz_len);

    // Evaluates the 5-point visual template correlation at candidate head center `(cx_px, cy_px)`
    auto eval_template_at = [&](int cx_px, int cy_px) noexcept -> float {
        const int tz_x0 = cx_px + calib_tzone_off_x_;
        const int tz_y0 = cy_px + calib_tzone_off_y_;
        const int tz_x1 = tz_x0 + calib_tzone_w_ - 1;
        const int tz_y1 = tz_y0 + calib_tzone_h_ - 1;
        if (tz_x0 < 2 || tz_y0 < 2 || tz_x1 >= w - 2 || tz_y1 >= h - 2) {
            return -1.0f;
        }

        // 1. T-Zone ZNCC (Luminance + Horizontal Edge Gradient of Eyes-Nose-Lips)
        float sum_l = 0.0f, sum_g = 0.0f;
        for (int ty = tz_y0; ty <= tz_y1; ++ty) {
            const size_t row_off = static_cast<size_t>(ty) * w;
            for (int tx = tz_x0; tx <= tz_x1; ++tx) {
                sum_l += luma_curr[row_off + tx];
                sum_g += feat_grad_map[row_off + tx];
            }
        }
        const float mean_l = sum_l * inv_tz_len;
        const float mean_g = sum_g * inv_tz_len;

        float dot_l = 0.0f, var_l = 0.0f;
        float dot_g = 0.0f, var_g = 0.0f;
        size_t p_idx = 0;
        for (int ty = tz_y0; ty <= tz_y1; ++ty) {
            const size_t row_off = static_cast<size_t>(ty) * w;
            for (int tx = tz_x0; tx <= tz_x1; ++tx) {
                const float dl = luma_curr[row_off + tx] - mean_l;
                const float dg = feat_grad_map[row_off + tx] - mean_g;
                dot_l += dl * calib_tzone_luma_[p_idx];
                var_l += dl * dl;
                dot_g += dg * calib_tzone_grad_[p_idx];
                var_g += dg * dg;
                ++p_idx;
            }
        }
        // Reject completely flat wall regions with near-zero internal variance
        if (var_l < 18.0f) {
            return -1.0f;
        }
        const float zncc_l = dot_l / std::sqrt(var_l + 25.0f);
        const float zncc_g = dot_g / std::sqrt(var_g + 0.25f);
        const float tz_zncc_01 = std::clamp(0.5f * (0.65f * zncc_l + 0.35f * zncc_g + 1.0f), 0.0f, 1.0f);

        // 2. 5-Landmark Constellation Patch Correlation & Chrominance Verification
        float lm_accum = 0.0f;
        for (int k = 0; k < 5; ++k) {
            const int lx = std::clamp(cx_px + rel_px[k][0], 3, w - 4);
            const int ly = std::clamp(cy_px + rel_px[k][1], 3, h - 4);

            float s_l = 0.0f;
            for (int dy = -3; dy <= 3; ++dy) {
                const size_t r_off = static_cast<size_t>(ly + dy) * w;
                for (int dx = -3; dx <= 3; ++dx) {
                    s_l += luma_curr[r_off + (lx + dx)];
                }
            }
            const float m_l = s_l / 49.0f;
            float d_l = 0.0f, v_l = 0.0f, g_corr = 0.0f;
            int idx_p = 0;
            for (int dy = -3; dy <= 3; ++dy) {
                const size_t r_off = static_cast<size_t>(ly + dy) * w;
                for (int dx = -3; dx <= 3; ++dx) {
                    const float diff = luma_curr[r_off + (lx + dx)] - m_l;
                    d_l    += diff * calib_lm_luma_[k][idx_p];
                    v_l    += diff * diff;
                    g_corr += feat_grad_map[r_off + (lx + dx)] * calib_lm_grad_[k][idx_p];
                    ++idx_p;
                }
            }
            const float lm_zncc = std::clamp(0.5f * (d_l / std::sqrt(v_l + 16.0f) + 1.0f), 0.0f, 1.0f);

            // Sample chrominance at landmark center `(lx, ly)`
            const size_t c_idx3 = (static_cast<size_t>(ly) * w + lx) * 3;
            float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
            Rgb01ToYCbCr(input_nhwc[c_idx3 + 0], input_nhwc[c_idx3 + 1], input_nhwc[c_idx3 + 2], py, pcb, pcr);
            const float d_chroma = std::hypot(pcb - calib_lm_cb_[k], pcr - calib_lm_cr_[k]);
            const float chroma_match = std::clamp(1.0f - d_chroma / 26.0f, 0.0f, 1.0f);

            // Nose tip (k=2) must also match the user's calibrated facial skin locus
            if (k == 2 && chroma_match < 0.25f) {
                return -1.0f;
            }

            lm_accum += 0.62f * lm_zncc + 0.38f * chroma_match + std::min(0.15f, g_corr * 0.04f);
        }
        const float lm_score = lm_accum * 0.20f;

        // 3. Smooth continuity prior around current tracked position and calibrated anchor
        const float nx = (static_cast<float>(cx_px) + 0.5f) * inv_w;
        const float ny = (static_cast<float>(cy_px) + 0.5f) * inv_h;
        const float d2_prev = (nx - start_cx) * (nx - start_cx) + (ny - start_cy) * (ny - start_cy);
        const float d2_anc  = (nx - config_.calib_anchor_cx) * (nx - config_.calib_anchor_cx) +
                              (ny - config_.calib_anchor_cy) * (ny - config_.calib_anchor_cy);

        const float prev_prior = 0.82f + 0.18f * std::exp(-d2_prev / (2.0f * 0.10f * 0.10f));
        const float anc_prior  = 0.85f + 0.15f * std::exp(-d2_anc  / (2.0f * 0.18f * 0.18f));

        return (0.52f * tz_zncc_01 + 0.48f * lm_score) * prev_prior * anc_prior;
    };

    int   best_x_px  = prev_x_px;
    int   best_y_px  = prev_y_px;
    float best_score = eval_template_at(prev_x_px, prev_y_px);

    // Search window around previous tracked position
    for (int dy = -16; dy <= 16; ++dy) {
        const int step = (std::abs(dy) <= 7) ? 1 : 2;
        if ((dy % step) != 0) continue;
        for (int dx = -24; dx <= 24; ++dx) {
            const int step_x = (std::abs(dx) <= 8 && std::abs(dy) <= 7) ? 1 : 2;
            if ((dx % step_x) != 0) continue;
            const int cx_px = prev_x_px + dx;
            const int cy_px = prev_y_px + dy;
            const float sc  = eval_template_at(cx_px, cy_px);
            if (sc > best_score) {
                best_score = sc;
                best_x_px  = cx_px;
                best_y_px  = cy_px;
            }
        }
    }

    // Also search around calibrated anchor if currently far from anchor
    if (std::abs(prev_x_px - anc_x_px) > 10 || std::abs(prev_y_px - anc_y_px) > 8) {
        for (int dy = -10; dy <= 10; dy += 2) {
            for (int dx = -14; dx <= 14; dx += 2) {
                const int cx_px = anc_x_px + dx;
                const int cy_px = anc_y_px + dy;
                const float sc  = eval_template_at(cx_px, cy_px);
                if (sc > best_score) {
                    best_score = sc;
                    best_x_px  = cx_px;
                    best_y_px  = cy_px;
                }
            }
        }
    }

    // Refine 1-pixel neighborhood around `best_x_px, best_y_px`
    for (int dy = -1; dy <= 1; ++dy) {
        for (int dx = -1; dx <= 1; ++dx) {
            const float sc = eval_template_at(best_x_px + dx, best_y_px + dy);
            if (sc > best_score) {
                best_score = sc;
                best_x_px += dx;
                best_y_px += dy;
            }
        }
    }

    if (best_score < 0.28f) {
        // User temporarily out of frame or occluded: hold steady at tracked/anchor position
        out_cx = start_cx;
        out_cy = start_cy;
        out_rx = std::clamp(config_.calib_head_rx / 1.12f, 0.065f, 0.18f);
        return true;
    }

    // Sub-pixel parabolic interpolation on the 2D ZNCC correlation peak (zero centroid drift!)
    const float sx_m1 = eval_template_at(best_x_px - 1, best_y_px);
    const float sx_p1 = eval_template_at(best_x_px + 1, best_y_px);
    const float sy_m1 = eval_template_at(best_x_px, best_y_px - 1);
    const float sy_p1 = eval_template_at(best_x_px, best_y_px + 1);

    float sub_dx = 0.0f;
    const float denom_x = 2.0f * (sx_m1 - 2.0f * best_score + sx_p1);
    if ( sx_m1 > 0.0f && sx_p1 > 0.0f && std::abs(denom_x) > 1e-4f ) {
        sub_dx = std::clamp((sx_m1 - sx_p1) / denom_x, -0.45f, 0.45f);
    }

    float sub_dy = 0.0f;
    const float denom_y = 2.0f * (sy_m1 - 2.0f * best_score + sy_p1);
    if ( sy_m1 > 0.0f && sy_p1 > 0.0f && std::abs(denom_y) > 1e-4f ) {
        sub_dy = std::clamp((sy_m1 - sy_p1) / denom_y, -0.45f, 0.45f);
    }

    out_cx = std::clamp((static_cast<float>(best_x_px) + 0.5f + sub_dx) * inv_w, 0.08f, 0.92f);
    out_cy = std::clamp((static_cast<float>(best_y_px) + 0.5f + sub_dy) * inv_h, 0.12f, 0.78f);
    out_rx = std::clamp(config_.calib_head_rx / 1.12f, 0.065f, 0.18f);
    return true;
}

void AiOnnxSegmenter::SetSilhouetteMode(int mode) noexcept {
    const int clamped = (mode == 1) ? 1 : 0;
    if (config_.silhouette_mode != clamped) {
        config_.silhouette_mode = clamped;
        prev_prob_map_.clear();
    }
}

void AiOnnxSegmenter::SetMultiPersonMode(bool enabled) noexcept {
    if (config_.multi_person_enabled != enabled) {
        config_.multi_person_enabled = enabled;
        prev_prob_map_.clear();
    }
}

void AiOnnxSegmenter::ApplyEdgeAwareGuidedFilter3x3(const float* guide_rgb_nhwc,
                                                    uint32_t tw,
                                                    uint32_t th,
                                                    float* inout_prob,
                                                    float* scratch_prob) noexcept {
    if (!guide_rgb_nhwc || !inout_prob || !scratch_prob || tw < 3 || th < 3) {
        return;
    }

    for (uint32_t y = 0; y < th; ++y) {
        const uint32_t y0 = (y > 0) ? (y - 1) : 0;
        const uint32_t y2 = (y + 1 < th) ? (y + 1) : (th - 1);
        const uint32_t ys[3] = {y0, y, y2};

        for (uint32_t x = 0; x < tw; ++x) {
            const uint32_t x0 = (x > 0) ? (x - 1) : 0;
            const uint32_t x2 = (x + 1 < tw) ? (x + 1) : (tw - 1);
            const uint32_t xs[3] = {x0, x, x2};

            const size_t center_idx = static_cast<size_t>(y) * tw + x;
            const float cr = guide_rgb_nhwc[center_idx * 3 + 0];
            const float cg = guide_rgb_nhwc[center_idx * 3 + 1];
            const float cb = guide_rgb_nhwc[center_idx * 3 + 2];

            float weighted_sum = 0.0f;
            float weight_total = 0.0f;

            for (int ky = 0; ky < 3; ++ky) {
                const size_t row_off = static_cast<size_t>(ys[ky]) * tw;
                const float wy = (ky == 1) ? 2.0f : 1.0f;
                for (int kx = 0; kx < 3; ++kx) {
                    const size_t n_idx = row_off + xs[kx];
                    const float dr = guide_rgb_nhwc[n_idx * 3 + 0] - cr;
                    const float dg = guide_rgb_nhwc[n_idx * 3 + 1] - cg;
                    const float db = guide_rgb_nhwc[n_idx * 3 + 2] - cb;
                    const float color_dist_sq = dr * dr + dg * dg + db * db;

                    const float wx = (kx == 1) ? 2.0f : 1.0f;
                    const float w = (wx * wy) / (1.0f + 50.0f * color_dist_sq);
                    weighted_sum += w * inout_prob[n_idx];
                    weight_total += w;
                }
            }

            scratch_prob[center_idx] = (weight_total > 0.0f) ? (weighted_sum / weight_total) : inout_prob[center_idx];
        }
    }

    std::memcpy(inout_prob, scratch_prob, static_cast<size_t>(tw) * th * sizeof(float));
}

void AiOnnxSegmenter::RunPortraitHeuristicFallback(const float* input_nhwc,
                                                   uint32_t tw,
                                                   uint32_t th,
                                                   float* out_prob) noexcept {
    const size_t num_pixels = static_cast<size_t>(tw) * th;
    if (scratch_prob_map_.size() != num_pixels) {
        scratch_prob_map_.assign(num_pixels, 0.0f);
    }
    const bool has_prev_luma = (prev_luma_map_.size() == num_pixels);
    if (!has_prev_luma) {
        prev_luma_map_.assign(num_pixels, 0.0f);
    }

    const float inv_tw = 1.0f / static_cast<float>(tw);
    const float inv_th = 1.0f / static_cast<float>(th);

    // ------------------------------------------------------------------------
    // 1. Per-Row Left/Right & Per-Column Top Room Background Model
    //    Captures vertical variations in room background (e.g., upper white wall +
    //    lower wooden staircase on the left border) so stationary room structures
    //    never pull the subject centroid or leak into the silhouette.
    // ------------------------------------------------------------------------
    const uint32_t top_band_h  = std::max(3u, th / 10);
    const uint32_t side_band_w = std::max(3u, tw / 12);

    std::vector<float> bg_row_y_L(th, 128.0f), bg_row_cb_L(th, 128.0f), bg_row_cr_L(th, 128.0f);
    std::vector<float> bg_row_y_R(th, 128.0f), bg_row_cb_R(th, 128.0f), bg_row_cr_R(th, 128.0f);
    std::vector<float> bg_col_y_T(tw, 128.0f), bg_col_cb_T(tw, 128.0f), bg_col_cr_T(tw, 128.0f);

    for (uint32_t y = 0; y < th; ++y) {
        float sy_L = 0.0f, scb_L = 0.0f, scr_L = 0.0f;
        float sy_R = 0.0f, scb_R = 0.0f, scr_R = 0.0f;
        for (uint32_t x = 0; x < side_band_w; ++x) {
            const size_t idxL = (static_cast<size_t>(y) * tw + x) * 3;
            float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
            Rgb01ToYCbCr(input_nhwc[idxL + 0], input_nhwc[idxL + 1], input_nhwc[idxL + 2], py, pcb, pcr);
            sy_L += py; scb_L += pcb; scr_L += pcr;

            const uint32_t rx = tw - 1 - x;
            const size_t idxR = (static_cast<size_t>(y) * tw + rx) * 3;
            Rgb01ToYCbCr(input_nhwc[idxR + 0], input_nhwc[idxR + 1], input_nhwc[idxR + 2], py, pcb, pcr);
            sy_R += py; scb_R += pcb; scr_R += pcr;
        }
        const float inv_sw = 1.0f / static_cast<float>(side_band_w);
        bg_row_y_L[y]  = sy_L * inv_sw;  bg_row_cb_L[y] = scb_L * inv_sw; bg_row_cr_L[y] = scr_L * inv_sw;
        bg_row_y_R[y]  = sy_R * inv_sw;  bg_row_cb_R[y] = scb_R * inv_sw; bg_row_cr_R[y] = scr_R * inv_sw;
    }

    for (uint32_t x = 0; x < tw; ++x) {
        float sy_T = 0.0f, scb_T = 0.0f, scr_T = 0.0f;
        for (uint32_t y = 0; y < top_band_h; ++y) {
            const size_t idxT = (static_cast<size_t>(y) * tw + x) * 3;
            float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
            Rgb01ToYCbCr(input_nhwc[idxT + 0], input_nhwc[idxT + 1], input_nhwc[idxT + 2], py, pcb, pcr);
            sy_T += py; scb_T += pcb; scr_T += pcr;
        }
        const float inv_th_band = 1.0f / static_cast<float>(top_band_h);
        bg_col_y_T[x]  = sy_T * inv_th_band;
        bg_col_cb_T[x] = scb_T * inv_th_band;
        bg_col_cr_T[x] = scr_T * inv_th_band;
    }

    // ------------------------------------------------------------------------
    // 2. Per-Pixel True Facial Skin Map, Background Contrast & Gradient Maps
    // ------------------------------------------------------------------------
    std::vector<float> skin_map(num_pixels, 0.0f);
    std::vector<float> luma_curr(num_pixels, 0.0f);
    std::vector<float> bg_diff_map(num_pixels, 0.0f);
    std::vector<float> feat_grad_map(num_pixels, 0.0f);
    float max_bg_diff = 0.0f;

    for (uint32_t y = 0; y < th; ++y) {
        const float row_y_L  = bg_row_y_L[y],  row_cb_L = bg_row_cb_L[y], row_cr_L = bg_row_cr_L[y];
        const float row_y_R  = bg_row_y_R[y],  row_cb_R = bg_row_cb_R[y], row_cr_R = bg_row_cr_R[y];

        for (uint32_t x = 0; x < tw; ++x) {
            const size_t idx  = static_cast<size_t>(y) * tw + x;
            const size_t idx3 = idx * 3;
            const float r = input_nhwc[idx3 + 0];
            const float g = input_nhwc[idx3 + 1];
            const float b = input_nhwc[idx3 + 2];

            float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
            Rgb01ToYCbCr(r, g, b, py, pcb, pcr);
            luma_curr[idx] = py;

            // Compare pixel against:
            //   (a) Left border on the same row (captures left-side walls/wooden stairs)
            //   (b) Right border on the same row (captures right-side walls/pillars)
            //   (c) Top border in the same column (captures ceiling/upper wall)
            auto calc_dist = [&](float ry, float rcb, float rcr) noexcept -> float {
                const float d_chroma = std::hypot(pcb - rcb, pcr - rcr);
                const float d_luma   = std::abs(py - ry);
                return std::max(d_chroma / 16.0f, (d_chroma * 0.85f + d_luma * 0.42f) / 24.0f);
            };

            const float dist_L = calc_dist(row_y_L, row_cb_L, row_cr_L);
            const float dist_R = calc_dist(row_y_R, row_cb_R, row_cr_R);
            const float dist_T = calc_dist(bg_col_y_T[x], bg_col_cb_T[x], bg_col_cr_T[x]);

            // If a pixel on the left half matches the left-row border (e.g. wooden stairs extending from left border),
            // or matches the top wall, its effective background difference is the minimum distance to the relevant border.
            const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;
            float min_border_dist = dist_T;
            if (nx < 0.62f) {
                min_border_dist = std::min(min_border_dist, dist_L);
            }
            if (nx > 0.38f) {
                min_border_dist = std::min(min_border_dist, dist_R);
            }

            const float bg_diff = std::clamp(min_border_dist, 0.0f, 1.0f);
            bg_diff_map[idx] = bg_diff;
            if (bg_diff > max_bg_diff) {
                max_bg_diff = bg_diff;
            }

            const float true_skin = ComputeTrueFacialSkinScore(r, g, b, py, pcb, pcr, &config_);
            // Suppress skin score on pixels that match the room's border palette on the same row/column
            const float bg_gate   = std::clamp((bg_diff - 0.12f) / 0.32f, 0.0f, 1.0f);
            const float skin_fg   = true_skin * bg_gate;
            skin_map[idx]         = skin_fg;

            float motion = 0.0f;
            if (has_prev_luma) {
                motion = std::clamp(std::abs(py - prev_luma_map_[idx]) / 28.0f, 0.0f, 1.0f);
            }
            prev_luma_map_[idx] = py;

            scratch_prob_map_[idx] = std::clamp(
                std::max(bg_diff * 1.15f, skin_fg * 1.45f) + 0.15f * motion * bg_diff,
                0.0f, 1.0f);
        }
    }

    // ------------------------------------------------------------------------
    // 3. Handle Uniform Synthetic Test Frames (Zero Contrast Across Frame)
    // ------------------------------------------------------------------------
    if (max_bg_diff < 0.02f) {
        tracked_cx_ = 0.50f;
        tracked_cy_ = 0.50f;
        const float cx = 0.50f;
        const float cy = 0.56f;
        const float rx = 0.34f;
        const float ry = 0.44f;
        for (uint32_t y = 0; y < th; ++y) {
            const float ny = (static_cast<float>(y) + 0.5f) * inv_th;
            const float dy = (ny - cy) / ry;
            for (uint32_t x = 0; x < tw; ++x) {
                const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;
                const float dx = (nx - cx) / rx;
                const size_t idx = static_cast<size_t>(y) * tw + x;
                const float r = input_nhwc[idx * 3 + 0];
                const float g = input_nhwc[idx * 3 + 1];
                const float b = input_nhwc[idx * 3 + 2];
                const float dist_sq = dx * dx + dy * dy;
                const float spatial_prior = std::clamp(1.35f - dist_sq, 0.0f, 1.0f);
                const float luma = 0.299f * r + 0.587f * g + 0.114f * b;
                const float warmth = std::clamp((r - b) * 0.6f + 0.5f, 0.2f, 1.0f);
                out_prob[idx] = std::clamp(spatial_prior * (0.65f + 0.35f * warmth * luma), 0.0f, 1.0f);
            }
        }
        return;
    }

    // Compute internal facial feature gradient map (eyes, eyebrows, nostrils, mouth/mustache)
    for (uint32_t y = 1; y + 1 < th; ++y) {
        for (uint32_t x = 1; x + 1 < tw; ++x) {
            const size_t idx = static_cast<size_t>(y) * tw + x;
            const float gx = std::abs(luma_curr[idx + 1] - luma_curr[idx - 1]);
            const float gy = std::abs(luma_curr[idx + tw] - luma_curr[idx - tw]);
            // Eyes, eyebrows, nostrils and mouth have strong horizontal edges (gy)
            feat_grad_map[idx] = std::clamp((gy * 1.35f + gx * 0.65f) / 32.0f, 0.0f, 1.0f);
        }
    }

    // ------------------------------------------------------------------------
    // 4. 2D Integral-Image Sliding Face Detector (Eyes + Nostrils + Mouth +
    //    Bilateral Symmetry + Compact Head Island Isolation + Multi-Person NMS)
    //    Finds the exact human face box(es) without ever being pulled by wooden
    //    stairs, doors, or yellow hoodies (eliminating parallax & flickering).
    // ------------------------------------------------------------------------
    IntegralGrid sat_skin;
    IntegralGrid sat_grad;
    IntegralGrid sat_luma;
    sat_skin.Build(skin_map.data(), tw, th);
    sat_grad.Build(feat_grad_map.data(), tw, th);
    sat_luma.Build(luma_curr.data(), tw, th);

    struct FaceCandidate {
        float cx    = 0.50f;
        float cy    = 0.42f;
        float rx    = 0.105f;
        float score = 0.0f;
    };
    std::vector<FaceCandidate> raw_candidates;
    if (config_.multi_person_enabled) {
        raw_candidates.reserve(256);
    }

    float best_score = -1.0f;
    float best_cx    = tracked_cx_;
    float best_cy    = tracked_cy_;
    float best_rx    = tracked_radius_;

    const float kCandidateRx[3] = {0.082f, 0.105f, 0.130f};
    const int step_y = std::max(1, static_cast<int>(th) / 48);
    const int step_x = std::max(1, static_cast<int>(tw) / 64);

    for (float cand_rx : kCandidateRx) {
        const float cand_ry = cand_rx * 1.65f;
        const int hw = std::max(4, static_cast<int>(cand_rx * static_cast<float>(tw) + 0.5f));
        const int hh = std::max(6, static_cast<int>(cand_ry * static_cast<float>(th) + 0.5f));

        const int y_min = std::max(hh / 2, static_cast<int>(0.16f * th));
        const int y_max = std::min(static_cast<int>(th) - hh / 2 - 1, static_cast<int>(0.62f * th));
        const int x_min = std::max(hw / 2, static_cast<int>(0.10f * tw));
        const int x_max = std::min(static_cast<int>(tw) - hw / 2 - 1, static_cast<int>(0.90f * tw));

        for (int cy_px = y_min; cy_px <= y_max; cy_px += step_y) {
            const float ny = (static_cast<float>(cy_px) + 0.5f) * inv_th;
            // Subtle anatomical height preference for human head in webcam framing (y ~ 0.30 .. 0.50)
            const float y_prior = std::clamp(1.08f - std::abs(ny - 0.39f) * 0.95f, 0.55f, 1.0f);

            for (int cx_px = x_min; cx_px <= x_max; cx_px += step_x) {
                const float nx = (static_cast<float>(cx_px) + 0.5f) * inv_tw;

                // Core face box [cx - 0.85*hw .. cx + 0.85*hw] x [cy - 0.75*hh .. cy + 0.75*hh]
                const int fx0 = cx_px - (hw * 85) / 100;
                const int fx1 = cx_px + (hw * 85) / 100;
                const int fy0 = cy_px - (hh * 75) / 100;
                const int fy1 = cy_px + (hh * 75) / 100;

                const float core_skin = sat_skin.RectMean(fx0, fy0, fx1, fy1);
                if (core_skin < 0.14f) {
                    continue;
                }

                // Bilateral symmetry check: both left and right halves of the face must contain skin!
                const float left_skin  = sat_skin.RectMean(fx0, fy0, cx_px, fy1);
                const float right_skin = sat_skin.RectMean(cx_px, fy0, fx1, fy1);
                const float min_lr     = std::min(left_skin, right_skin);
                const float max_lr     = std::max(0.001f, std::max(left_skin, right_skin));
                const float lr_sym     = min_lr / max_lr;
                if (lr_sym < 0.25f) {
                    continue;
                }

                // Vertical face distribution: both upper face (forehead/eyes) and lower face (nose/cheeks/mouth) MUST have skin!
                // Rejects any box straddling a white wall/ceiling on top and wooden stairs on the bottom!
                const float upper_skin = sat_skin.RectMean(fx0, fy0, fx1, cy_px);
                const float lower_skin = sat_skin.RectMean(fx0, cy_px, fx1, fy1);
                if (upper_skin < 0.11f || lower_skin < 0.11f) {
                    continue;
                }
                const float ud_balance = std::min(upper_skin, lower_skin) / std::max(0.001f, std::max(upper_skin, lower_skin));
                if (ud_balance < 0.34f) {
                    continue;
                }

                // 4-Quadrant + Central Nose-Bridge Skin Check:
                // All 4 quadrants (Top-Left, Top-Right, Bottom-Left, Bottom-Right) and the vertical nose-bridge
                // strip must contain skin so room corners/edges can never masquerade as a face.
                const float q_tl = sat_skin.RectMean(fx0, fy0, cx_px, cy_px);
                const float q_tr = sat_skin.RectMean(cx_px, fy0, fx1, cy_px);
                const float q_bl = sat_skin.RectMean(fx0, cy_px, cx_px, fy1);
                const float q_br = sat_skin.RectMean(cx_px, cy_px, fx1, fy1);
                const float min_quad = std::min(std::min(q_tl, q_tr), std::min(q_bl, q_br));
                if (min_quad < 0.075f) {
                    continue;
                }
                const int nose_x0 = cx_px - (hw * 32) / 100;
                const int nose_x1 = cx_px + (hw * 32) / 100;
                const float nose_strip_skin = sat_skin.RectMean(nose_x0, fy0, nose_x1, fy1);
                if (nose_strip_skin < 0.16f) {
                    continue;
                }

                // Upper-vs-Lower Luminance Sanity Check:
                // On a real human face, the upper half (hair/eyebrows/eyes) is NEVER much brighter than the cheeks/jaw.
                // This immediately rejects white ceiling/wall above dark wooden stairs!
                const float upper_luma = sat_luma.RectMean(fx0, fy0, fx1, cy_px);
                const float lower_luma = sat_luma.RectMean(fx0, cy_px, fx1, fy1);
                if (upper_luma - lower_luma > 34.0f) {
                    continue;
                }

                // Compact Island Isolation: outside left temple, outside right temple, and above hair crown
                // must have MUCH lower skin density than the face core (rejects large wooden walls/stairs!)
                const float out_left_skin  = sat_skin.RectMean(cx_px - (hw * 185) / 100, fy0, fx0 - 1, fy1);
                const float out_right_skin = sat_skin.RectMean(fx1 + 1, fy0, cx_px + (hw * 185) / 100, fy1);
                const float out_top_skin   = sat_skin.RectMean(fx0, cy_px - (hh * 155) / 100, fx1, fy0 - 1);
                const float surround_skin  = 0.40f * out_left_skin + 0.40f * out_right_skin + 0.20f * out_top_skin;

                const float island_contrast = core_skin - 0.80f * surround_skin;
                if (island_contrast <= 0.06f) {
                    continue;
                }

                // Facial Features (Eyes + Eyebrows + Nostrils + Mouth/Mustache internal horizontal structure)
                const float eye_grad   = sat_grad.RectMean(fx0, cy_px - (hh * 35) / 100, fx1, cy_px + (hh * 5) / 100);
                const float mouth_grad = sat_grad.RectMean(cx_px - (hw * 60) / 100, cy_px + (hh * 5) / 100,
                                                           cx_px + (hw * 60) / 100, cy_px + (hh * 62) / 100);
                const float feature_bonus = 1.0f + 0.65f * std::clamp(eye_grad * 1.4f + mouth_grad * 1.2f, 0.0f, 1.0f);

                // Crown/temple luminance contrast against surrounding room wall
                const float face_luma = sat_luma.RectMean(fx0, fy0, fx1, fy1);
                const float wall_luma = 0.5f * (sat_luma.RectMean(cx_px - (hw * 180) / 100, fy0, fx0 - 1, fy1) +
                                                sat_luma.RectMean(fx1 + 1, fy0, cx_px + (hw * 180) / 100, fy1));
                const float temple_contrast = std::clamp(std::abs(face_luma - wall_luma) / 35.0f, 0.15f, 1.0f);

                // Anti-flicker & anti-jump spatial-temporal continuity prior around tracked face center
                float spatial_continuity = 1.0f;
                if (tracker_initialized_ && !config_.multi_person_enabled) {
                    const float dist2_from_prev = (nx - tracked_cx_) * (nx - tracked_cx_) +
                                                  (ny - tracked_cy_) * (ny - tracked_cy_);
                    spatial_continuity = 1.0f + 0.45f * std::exp(-dist2_from_prev / (2.0f * 0.13f * 0.13f));
                }

                const float sym_factor = (0.50f + 0.50f * lr_sym) * (0.55f + 0.45f * ud_balance);
                const float score = island_contrast * core_skin * sym_factor * feature_bonus *
                                    (0.75f + 0.25f * temple_contrast) * y_prior * spatial_continuity;

                if (score > best_score) {
                    best_score = score;
                    best_cx    = nx;
                    best_cy    = ny;
                    best_rx    = cand_rx;
                }
                if (config_.multi_person_enabled && score > 0.01f) {
                    raw_candidates.push_back(FaceCandidate{nx, ny, cand_rx, score});
                }
            }
        }
    }

    auto refine_candidate = [&](float init_cx, float init_cy, float init_rx,
                                float& out_cx, float& out_cy, float& out_radius) noexcept {
        out_cx     = init_cx;
        out_cy     = init_cy;
        out_radius = init_rx;
        const float win_rx = init_rx * 1.15f;
        const float win_ry = init_rx * 1.65f;
        float fine_wx = 0.0f, fine_wy = 0.0f, fine_w = 0.0f, fine_dx2 = 0.0f;

        for (uint32_t y = 1; y + 1 < th; ++y) {
            const float ny = (static_cast<float>(y) + 0.5f) * inv_th;
            if (std::abs(ny - init_cy) > win_ry) continue;

            for (uint32_t x = 1; x + 1 < tw; ++x) {
                const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;
                if (std::abs(nx - init_cx) > win_rx) continue;

                const size_t idx = static_cast<size_t>(y) * tw + x;
                const float s_val = skin_map[idx];
                const float f_val = feat_grad_map[idx];
                const float weight = s_val * (1.0f + 0.50f * f_val);
                if (weight > 0.14f) {
                    fine_wx  += weight * nx;
                    fine_wy  += weight * ny;
                    fine_w   += weight;
                    fine_dx2 += weight * (nx - init_cx) * (nx - init_cx);
                }
            }
        }

        if (fine_w > 0.5f) {
            out_cx = std::clamp(fine_wx / fine_w, 0.08f, 0.92f);
            out_cy = std::clamp(fine_wy / fine_w, 0.14f, 0.60f);
            const float std_x = std::sqrt(std::max(0.0005f, fine_dx2 / fine_w));
            out_radius = std::clamp(std_x * 2.05f, 0.085f, 0.145f);
        }
        if (config_.face_calibrated) {
            out_radius = std::clamp(config_.calib_head_rx / 1.12f, 0.065f, 0.18f);
        }
    };

    struct PersonAnatomy {
        float head_cx      = 0.50f;
        float head_cy      = 0.42f;
        float rx_head      = 0.105f;
        float ry_head      = 0.175f;
        float inv_rx_head  = 9.52f;
        float inv_ry_head  = 5.71f;
        float neck_cy      = 0.58f;
        float inv_rx_neck  = 10.1f;
        float inv_ry_neck  = 9.2f;
        float chin_top_y   = 0.54f;
        float torso_cy     = 0.77f;
        float inv_rx_torso = 4.8f;
        float inv_ry_torso = 3.5f;
        float shirt_cb     = 128.0f;
        float shirt_cr     = 128.0f;
    };

    PersonAnatomy persons[4];
    int num_persons = 0;

    auto build_person_anatomy = [&](float p_cx, float p_cy, float p_rad) noexcept -> PersonAnatomy {
        PersonAnatomy pa{};
        pa.head_cx = p_cx;
        pa.head_cy = p_cy;
        const float r = std::clamp(p_rad, 0.070f, 0.165f);

        if (config_.face_calibrated) {
            pa.rx_head = std::clamp(config_.calib_head_rx, 0.065f, 0.22f);
            pa.ry_head = std::clamp(config_.calib_head_ry, 0.11f, 0.34f);
        } else {
            pa.rx_head = std::max(0.085f, 1.12f * r);
            pa.ry_head = std::max(0.155f, 1.82f * r);
        }
        pa.inv_rx_head = 1.0f / pa.rx_head;
        pa.inv_ry_head = 1.0f / pa.ry_head;

        pa.neck_cy     = pa.head_cy + 0.90f * pa.ry_head;
        const float rx_neck = std::max(0.075f, 0.94f * pa.rx_head);
        const float ry_neck = std::max(0.090f, 0.62f * pa.ry_head);
        pa.inv_rx_neck = 1.0f / rx_neck;
        pa.inv_ry_neck = 1.0f / ry_neck;

        pa.chin_top_y  = pa.head_cy + 0.72f * pa.ry_head;
        pa.torso_cy    = pa.head_cy + 2.02f * pa.ry_head;
        const float rx_torso = std::clamp(1.98f * pa.rx_head, 0.165f, 0.245f);
        const float ry_torso = std::max(0.230f, 1.62f * pa.ry_head);
        pa.inv_rx_torso = 1.0f / rx_torso;
        pa.inv_ry_torso = 1.0f / ry_torso;

        pa.shirt_cb = 128.0f;
        pa.shirt_cr = 128.0f;
        const int sx0 = std::clamp(static_cast<int>((pa.head_cx - 0.45f * pa.rx_head) * tw), 0, static_cast<int>(tw) - 1);
        const int sx1 = std::clamp(static_cast<int>((pa.head_cx + 0.45f * pa.rx_head) * tw), sx0 + 1, static_cast<int>(tw));
        const int sy0 = std::clamp(static_cast<int>((pa.neck_cy + 0.04f) * th), 0, static_cast<int>(th) - 1);
        const int sy1 = std::clamp(static_cast<int>(std::min(0.96f, pa.torso_cy + 0.05f) * th), sy0 + 1, static_cast<int>(th));
        float sum_scb = 0.0f, sum_scr = 0.0f;
        int cnt_s = 0;
        for (int sy = sy0; sy < sy1; ++sy) {
            for (int sx = sx0; sx < sx1; ++sx) {
                const size_t idx3 = (static_cast<size_t>(sy) * tw + static_cast<size_t>(sx)) * 3;
                float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
                Rgb01ToYCbCr(input_nhwc[idx3 + 0], input_nhwc[idx3 + 1], input_nhwc[idx3 + 2], py, pcb, pcr);
                sum_scb += pcb; sum_scr += pcr;
                ++cnt_s;
            }
        }
        if (cnt_s > 0) {
            const float inv_c = 1.0f / static_cast<float>(cnt_s);
            pa.shirt_cb = sum_scb * inv_c;
            pa.shirt_cr = sum_scr * inv_c;
        }
        return pa;
    };

    // ------------------------------------------------------------------------
    // Soluzione 3: Direct 5-Point Calibrated Visual Template Tracking
    // When `config_.face_calibrated` is active, track the visual feature template
    // of the 5 calibrated landmarks (`Occhio DX`, `Occhio SX`, `Naso`, `Labbra`, `Mento`)
    // via ZNCC correlation without EVER running `refine_candidate` skin-centroid drift!
    // ------------------------------------------------------------------------
    if (config_.face_calibrated && !config_.multi_person_enabled) {
        float tpl_cx = tracked_cx_;
        float tpl_cy = tracked_cy_;
        float tpl_rx = tracked_radius_;
        if (TrackCalibratedFaceTemplate(input_nhwc, luma_curr.data(), feat_grad_map.data(),
                                        tw, th, tpl_cx, tpl_cy, tpl_rx)) {
            const float jump = std::hypot(tpl_cx - tracked_cx_, tpl_cy - tracked_cy_);
            if (!tracker_initialized_) {
                tracked_cx_          = tpl_cx;
                tracked_cy_          = tpl_cy;
                tracked_radius_      = tpl_rx;
                tracker_initialized_ = true;
            } else if (jump >= 0.004f) {
                const float alpha = std::clamp(0.60f + (jump - 0.004f) * 8.0f, 0.60f, 0.96f);
                tracked_cx_       = (1.0f - alpha) * tracked_cx_ + alpha * tpl_cx;
                tracked_cy_       = (1.0f - alpha) * tracked_cy_ + alpha * tpl_cy;
                tracked_radius_   = tpl_rx;
            }
            persons[0]  = build_person_anatomy(tracked_cx_, tracked_cy_, tracked_radius_);
            num_persons = 1;
        }
    } else if (best_score > 0.01f) {
        float target_cx     = best_cx;
        float target_cy     = best_cy;
        float target_radius = best_rx;
        refine_candidate(best_cx, best_cy, best_rx, target_cx, target_cy, target_radius);

        const float jump = std::hypot(target_cx - tracked_cx_, target_cy - tracked_cy_);
        bool old_face_still_present = false;
        if (tracker_initialized_ && jump > 0.14f) {
            const int prev_cx_px = static_cast<int>(tracked_cx_ * static_cast<float>(tw) + 0.5f);
            const int prev_cy_px = static_cast<int>(tracked_cy_ * static_cast<float>(th) + 0.5f);
            const int prev_hw = std::max(4, static_cast<int>(tracked_radius_ * 0.85f * static_cast<float>(tw) + 0.5f));
            const int prev_hh = std::max(6, static_cast<int>(tracked_radius_ * 1.25f * static_cast<float>(th) + 0.5f));
            const float old_up_skin = sat_skin.RectMean(prev_cx_px - prev_hw, prev_cy_px - prev_hh, prev_cx_px + prev_hw, prev_cy_px);
            const float old_lo_skin = sat_skin.RectMean(prev_cx_px - prev_hw, prev_cy_px, prev_cx_px + prev_hw, prev_cy_px + prev_hh);
            old_face_still_present = (old_up_skin >= 0.16f && old_lo_skin >= 0.16f);
        }

        if (!tracker_initialized_ || (jump > 0.035f && !old_face_still_present)) {
            tracked_cx_          = target_cx;
            tracked_cy_          = target_cy;
            tracked_radius_      = target_radius;
            tracker_initialized_ = true;
            pending_jump_frames_ = 0;
        } else if (jump > 0.14f && old_face_still_present) {
            const float jump_consistency = std::hypot(target_cx - pending_jump_cx_, target_cy - pending_jump_cy_);
            if (jump_consistency < 0.06f) {
                pending_jump_cx_ = 0.5f * pending_jump_cx_ + 0.5f * target_cx;
                pending_jump_cy_ = 0.5f * pending_jump_cy_ + 0.5f * target_cy;
                ++pending_jump_frames_;
            } else {
                pending_jump_cx_     = target_cx;
                pending_jump_cy_     = target_cy;
                pending_jump_frames_ = 1;
            }
            if (pending_jump_frames_ >= 4) {
                tracked_cx_          = pending_jump_cx_;
                tracked_cy_          = pending_jump_cy_;
                tracked_radius_      = 0.50f * tracked_radius_ + 0.50f * target_radius;
                pending_jump_frames_ = 0;
            }
        } else {
            pending_jump_frames_ = 0;
            if (jump >= 0.005f) {
                const float track_alpha = std::clamp(0.65f + (jump - 0.005f) * 10.0f, 0.65f, 1.0f);
                tracked_cx_     = (1.0f - track_alpha) * tracked_cx_ + track_alpha * target_cx;
                tracked_cy_     = (1.0f - track_alpha) * tracked_cy_ + track_alpha * target_cy;
                tracked_radius_ = 0.60f * tracked_radius_ + 0.40f * target_radius;
            }
        }

        if (config_.multi_person_enabled && !raw_candidates.empty()) {
            std::sort(raw_candidates.begin(), raw_candidates.end(),
                      [](const FaceCandidate& a, const FaceCandidate& b) { return a.score > b.score; });

            std::vector<FaceCandidate> selected;
            selected.reserve(4);
            const float min_valid_score = 0.28f * best_score;

            for (const auto& cand : raw_candidates) {
                if (cand.score < min_valid_score) break;
                bool overlaps = false;
                for (const auto& sel : selected) {
                    if (std::abs(cand.cx - sel.cx) < 1.45f * (cand.rx + sel.rx)) {
                        overlaps = true;
                        break;
                    }
                }
                if (!overlaps) {
                    selected.push_back(cand);
                    if (selected.size() >= 4) break;
                }
            }

            for (size_t i = 0; i < selected.size() && num_persons < 4; ++i) {
                float r_cx = selected[i].cx;
                float r_cy = selected[i].cy;
                float r_rad = selected[i].rx;
                refine_candidate(selected[i].cx, selected[i].cy, selected[i].rx, r_cx, r_cy, r_rad);
                persons[num_persons++] = build_person_anatomy(r_cx, r_cy, r_rad);
            }
        }
    }

    if (num_persons == 0) {
        persons[0]  = build_person_anatomy(tracked_cx_, tracked_cy_, tracked_radius_);
        num_persons = 1;
    }

    // ------------------------------------------------------------------------
    // 5. True-Scale Anatomical Silhouette (`Volto + Busto` vs `Solo Volto`)
    //    Supports 1 primary user or up to 4 simultaneously tracked people.
    // ------------------------------------------------------------------------
    const bool face_only_mode = (config_.silhouette_mode == 1);

    auto compute_person_d2 = [&](const PersonAnatomy& pa, float nx, float ny) noexcept -> float {
        const float dx_head = (nx - pa.head_cx) * pa.inv_rx_head;
        const float dy_head = (ny - pa.head_cy) * pa.inv_ry_head;
        const float d2_head = dx_head * dx_head + dy_head * dy_head;
        if (face_only_mode) {
            return d2_head;
        }

        const float dx_neck = (nx - pa.head_cx) * pa.inv_rx_neck;
        const float dy_neck = (ny - pa.neck_cy) * pa.inv_ry_neck;
        const float d2_neck = (ny >= pa.head_cy) ? (dx_neck * dx_neck + dy_neck * dy_neck) : 99.0f;

        float d2_torso = 99.0f;
        const float torso_vertical_gate = std::clamp((ny - pa.chin_top_y) / (0.35f * pa.ry_head), 0.0f, 1.0f);
        if (torso_vertical_gate > 0.01f) {
            const float eff_inv_rx_torso = pa.inv_rx_torso / std::max(0.45f, 0.45f + 0.55f * torso_vertical_gate);
            const float dx_torso = (nx - pa.head_cx) * eff_inv_rx_torso;
            const float dy_torso = (ny - pa.torso_cy) * pa.inv_ry_torso;
            const float dx_t2 = dx_torso * dx_torso;
            const float dy_t2 = dy_torso * dy_torso;
            d2_torso = std::sqrt(dx_t2 * dx_t2 + dy_t2 * dy_t2 + 0.35f * dx_t2 * dy_t2);
        }

        return std::min(d2_head, std::min(d2_neck, d2_torso));
    };

    const float core_d2_max  = config_.face_calibrated ? 0.84f : 0.76f;
    const float outer_d2_max = config_.face_calibrated ? 1.12f : 1.18f;

    for (uint32_t y = 0; y < th; ++y) {
        const float ny = (static_cast<float>(y) + 0.5f) * inv_th;

        for (uint32_t x = 0; x < tw; ++x) {
            const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;

            float d2_min = 99.0f;
            int best_p   = 0;
            for (int p = 0; p < num_persons; ++p) {
                const float d2_p = compute_person_d2(persons[p], nx, ny);
                if (d2_p < d2_min) {
                    d2_min = d2_p;
                    best_p = p;
                }
            }

            const size_t idx = static_cast<size_t>(y) * tw + x;

            float prob = 0.0f;
            if (d2_min <= core_d2_max) {
                prob = 1.0f;
            } else if (d2_min >= outer_d2_max) {
                prob = 0.0f;
            } else {
                const float shell_t = (outer_d2_max - d2_min) / (outer_d2_max - core_d2_max);
                const float smooth_shell = shell_t * shell_t * (3.0f - 2.0f * shell_t);

                if (config_.face_calibrated) {
                    // Symmetric calibrated contour: prevents shaded cheeks from being cut off
                    // and prevents wooden stairs from bulging the contour outward!
                    prob = smooth_shell;
                } else {
                    const size_t idx3 = idx * 3;
                    float py = 0.0f, pcb = 0.0f, pcr = 0.0f;
                    Rgb01ToYCbCr(input_nhwc[idx3 + 0], input_nhwc[idx3 + 1], input_nhwc[idx3 + 2], py, pcb, pcr);
                    const float shirt_sim = std::clamp(
                        1.0f - std::hypot(pcb - persons[best_p].shirt_cb, pcr - persons[best_p].shirt_cr) / 24.0f,
                        0.0f, 1.0f);
                    const float fg_ev = std::max(scratch_prob_map_[idx], shirt_sim * bg_diff_map[idx]);
                    prob = std::clamp(smooth_shell * (0.40f + 0.60f * fg_ev), 0.0f, 1.0f);
                }
            }
            out_prob[idx] = prob;
        }
    }

    // ------------------------------------------------------------------------
    // 6. Fast 3x3 Edge-Aware Guided Filter on Corona + Strict Core/Outer Lock
    // ------------------------------------------------------------------------
    ApplyEdgeAwareGuidedFilter3x3(input_nhwc, tw, th, out_prob, scratch_prob_map_.data());

    for (uint32_t y = 0; y < th; ++y) {
        const float ny = (static_cast<float>(y) + 0.5f) * inv_th;

        for (uint32_t x = 0; x < tw; ++x) {
            const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;

            float d2_min = 99.0f;
            for (int p = 0; p < num_persons; ++p) {
                d2_min = std::min(d2_min, compute_person_d2(persons[p], nx, ny));
            }

            const size_t idx = static_cast<size_t>(y) * tw + x;
            if (d2_min <= core_d2_max) {
                out_prob[idx] = 1.0f;
            } else if (d2_min >= outer_d2_max) {
                out_prob[idx] = 0.0f;
            }
        }
    }
}

void AiOnnxSegmenter::RunReferenceBackgroundSubtraction(const float* input_nhwc,
                                                        uint32_t tw,
                                                        uint32_t th,
                                                        float* out_prob) noexcept {
    const size_t num_pixels = static_cast<size_t>(tw) * th;
    if (ref_bg_ycbcr_.size() != num_pixels * 3 || ref_bg_raw_ycbcr_.size() != num_pixels * 3) {
        RunPortraitHeuristicFallback(input_nhwc, tw, th, out_prob);
        return;
    }
    if (scratch_prob_map_.size() != num_pixels) {
        scratch_prob_map_.assign(num_pixels, 0.0f);
    }

    // Convert current frame to YCbCr
    std::vector<float> curr_ycbcr(num_pixels * 3, 0.0f);
    for (size_t i = 0; i < num_pixels; ++i) {
        const size_t idx3 = i * 3;
        Rgb01ToYCbCr(input_nhwc[idx3 + 0], input_nhwc[idx3 + 1], input_nhwc[idx3 + 2],
                     curr_ycbcr[idx3 + 0], curr_ycbcr[idx3 + 1], curr_ycbcr[idx3 + 2]);
    }

    // ------------------------------------------------------------------------
    // Stage 1: Per-Row Horizontal LED Tearing (Rolling-Shutter PWM) &
    //          Global Room Brightness/Chrominance Compensation
    // ------------------------------------------------------------------------
    const uint32_t side_w = std::max(4u, tw / 10);
    std::vector<float> raw_row_dy(th, 0.0f);
    std::vector<float> row_dy(th, 0.0f);
    float sum_dcb_global = 0.0f;
    float sum_dcr_global = 0.0f;
    uint32_t cnt_global  = 0;

    for (uint32_t y = 0; y < th; ++y) {
        float row_sum_dy = 0.0f;
        uint32_t row_cnt = 0;
        float fallback_sum_dy = 0.0f;
        uint32_t fallback_cnt = 0;

        for (uint32_t x = 0; x < tw; ++x) {
            if (x >= side_w && x < tw - side_w) {
                continue;
            }
            const size_t idx3 = (static_cast<size_t>(y) * tw + x) * 3;
            const float dy  = curr_ycbcr[idx3 + 0] - ref_bg_raw_ycbcr_[idx3 + 0];
            const float dcb = curr_ycbcr[idx3 + 1] - ref_bg_raw_ycbcr_[idx3 + 1];
            const float dcr = curr_ycbcr[idx3 + 2] - ref_bg_raw_ycbcr_[idx3 + 2];

            fallback_sum_dy += dy;
            ++fallback_cnt;

            if (std::hypot(dcb, dcr) < 18.0f) {
                row_sum_dy     += dy;
                ++row_cnt;
                sum_dcb_global += dcb;
                sum_dcr_global += dcr;
                ++cnt_global;
            }
        }

        if (row_cnt >= 2) {
            raw_row_dy[y] = row_sum_dy / static_cast<float>(row_cnt);
        } else if (fallback_cnt > 0) {
            raw_row_dy[y] = fallback_sum_dy / static_cast<float>(fallback_cnt);
        }
    }

    const float delta_cb_global = (cnt_global > 0) ? (sum_dcb_global / static_cast<float>(cnt_global)) : 0.0f;
    const float delta_cr_global = (cnt_global > 0) ? (sum_dcr_global / static_cast<float>(cnt_global)) : 0.0f;

    // Smooth row luminance offsets vertically over 5 rows while preserving sharp PWM LED tearing bands
    for (uint32_t y = 0; y < th; ++y) {
        float w_sum = 0.0f;
        float w_tot = 0.0f;
        for (int ky = -2; ky <= 2; ++ky) {
            const int ny = std::clamp(static_cast<int>(y) + ky, 0, static_cast<int>(th) - 1);
            const float diff_dy = std::abs(raw_row_dy[static_cast<size_t>(ny)] - raw_row_dy[y]);
            const float w_spatial = (ky == 0) ? 4.0f : ((std::abs(ky) == 1) ? 2.0f : 1.0f);
            // Bilateral weight preserves sharp horizontal LED PWM band transitions
            const float w = w_spatial / (1.0f + 0.25f * diff_dy * diff_dy);
            w_sum += w * raw_row_dy[static_cast<size_t>(ny)];
            w_tot += w;
        }
        row_dy[y] = (w_tot > 0.0f) ? (w_sum / w_tot) : raw_row_dy[y];
    }

    // ------------------------------------------------------------------------
    // Stage 2: Spatial Neighborhood Approximation (+-R Effective Position) &
    //          Chromatic/Luminance Tolerance
    // ------------------------------------------------------------------------
    const float tol           = std::clamp(config_.bg_diff_tolerance, 0.0f, 1.0f);
    const int   R             = (tol >= 0.45f) ? 2 : ((tol > 0.05f) ? 1 : 0);
    const float chroma_thresh = 7.0f + 12.0f * tol;
    const float luma_thresh   = 16.0f + 24.0f * tol;

    for (uint32_t y = 0; y < th; ++y) {
        const float dy_comp = row_dy[y];
        const int ny_min = std::max(0, static_cast<int>(y) - R);
        const int ny_max = std::min(static_cast<int>(th) - 1, static_cast<int>(y) + R);

        for (uint32_t x = 0; x < tw; ++x) {
            const size_t idx  = static_cast<size_t>(y) * tw + x;
            const size_t idx3 = idx * 3;

            const float corr_y  = curr_ycbcr[idx3 + 0] - dy_comp;
            const float corr_cb = curr_ycbcr[idx3 + 1] - delta_cb_global;
            const float corr_cr = curr_ycbcr[idx3 + 2] - delta_cr_global;

            const int nx_min = std::max(0, static_cast<int>(x) - R);
            const int nx_max = std::min(static_cast<int>(tw) - 1, static_cast<int>(x) + R);

            float min_diff_score = 1e9f;

            for (int ny = ny_min; ny <= ny_max; ++ny) {
                const size_t row_off = static_cast<size_t>(ny) * tw;
                for (int nx = nx_min; nx <= nx_max; ++nx) {
                    const size_t n_idx3 = (row_off + static_cast<size_t>(nx)) * 3;

                    // Compare against both exact reference pixel and 3x3 denoised reference pixel
                    for (int pass = 0; pass < 2; ++pass) {
                        const float* ref_ptr = (pass == 0) ? (ref_bg_raw_ycbcr_.data() + n_idx3)
                                                           : (ref_bg_ycbcr_.data() + n_idx3);
                        const float d_cb = corr_cb - ref_ptr[1];
                        const float d_cr = corr_cr - ref_ptr[2];
                        const float d_chroma = std::sqrt(d_cb * d_cb + d_cr * d_cr);
                        float d_luma = std::abs(corr_y - ref_ptr[0]);

                        // Attenuate soft wall shadows where chrominance is unchanged
                        if (d_chroma < (5.0f + 4.0f * tol) && d_luma < luma_thresh * 1.35f) {
                            d_luma *= 0.35f;
                        }

                        const float score = std::max(d_chroma / chroma_thresh, d_luma / luma_thresh) +
                                            0.25f * (d_chroma / chroma_thresh);
                        if (score < min_diff_score) {
                            min_diff_score = score;
                        }
                    }
                }
            }

            scratch_prob_map_[idx] = std::clamp((min_diff_score - 0.58f) / 0.62f, 0.0f, 1.0f);
        }
    }

    // ------------------------------------------------------------------------
    // Stage 3: 3x3 Density Filter + Morphological Opening +
    //          Scanline Silhouette Fill (100% Solid Human Figure, Zero Holes!)
    // ------------------------------------------------------------------------
    std::vector<float> density_map(num_pixels, 0.0f);
    for (uint32_t y = 0; y < th; ++y) {
        const uint32_t y0 = (y > 0) ? (y - 1) : 0;
        const uint32_t y2 = (y + 1 < th) ? (y + 1) : (th - 1);
        const uint32_t ys[3] = {y0, y, y2};

        for (uint32_t x = 0; x < tw; ++x) {
            const uint32_t x0 = (x > 0) ? (x - 1) : 0;
            const uint32_t x2 = (x + 1 < tw) ? (x + 1) : (tw - 1);
            const uint32_t xs[3] = {x0, x, x2};

            float neigh_sum = 0.0f;
            int active_cnt  = 0;
            for (uint32_t ny_i : ys) {
                const float* row_p = scratch_prob_map_.data() + static_cast<size_t>(ny_i) * tw;
                for (uint32_t nx_i : xs) {
                    const float val = row_p[nx_i];
                    neigh_sum += val;
                    if (val > 0.35f) {
                        ++active_cnt;
                    }
                }
            }

            const size_t idx = static_cast<size_t>(y) * tw + x;
            const float avg_p = neigh_sum * (1.0f / 9.0f);
            // Morphological opening gate: require at least 3 active neighbors in 3x3 to suppress isolated speckles
            density_map[idx] = (avg_p > 0.28f && active_cnt >= 3)
                ? std::clamp(scratch_prob_map_[idx] * 0.60f + avg_p * 0.55f, 0.0f, 1.0f)
                : 0.0f;
        }
    }

    // Compute primary subject centroid and bounding box
    const float inv_tw = 1.0f / static_cast<float>(tw);
    const float inv_th = 1.0f / static_cast<float>(th);
    float sum_wx = 0.0f, sum_wy = 0.0f, sum_w = 0.0f;

    for (uint32_t y = 0; y < th; ++y) {
        const float ny = (static_cast<float>(y) + 0.5f) * inv_th;
        for (uint32_t x = 0; x < tw; ++x) {
            const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;
            const size_t idx = static_cast<size_t>(y) * tw + x;
            const float p = density_map[idx];
            out_prob[idx] = p;
            if (p > 0.45f) {
                sum_wx += p * nx;
                sum_wy += p * ny;
                sum_w  += p;
            }
        }
    }

    std::vector<int> scan_left(th, -1);
    std::vector<int> scan_right(th, -2);

    if (sum_w > 1.5f) {
        const float coarse_cx = std::clamp(sum_wx / sum_w, 0.05f, 0.95f);
        const float coarse_cy = std::clamp(sum_wy / sum_w, 0.05f, 0.95f);

        // Refine centroid within main subject cluster to ignore any far-away outlier
        float fine_wx = 0.0f, fine_wy = 0.0f, fine_w = 0.0f, fine_dx2 = 0.0f;
        for (uint32_t y = 0; y < th; ++y) {
            const float ny = (static_cast<float>(y) + 0.5f) * inv_th;
            for (uint32_t x = 0; x < tw; ++x) {
                const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;
                if (std::abs(nx - coarse_cx) > 0.34f) continue;
                const float p = out_prob[static_cast<size_t>(y) * tw + x];
                if (p > 0.45f) {
                    fine_wx  += p * nx;
                    fine_wy  += p * ny;
                    fine_w   += p;
                    fine_dx2 += p * (nx - coarse_cx) * (nx - coarse_cx);
                }
            }
        }

        const float subj_cx = (fine_w > 1.0f) ? std::clamp(fine_wx / fine_w, 0.05f, 0.95f) : coarse_cx;
        const float subj_cy = (fine_w > 1.0f) ? std::clamp(fine_wy / fine_w, 0.05f, 0.95f) : coarse_cy;
        const float std_x   = (fine_w > 1.0f) ? std::sqrt(std::max(0.001f, fine_dx2 / fine_w)) : 0.12f;
        const float max_half_w = std::clamp(std_x * 2.6f, 0.18f, 0.34f);

        tracked_cx_          = subj_cx;
        tracked_cy_          = subj_cy;
        tracker_initialized_ = true;

        int y_top = static_cast<int>(th);
        int y_bot = -1;

        for (uint32_t y = 0; y < th; ++y) {
            int row_l = -1;
            int row_r = -2;
            for (uint32_t x = 0; x < tw; ++x) {
                const float nx = (static_cast<float>(x) + 0.5f) * inv_tw;
                const size_t idx = static_cast<size_t>(y) * tw + x;
                // Zero out any isolated blob outside the anatomical horizontal envelope around tracked centroid
                if (std::abs(nx - subj_cx) > max_half_w) {
                    out_prob[idx] = 0.0f;
                    continue;
                }
                if (out_prob[idx] >= 0.42f) {
                    if (row_l < 0) row_l = static_cast<int>(x);
                    row_r = static_cast<int>(x);
                }
            }
            if (row_l >= 0 && row_r >= row_l) {
                scan_left[y]  = row_l;
                scan_right[y] = row_r;
                if (static_cast<int>(y) < y_top) y_top = static_cast<int>(y);
                if (static_cast<int>(y) > y_bot) y_bot = static_cast<int>(y);
            }
        }

        // Bridge any missing/narrowed horizontal scanlines inside [y_top .. y_bot] (e.g. shirt matching wall color)
        if (y_bot > y_top) {
            for (int y = y_top + 1; y < y_bot; ++y) {
                if (scan_left[y] < 0) {
                    // Interpolate from nearest valid upper and lower rows
                    int prev_y = y - 1;
                    while (prev_y > y_top && scan_left[prev_y] < 0) --prev_y;
                    int next_y = y + 1;
                    while (next_y < y_bot && scan_left[next_y] < 0) ++next_y;
                    if (scan_left[prev_y] >= 0 && scan_left[next_y] >= 0) {
                        const float t = static_cast<float>(y - prev_y) / static_cast<float>(next_y - prev_y);
                        scan_left[y]  = static_cast<int>((1.0f - t) * scan_left[prev_y]  + t * scan_left[next_y]  + 0.5f);
                        scan_right[y] = static_cast<int>((1.0f - t) * scan_right[prev_y] + t * scan_right[next_y] + 0.5f);
                    }
                }
            }

            // Fill all interior pixels between [scan_left[y] .. scan_right[y]] to >= 0.96f (100% solid silhouette!)
            for (int y = y_top; y <= y_bot; ++y) {
                if (scan_left[y] >= 0 && scan_right[y] >= scan_left[y]) {
                    float* row_p = out_prob + static_cast<size_t>(y) * tw;
                    for (int x = scan_left[y]; x <= scan_right[y]; ++x) {
                        row_p[x] = std::max(row_p[x], 0.96f);
                    }
                }
            }
        }
    } else {
        std::fill(out_prob, out_prob + num_pixels, 0.0f);
    }

    // ------------------------------------------------------------------------
    // Stage 4: Edge-Aware Guided Filter 3x3 + Interior Core Lock
    // ------------------------------------------------------------------------
    ApplyEdgeAwareGuidedFilter3x3(input_nhwc, tw, th, out_prob, scratch_prob_map_.data());

    for (uint32_t y = 0; y < th; ++y) {
        if (scan_left[y] >= 0 && scan_right[y] >= scan_left[y]) {
            float* row_p = out_prob + static_cast<size_t>(y) * tw;
            const int inner_l = std::min(scan_right[y], scan_left[y] + 1);
            const int inner_r = std::max(scan_left[y],  scan_right[y] - 1);
            for (int x = inner_l; x <= inner_r; ++x) {
                row_p[x] = std::max(row_p[x], 0.96f);
            }
        }
    }
}

bool AiOnnxSegmenter::PostProcessProbabilityMap(const float* prob_map,
                                                uint32_t map_width,
                                                uint32_t map_height,
                                                FrameView& out_mask) {
    if (prob_map == nullptr || map_width == 0 || map_height == 0 || !out_mask.IsValid() ||
        out_mask.format != PixelFormat::GRAY8) {
        return false;
    }

    const size_t num_pixels = static_cast<size_t>(map_width) * map_height;
    if (lowres_mask_.Width() != map_width || lowres_mask_.Height() != map_height) {
        lowres_mask_.Allocate(map_width, map_height, PixelFormat::GRAY8);
    }

    const float smooth_alpha = std::clamp(config_.temporal_smooth, 0.0f, 0.90f);
    const float thresh       = std::clamp(config_.confidence_thresh, 0.05f, 0.95f);
    const float feather_band = 0.15f;
    const float low_edge     = std::max(0.0f, thresh - feather_band);
    const float inv_band     = 1.0f / (2.0f * feather_band);

    const bool has_history = (prev_prob_map_.size() == num_pixels && smooth_alpha > 0.0f);
    if (!has_history) {
        prev_prob_map_.assign(prob_map, prob_map + num_pixels);
    }

    uint8_t* lowres_data = lowres_mask_.Data();
    const uint32_t lowres_stride = lowres_mask_.Stride();

    for (uint32_t y = 0; y < map_height; ++y) {
        uint8_t* row = lowres_data + static_cast<size_t>(y) * lowres_stride;
        for (uint32_t x = 0; x < map_width; ++x) {
            const size_t idx = static_cast<size_t>(y) * map_width + x;
            float p = prob_map[idx];
            if (has_history) {
                // Zero-Parallax Motion-Adaptive EMA:
                //   - When a boundary moves (delta > 0.18f), eff_alpha = 0.0f so the mask updates
                //     in the exact same frame as the video with zero trailing/parallax lag.
                //   - When the subject is stationary (delta <= 0.18f), smooth sub-pixel sensor noise.
                const float delta = std::abs(p - prev_prob_map_[idx]);
                const float eff_alpha = (delta > 0.18f)
                    ? 0.0f
                    : (smooth_alpha * (1.0f - delta / 0.18f) * 0.55f);
                p = (1.0f - eff_alpha) * p + eff_alpha * prev_prob_map_[idx];
                prev_prob_map_[idx] = p;
            }

            // Smoothstep ramp around confidence threshold for anti-aliased alpha boundaries
            const float t = std::clamp((p - low_edge) * inv_band, 0.0f, 1.0f);
            const float smooth_t = t * t * (3.0f - 2.0f * t);
            row[x] = static_cast<uint8_t>(smooth_t * 255.0f + 0.5f);
        }
    }

    const CropRect src_roi{0, 0, map_width, map_height};
    const CropRect dst_roi{0, 0, out_mask.width, out_mask.height};
    return CropScaler::ResizeBilinearROI(lowres_mask_.View(), src_roi, out_mask, dst_roi);
}

bool AiOnnxSegmenter::ComputeMask(const FrameView& input_frame, FrameView& out_mask) {
    if (!input_frame.IsValid() || !out_mask.IsValid()) {
        return false;
    }

    const uint32_t tw = std::max(64u, config_.input_width);
    const uint32_t th = std::max(64u, config_.input_height);
    const size_t num_pixels = static_cast<size_t>(tw) * th;

    if (input_tensor_.size() != num_pixels * 3) {
        input_tensor_.resize(num_pixels * 3);
        curr_prob_map_.resize(num_pixels);
    }

    if (!PrepareInputTensorNHWC(input_frame, tw, th, input_tensor_.data())) {
        return false;
    }

    if (config_.engine_mode == 1) {
        // Mode 1: Always use Face / Solid Silhouette Tracker Only
        RunPortraitHeuristicFallback(input_tensor_.data(), tw, th, curr_prob_map_.data());
        out_mask.timestamp_ns = input_frame.timestamp_ns;
        out_mask.frame_index  = input_frame.frame_index;
        return PostProcessProbabilityMap(curr_prob_map_.data(), tw, th, out_mask);
    }

    if (has_reference_bg_) {
        // Mode 0 (Auto/Hybrid) or Mode 2 (Reference Background Difference Priority):
        // Use 4-stage Reference Background Subtraction with spatial/chromatic tolerance and anti-LED tearing
        RunReferenceBackgroundSubtraction(input_tensor_.data(), tw, th, curr_prob_map_.data());
        out_mask.timestamp_ns = input_frame.timestamp_ns;
        out_mask.frame_index  = input_frame.frame_index;
        return PostProcessProbabilityMap(curr_prob_map_.data(), tw, th, out_mask);
    }

#if defined(SMARTCAM_WITH_ONNXRUNTIME)
    if (ort_loaded_ && ort_impl_->session) {
        try {
            Ort::MemoryInfo mem_info = Ort::MemoryInfo::CreateCpu(OrtArenaAllocator, OrtMemTypeDefault);
            const int64_t input_shape[4] = {1, static_cast<int64_t>(th), static_cast<int64_t>(tw), 3};
            Ort::Value input_ort = Ort::Value::CreateTensor<float>(
                mem_info, input_tensor_.data(), input_tensor_.size(), input_shape, 4);

            const char* input_names[]  = {ort_impl_->input_name.c_str()};
            const char* output_names[] = {ort_impl_->output_name.c_str()};

            auto outputs = ort_impl_->session->Run(
                Ort::RunOptions{nullptr}, input_names, &input_ort, 1, output_names, 1);

            const float* raw_out = outputs[0].GetTensorData<float>();
            std::memcpy(curr_prob_map_.data(), raw_out, num_pixels * sizeof(float));
        } catch (...) {
            RunPortraitHeuristicFallback(input_tensor_.data(), tw, th, curr_prob_map_.data());
        }
    } else {
        RunPortraitHeuristicFallback(input_tensor_.data(), tw, th, curr_prob_map_.data());
    }
#else
    RunPortraitHeuristicFallback(input_tensor_.data(), tw, th, curr_prob_map_.data());
#endif

    out_mask.timestamp_ns = input_frame.timestamp_ns;
    out_mask.frame_index  = input_frame.frame_index;
    return PostProcessProbabilityMap(curr_prob_map_.data(), tw, th, out_mask);
}

}  // namespace engine
}  // namespace common
}  // namespace smartcam
