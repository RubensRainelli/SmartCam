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

#include "crop_scaler.h"
#include <algorithm>
#include <cstring>

namespace smartcam {
namespace common {
namespace engine {

void CropScaler::ResolveAspectRatio(AspectRatioMode mode,
                                    uint32_t src_width,
                                    uint32_t src_height,
                                    uint32_t custom_w,
                                    uint32_t custom_h,
                                    uint32_t& out_aspect_w,
                                    uint32_t& out_aspect_h) noexcept {
    switch (mode) {
        case AspectRatioMode::RATIO_16_9:
            out_aspect_w = 16;
            out_aspect_h = 9;
            break;
        case AspectRatioMode::RATIO_4_3:
            out_aspect_w = 4;
            out_aspect_h = 3;
            break;
        case AspectRatioMode::RATIO_1_1:
            out_aspect_w = 1;
            out_aspect_h = 1;
            break;
        case AspectRatioMode::RATIO_9_16:
            out_aspect_w = 9;
            out_aspect_h = 16;
            break;
        case AspectRatioMode::CUSTOM:
            out_aspect_w = (custom_w > 0) ? custom_w : 4;
            out_aspect_h = (custom_h > 0) ? custom_h : 3;
            break;
        case AspectRatioMode::NATIVE:
        default:
            out_aspect_w = (src_width > 0) ? src_width : 16;
            out_aspect_h = (src_height > 0) ? src_height : 9;
            break;
    }
}

CropRect CropScaler::ComputeCenterCropRect(uint32_t src_width,
                                           uint32_t src_height,
                                           uint32_t aspect_w,
                                           uint32_t aspect_h) noexcept {
    CropRect rect{0, 0, src_width, src_height};
    if (src_width == 0 || src_height == 0 || aspect_w == 0 || aspect_h == 0) {
        return rect;
    }

    // Compare src_width / src_height vs aspect_w / aspect_h using 64-bit integer cross-multiplication
    const uint64_t lhs = static_cast<uint64_t>(src_width) * aspect_h;
    const uint64_t rhs = static_cast<uint64_t>(src_height) * aspect_w;

    if (lhs > rhs) {
        // Source is wider than target aspect ratio (e.g. 16:9 input -> 4:3 target): crop horizontally
        uint32_t target_w = static_cast<uint32_t>(rhs / aspect_h);
        target_w &= ~1u;  // Ensure even pixel width for video encoders
        if (target_w == 0) target_w = src_width;
        rect.width  = target_w;
        rect.height = src_height;
        rect.x      = (src_width - target_w) / 2;
        rect.y      = 0;
    } else if (lhs < rhs) {
        // Source is taller than target aspect ratio: crop vertically
        uint32_t target_h = static_cast<uint32_t>(lhs / aspect_w);
        target_h &= ~1u;  // Ensure even pixel height
        if (target_h == 0) target_h = src_height;
        rect.width  = src_width;
        rect.height = target_h;
        rect.x      = 0;
        rect.y      = (src_height - target_h) / 2;
    }

    return rect;
}

bool CropScaler::ResizeBilinearROI(const FrameView& src,
                                   const CropRect& src_roi,
                                   FrameView& dst,
                                   const CropRect& dst_roi) noexcept {
    if (!src.IsValid() || !dst.IsValid() || src.format != dst.format) {
        return false;
    }
    if (src_roi.width == 0 || src_roi.height == 0 || dst_roi.width == 0 || dst_roi.height == 0) {
        return false;
    }
    if (src_roi.x + src_roi.width > src.width || src_roi.y + src_roi.height > src.height ||
        dst_roi.x + dst_roi.width > dst.width || dst_roi.y + dst_roi.height > dst.height) {
        return false;
    }

    const uint32_t bpp = GetBytesPerPixel(src.format);
    if (bpp != 4 && bpp != 1) {
        return false;
    }

    // Fast path: 1:1 pixel copy when source ROI and destination ROI have identical dimensions
    if (src_roi.width == dst_roi.width && src_roi.height == dst_roi.height) {
        const size_t row_bytes = static_cast<size_t>(dst_roi.width) * bpp;
        for (uint32_t y = 0; y < dst_roi.height; ++y) {
            const uint8_t* src_row = src.data + static_cast<size_t>(src_roi.y + y) * src.stride +
                                     static_cast<size_t>(src_roi.x) * bpp;
            uint8_t* dst_row = dst.data + static_cast<size_t>(dst_roi.y + y) * dst.stride +
                               static_cast<size_t>(dst_roi.x) * bpp;
            std::memcpy(dst_row, src_row, row_bytes);
        }
        return true;
    }

    // 16.16 fixed-point bilinear resampling
    const uint32_t scale_x = (src_roi.width << 16) / dst_roi.width;
    const uint32_t scale_y = (src_roi.height << 16) / dst_roi.height;
    const uint32_t max_src_x = src_roi.width - 1;
    const uint32_t max_src_y = src_roi.height - 1;

    for (uint32_t dy = 0; dy < dst_roi.height; ++dy) {
        const uint32_t sy_fp = dy * scale_y;
        const uint32_t sy0   = std::min(sy_fp >> 16, max_src_y);
        const uint32_t sy1   = std::min(sy0 + 1, max_src_y);
        const uint32_t wy    = (sy_fp >> 8) & 0xFFu;
        const uint32_t inv_wy = 256u - wy;

        const uint8_t* row0 = src.data + static_cast<size_t>(src_roi.y + sy0) * src.stride +
                              static_cast<size_t>(src_roi.x) * bpp;
        const uint8_t* row1 = src.data + static_cast<size_t>(src_roi.y + sy1) * src.stride +
                              static_cast<size_t>(src_roi.x) * bpp;
        uint8_t* dst_row = dst.data + static_cast<size_t>(dst_roi.y + dy) * dst.stride +
                           static_cast<size_t>(dst_roi.x) * bpp;

        if (bpp == 4) {
            for (uint32_t dx = 0; dx < dst_roi.width; ++dx) {
                const uint32_t sx_fp  = dx * scale_x;
                const uint32_t sx0    = std::min(sx_fp >> 16, max_src_x);
                const uint32_t sx1    = std::min(sx0 + 1, max_src_x);
                const uint32_t wx     = (sx_fp >> 8) & 0xFFu;
                const uint32_t inv_wx = 256u - wx;

                const uint32_t w00 = inv_wx * inv_wy;
                const uint32_t w10 = wx * inv_wy;
                const uint32_t w01 = inv_wx * wy;
                const uint32_t w11 = wx * wy;

                const uint8_t* p00 = row0 + (sx0 << 2);
                const uint8_t* p10 = row0 + (sx1 << 2);
                const uint8_t* p01 = row1 + (sx0 << 2);
                const uint8_t* p11 = row1 + (sx1 << 2);
                uint8_t* out_px    = dst_row + (dx << 2);

                out_px[0] = static_cast<uint8_t>((p00[0] * w00 + p10[0] * w10 + p01[0] * w01 + p11[0] * w11) >> 16);
                out_px[1] = static_cast<uint8_t>((p00[1] * w00 + p10[1] * w10 + p01[1] * w01 + p11[1] * w11) >> 16);
                out_px[2] = static_cast<uint8_t>((p00[2] * w00 + p10[2] * w10 + p01[2] * w01 + p11[2] * w11) >> 16);
                out_px[3] = static_cast<uint8_t>((p00[3] * w00 + p10[3] * w10 + p01[3] * w01 + p11[3] * w11) >> 16);
            }
        } else {
            for (uint32_t dx = 0; dx < dst_roi.width; ++dx) {
                const uint32_t sx_fp  = dx * scale_x;
                const uint32_t sx0    = std::min(sx_fp >> 16, max_src_x);
                const uint32_t sx1    = std::min(sx0 + 1, max_src_x);
                const uint32_t wx     = (sx_fp >> 8) & 0xFFu;
                const uint32_t inv_wx = 256u - wx;

                const uint32_t val =
                    (row0[sx0] * inv_wx + row0[sx1] * wx) * inv_wy +
                    (row1[sx0] * inv_wx + row1[sx1] * wx) * wy;
                dst_row[dx] = static_cast<uint8_t>(val >> 16);
            }
        }
    }

    return true;
}

bool CropScaler::Process(const FrameView& src,
                         FrameView& dst,
                         AspectRatioMode aspect_mode,
                         CropStrategy strategy,
                         uint32_t custom_aspect_w,
                         uint32_t custom_aspect_h) noexcept {
    if (!src.IsValid() || !dst.IsValid() || src.format != dst.format) {
        return false;
    }

    uint32_t aspect_w = 4;
    uint32_t aspect_h = 3;
    ResolveAspectRatio(aspect_mode, src.width, src.height, custom_aspect_w, custom_aspect_h, aspect_w, aspect_h);

    if (strategy == CropStrategy::CENTER_CROP) {
        const CropRect src_crop = ComputeCenterCropRect(src.width, src.height, aspect_w, aspect_h);
        const CropRect dst_full{0, 0, dst.width, dst.height};
        dst.timestamp_ns = src.timestamp_ns;
        dst.frame_index  = src.frame_index;
        return ResizeBilinearROI(src, src_crop, dst, dst_full);
    } else {
        // LETTERBOX / PILLARBOX: clear destination frame to opaque black first
        const uint32_t bpp = GetBytesPerPixel(dst.format);
        for (uint32_t y = 0; y < dst.height; ++y) {
            uint8_t* row = dst.data + static_cast<size_t>(y) * dst.stride;
            if (bpp == 4) {
                for (uint32_t x = 0; x < dst.width; ++x) {
                    row[x * 4 + 0] = 0;
                    row[x * 4 + 1] = 0;
                    row[x * 4 + 2] = 0;
                    row[x * 4 + 3] = 255;
                }
            } else {
                std::memset(row, 0, dst.width * bpp);
            }
        }

        // Fit full source aspect inside dst_box
        const CropRect src_full{0, 0, src.width, src.height};
        const CropRect dst_box = ComputeCenterCropRect(dst.width, dst.height, src.width, src.height);
        dst.timestamp_ns = src.timestamp_ns;
        dst.frame_index  = src.frame_index;
        return ResizeBilinearROI(src, src_full, dst, dst_box);
    }
}

}  // namespace engine
}  // namespace common
}  // namespace smartcam
