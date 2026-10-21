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

#include "green_screen_segmenter.h"
#include <algorithm>
#include <cmath>
#include <cstring>

namespace smartcam {
namespace common {
namespace engine {

GreenScreenSegmenter::GreenScreenSegmenter(const GreenScreenConfig& config) noexcept
    : config_(config) {}

bool GreenScreenSegmenter::ComputeMask(const FrameView& input_frame, FrameView& out_mask) {
    if (!input_frame.IsValid() || !out_mask.IsValid()) {
        return false;
    }
    if (input_frame.width != out_mask.width || input_frame.height != out_mask.height) {
        return false;
    }
    if ((input_frame.format != PixelFormat::BGRA32 && input_frame.format != PixelFormat::RGBA32) ||
        out_mask.format != PixelFormat::GRAY8) {
        return false;
    }

    const uint32_t w       = input_frame.width;
    const uint32_t h       = input_frame.height;
    const bool     is_bgra = (input_frame.format == PixelFormat::BGRA32);

    const uint8_t h_min = config_.hue_min;
    const uint8_t h_max = config_.hue_max;
    const uint8_t s_min = config_.sat_min;
    const uint8_t s_max = config_.sat_max;
    const uint8_t v_min = config_.val_min;
    const uint8_t v_max = config_.val_max;

    // 1. HSV Thresholding pass (255 = Foreground Person, 0 = Green Screen Background)
    for (uint32_t y = 0; y < h; ++y) {
        const uint8_t* src_row = input_frame.data + static_cast<size_t>(y) * input_frame.stride;
        uint8_t*       dst_row = out_mask.data + static_cast<size_t>(y) * out_mask.stride;

        for (uint32_t x = 0; x < w; ++x) {
            const uint8_t c0 = src_row[x * 4 + 0];
            const uint8_t c1 = src_row[x * 4 + 1];
            const uint8_t c2 = src_row[x * 4 + 2];

            const uint8_t r = is_bgra ? c2 : c0;
            const uint8_t g = c1;
            const uint8_t b = is_bgra ? c0 : c2;

            uint8_t hue = 0, sat = 0, val = 0;
            RgbToHsvFast(r, g, b, hue, sat, val);

            const bool is_green_bg =
                (hue >= h_min && hue <= h_max) &&
                (sat >= s_min && sat <= s_max) &&
                (val >= v_min && val <= v_max);

            dst_row[x] = is_green_bg ? 0u : 255u;
        }
    }

    // 2. Separable Morphological Opening/Closing to remove isolated chroma noise
    if (config_.morph_kernel_size >= 3) {
        const uint8_t morph_radius = static_cast<uint8_t>(config_.morph_kernel_size / 2);
        MorphologicalOpen(out_mask.data, w, h, out_mask.stride, morph_radius);
    }

    // 3. Separable Gaussian-approximate Soft-Edge Blur (2-pass sliding box filter)
    if (config_.blur_radius > 0) {
        SeparableSoftBlur(out_mask.data, w, h, out_mask.stride, config_.blur_radius);
        SeparableSoftBlur(out_mask.data, w, h, out_mask.stride, config_.blur_radius);
    }

    out_mask.timestamp_ns = input_frame.timestamp_ns;
    out_mask.frame_index  = input_frame.frame_index;
    return true;
}

void GreenScreenSegmenter::MorphologicalOpen(uint8_t* mask,
                                             uint32_t width,
                                             uint32_t height,
                                             uint32_t stride,
                                             uint8_t radius) {
    const size_t total = static_cast<size_t>(width) * height;
    if (scratch_a_.size() < total) scratch_a_.resize(total);
    if (scratch_b_.size() < total) scratch_b_.resize(total);

    const int32_t r = static_cast<int32_t>(radius);
    const int32_t w = static_cast<int32_t>(width);
    const int32_t h = static_cast<int32_t>(height);

    // Step A: Separable Erosion (min filter) -> removes tiny foreground speckles in green background
    for (int32_t y = 0; y < h; ++y) {
        const uint8_t* src_row = mask + static_cast<size_t>(y) * stride;
        uint8_t*       tmp_row = scratch_a_.data() + static_cast<size_t>(y) * width;
        for (int32_t x = 0; x < w; ++x) {
            uint8_t vmin = 255;
            const int32_t x0 = std::max(0, x - r);
            const int32_t x1 = std::min(w - 1, x + r);
            for (int32_t kx = x0; kx <= x1; ++kx) {
                vmin = std::min(vmin, src_row[kx]);
            }
            tmp_row[x] = vmin;
        }
    }
    for (int32_t y = 0; y < h; ++y) {
        uint8_t* eroded_row = scratch_b_.data() + static_cast<size_t>(y) * width;
        const int32_t y0    = std::max(0, y - r);
        const int32_t y1    = std::min(h - 1, y + r);
        for (int32_t x = 0; x < w; ++x) {
            uint8_t vmin = 255;
            for (int32_t ky = y0; ky <= y1; ++ky) {
                vmin = std::min(vmin, scratch_a_[static_cast<size_t>(ky) * width + x]);
            }
            eroded_row[x] = vmin;
        }
    }

    // Step B: Separable Dilation (max filter) -> restores true foreground boundary
    for (int32_t y = 0; y < h; ++y) {
        const uint8_t* src_row = scratch_b_.data() + static_cast<size_t>(y) * width;
        uint8_t*       tmp_row = scratch_a_.data() + static_cast<size_t>(y) * width;
        for (int32_t x = 0; x < w; ++x) {
            uint8_t vmax = 0;
            const int32_t x0 = std::max(0, x - r);
            const int32_t x1 = std::min(w - 1, x + r);
            for (int32_t kx = x0; kx <= x1; ++kx) {
                vmax = std::max(vmax, src_row[kx]);
            }
            tmp_row[x] = vmax;
        }
    }
    for (int32_t y = 0; y < h; ++y) {
        uint8_t* dst_row = mask + static_cast<size_t>(y) * stride;
        const int32_t y0 = std::max(0, y - r);
        const int32_t y1 = std::min(h - 1, y + r);
        for (int32_t x = 0; x < w; ++x) {
            uint8_t vmax = 0;
            for (int32_t ky = y0; ky <= y1; ++ky) {
                vmax = std::max(vmax, scratch_a_[static_cast<size_t>(ky) * width + x]);
            }
            dst_row[x] = vmax;
        }
    }
}

void GreenScreenSegmenter::SeparableSoftBlur(uint8_t* mask,
                                             uint32_t width,
                                             uint32_t height,
                                             uint32_t stride,
                                             uint8_t radius) {
    const size_t total = static_cast<size_t>(width) * height;
    if (scratch_a_.size() < total) scratch_a_.resize(total);

    const int32_t r     = static_cast<int32_t>(std::clamp<uint8_t>(radius, 1, 32));
    const uint32_t diam = static_cast<uint32_t>(2 * r + 1);
    const int32_t w     = static_cast<int32_t>(width);
    const int32_t h     = static_cast<int32_t>(height);

    // Horizontal O(1) sliding window pass
    for (int32_t y = 0; y < h; ++y) {
        const uint8_t* src_row = mask + static_cast<size_t>(y) * stride;
        uint8_t*       tmp_row = scratch_a_.data() + static_cast<size_t>(y) * width;

        uint32_t sum = 0;
        for (int32_t k = -r; k <= r; ++k) {
            sum += src_row[std::clamp(k, 0, w - 1)];
        }
        for (int32_t x = 0; x < w; ++x) {
            tmp_row[x] = static_cast<uint8_t>(sum / diam);
            const int32_t left_x  = std::max(0, x - r);
            const int32_t right_x = std::min(w - 1, x + r + 1);
            sum += src_row[right_x] - src_row[left_x];
        }
    }

    // Vertical O(1) sliding window pass
    for (int32_t x = 0; x < w; ++x) {
        uint32_t sum = 0;
        for (int32_t k = -r; k <= r; ++k) {
            sum += scratch_a_[static_cast<size_t>(std::clamp(k, 0, h - 1)) * width + x];
        }
        for (int32_t y = 0; y < h; ++y) {
            mask[static_cast<size_t>(y) * stride + x] = static_cast<uint8_t>(sum / diam);
            const int32_t top_y = std::max(0, y - r);
            const int32_t bot_y = std::min(h - 1, y + r + 1);
            sum += scratch_a_[static_cast<size_t>(bot_y) * width + x] -
                   scratch_a_[static_cast<size_t>(top_y) * width + x];
        }
    }
}

void GreenScreenSegmenter::SuppressGreenSpill(FrameView& frame, const FrameView& mask) const noexcept {
    if (!frame.IsValid() || !mask.IsValid() || config_.spill_suppression <= 0.0f) {
        return;
    }
    if (frame.width != mask.width || frame.height != mask.height) {
        return;
    }

    const bool is_bgra = (frame.format == PixelFormat::BGRA32);
    const uint32_t strength_fp = static_cast<uint32_t>(std::clamp(config_.spill_suppression, 0.0f, 1.0f) * 256.0f);

    for (uint32_t y = 0; y < frame.height; ++y) {
        uint8_t*       px_row = frame.data + static_cast<size_t>(y) * frame.stride;
        const uint8_t* m_row  = mask.data + static_cast<size_t>(y) * mask.stride;

        for (uint32_t x = 0; x < frame.width; ++x) {
            const uint8_t alpha = m_row[x];
            if (alpha == 0) continue;

            uint8_t& c0 = px_row[x * 4 + 0];
            uint8_t& g  = px_row[x * 4 + 1];
            uint8_t& c2 = px_row[x * 4 + 2];

            const uint8_t r = is_bgra ? c2 : c0;
            const uint8_t b = is_bgra ? c0 : c2;
            const uint8_t rb_avg = static_cast<uint8_t>((static_cast<uint32_t>(r) + b) >> 1);

            if (g > rb_avg) {
                const uint32_t excess = g - rb_avg;
                g = static_cast<uint8_t>(g - ((excess * strength_fp) >> 8));
            }
        }
    }
}

bool GreenScreenSegmenter::AutoCalibrate(const FrameView& input_frame,
                                         int32_t sample_x,
                                         int32_t sample_y,
                                         uint8_t* out_hue_min,
                                         uint8_t* out_hue_max,
                                         uint8_t* out_sat_min,
                                         uint8_t* out_val_min) noexcept {
    if (!input_frame.IsValid() || input_frame.width == 0 || input_frame.height == 0) {
        return false;
    }
    if (input_frame.format != PixelFormat::BGRA32 && input_frame.format != PixelFormat::RGBA32) {
        return false;
    }

    const int32_t w = static_cast<int32_t>(input_frame.width);
    const int32_t h = static_cast<int32_t>(input_frame.height);
    const bool is_bgra = (input_frame.format == PixelFormat::BGRA32);

    std::vector<uint8_t> h_samples;
    std::vector<uint8_t> s_samples;
    std::vector<uint8_t> v_samples;
    h_samples.reserve(2048);
    s_samples.reserve(2048);
    v_samples.reserve(2048);

    auto sample_rect = [&](int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
        x0 = std::clamp(x0, 0, w - 1);
        y0 = std::clamp(y0, 0, h - 1);
        x1 = std::clamp(x1, x0, w - 1);
        y1 = std::clamp(y1, y0, h - 1);

        const int32_t step_x = std::max(1, (x1 - x0 + 1) / 32);
        const int32_t step_y = std::max(1, (y1 - y0 + 1) / 32);

        for (int32_t y = y0; y <= y1; y += step_y) {
            const uint8_t* row = input_frame.data + static_cast<size_t>(y) * input_frame.stride;
            for (int32_t x = x0; x <= x1; x += step_x) {
                const uint8_t c0 = row[x * 4 + 0];
                const uint8_t c1 = row[x * 4 + 1];
                const uint8_t c2 = row[x * 4 + 2];

                const uint8_t r = is_bgra ? c2 : c0;
                const uint8_t g = c1;
                const uint8_t b = is_bgra ? c0 : c2;

                uint8_t hue = 0, sat = 0, val = 0;
                RgbToHsvFast(r, g, b, hue, sat, val);
                h_samples.push_back(hue);
                s_samples.push_back(sat);
                v_samples.push_back(val);
            }
        }
    };

    if (sample_x >= 0 && sample_y >= 0) {
        // Eyedropper mode: sample patch around clicked point (sample_x, sample_y)
        const int32_t radius = std::max(4, std::min(w, h) / 32);
        sample_rect(sample_x - radius, sample_y - radius, sample_x + radius, sample_y + radius);
    } else {
        // Automatic 1-click mode: sample upper-left and upper-right background corner regions
        const int32_t margin_x = std::max(1, w / 32);
        const int32_t margin_y = std::max(1, h / 32);
        const int32_t patch_w  = std::max(4, w / 6);
        const int32_t patch_h  = std::max(4, h / 5);

        sample_rect(margin_x, margin_y, margin_x + patch_w, margin_y + patch_h);
        sample_rect(w - 1 - margin_x - patch_w, margin_y, w - 1 - margin_x, margin_y + patch_h);
    }

    if (h_samples.empty()) {
        return false;
    }

    std::sort(h_samples.begin(), h_samples.end());
    std::sort(s_samples.begin(), s_samples.end());
    std::sort(v_samples.begin(), v_samples.end());

    const size_t n = h_samples.size();
    const double med_h = static_cast<double>(h_samples[n / 2]);
    const double med_s = static_cast<double>(s_samples[n / 2]);
    const double med_v = static_cast<double>(v_samples[n / 2]);

    double sum_h = 0.0, sum_s = 0.0, sum_v = 0.0;
    for (size_t i = 0; i < n; ++i) {
        sum_h += h_samples[i];
        sum_s += s_samples[i];
        sum_v += v_samples[i];
    }
    const double mean_h = sum_h / static_cast<double>(n);
    const double mean_s = sum_s / static_cast<double>(n);
    const double mean_v = sum_v / static_cast<double>(n);

    double var_h = 0.0, var_s = 0.0, var_v = 0.0;
    for (size_t i = 0; i < n; ++i) {
        const double dh = static_cast<double>(h_samples[i]) - mean_h;
        const double ds = static_cast<double>(s_samples[i]) - mean_s;
        const double dv = static_cast<double>(v_samples[i]) - mean_v;
        var_h += dh * dh;
        var_s += ds * ds;
        var_v += dv * dv;
    }
    const double std_h = std::sqrt(var_h / static_cast<double>(n));
    const double std_s = std::sqrt(var_s / static_cast<double>(n));
    const double std_v = std::sqrt(var_v / static_cast<double>(n));

    const double p05_h = static_cast<double>(h_samples[n / 20]);
    const double p95_h = static_cast<double>(h_samples[(n * 19) / 20]);
    const double p05_s = static_cast<double>(s_samples[n / 20]);
    const double p05_v = static_cast<double>(v_samples[n / 20]);

    const double h_tol = std::max(12.0, 2.5 * std_h + 6.0);
    const double s_tol = std::max(35.0, 3.0 * std_s + 15.0);
    const double v_tol = std::max(45.0, 3.0 * std_v + 20.0);

    const int32_t calc_h_min = static_cast<int32_t>(std::round(std::min(p05_h - 8.0, med_h - h_tol)));
    const int32_t calc_h_max = static_cast<int32_t>(std::round(std::max(p95_h + 8.0, med_h + h_tol)));
    const int32_t calc_s_min = static_cast<int32_t>(std::round(std::min(p05_s - 15.0, med_s - s_tol)));
    const int32_t calc_v_min = static_cast<int32_t>(std::round(std::min(p05_v - 20.0, med_v - v_tol)));

    config_.hue_min = static_cast<uint8_t>(std::clamp(calc_h_min, 0, 180));
    config_.hue_max = static_cast<uint8_t>(std::clamp(calc_h_max, static_cast<int32_t>(config_.hue_min), 180));
    config_.sat_min = static_cast<uint8_t>(std::clamp(calc_s_min, 10, 240));
    config_.sat_max = 255;
    config_.val_min = static_cast<uint8_t>(std::clamp(calc_v_min, 15, 240));
    config_.val_max = 255;

    if (out_hue_min) *out_hue_min = config_.hue_min;
    if (out_hue_max) *out_hue_max = config_.hue_max;
    if (out_sat_min) *out_sat_min = config_.sat_min;
    if (out_val_min) *out_val_min = config_.val_min;

    return true;
}

}  // namespace engine
}  // namespace common
}  // namespace smartcam
