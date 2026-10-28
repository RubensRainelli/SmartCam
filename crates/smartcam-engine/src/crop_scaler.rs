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

//! Aspect Ratio Crop & Bilinear Scaler (`smartcam-engine`).
//! Provides distortion-free CenterCrop, Letterbox, and Social Crop/Zoom.

use crate::{AspectRatioMode, CropStrategy, SimdTier};

/// Axis-aligned rectangle describing a crop region inside a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Resolves standard `AspectRatioMode` enum into numerator/denominator pair.
pub fn resolve_aspect_ratio(
    mode: AspectRatioMode,
    src_width: u32,
    src_height: u32,
    custom_w: u32,
    custom_h: u32,
) -> (u32, u32) {
    match mode {
        AspectRatioMode::Ratio16x9 => (16, 9),
        AspectRatioMode::Ratio4x3 => (4, 3),
        AspectRatioMode::Ratio1x1 => (1, 1),
        AspectRatioMode::Ratio9x16 => (9, 16),
        AspectRatioMode::Custom => (
            if custom_w > 0 { custom_w } else { 4 },
            if custom_h > 0 { custom_h } else { 3 },
        ),
        AspectRatioMode::Native => (
            if src_width > 0 { src_width } else { 16 },
            if src_height > 0 { src_height } else { 9 },
        ),
    }
}

/// Computes the largest centered crop rectangle inside `(src_width, src_height)`
/// that matches the requested aspect ratio `aspect_w : aspect_h` without geometric distortion.
pub fn compute_center_crop_rect(
    src_width: u32,
    src_height: u32,
    aspect_w: u32,
    aspect_h: u32,
) -> CropRect {
    let mut rect = CropRect {
        x: 0,
        y: 0,
        width: src_width,
        height: src_height,
    };
    if src_width == 0 || src_height == 0 || aspect_w == 0 || aspect_h == 0 {
        return rect;
    }

    let lhs = (src_width as u64) * (aspect_h as u64);
    let rhs = (src_height as u64) * (aspect_w as u64);

    if lhs > rhs {
        // Source is wider than target aspect: crop horizontally
        let mut target_w = (rhs / (aspect_h as u64)) as u32;
        target_w &= !1u32;
        if target_w == 0 {
            target_w = src_width;
        }
        rect.width = target_w;
        rect.height = src_height;
        rect.x = (src_width - target_w) / 2;
        rect.y = 0;
    } else if lhs < rhs {
        // Source is taller than target aspect: crop vertically
        let mut target_h = (lhs / (aspect_w as u64)) as u32;
        target_h &= !1u32;
        if target_h == 0 {
            target_h = src_height;
        }
        rect.width = src_width;
        rect.height = target_h;
        rect.x = 0;
        rect.y = (src_height - target_h) / 2;
    }

    rect
}

/// Computes the exact source crop rectangle `(crop_x, crop_y, crop_w, crop_h)` inside `(src_w, src_h)`
/// matching target aspect ratio `(aspect_w : aspect_h)`, zoom factor `[1.0..=4.0]`, and pan `[0.0..=1.0]`.
pub fn compute_social_crop_rect(
    src_w: u32,
    src_h: u32,
    aspect_w: u32,
    aspect_h: u32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> (u32, u32, u32, u32) {
    if src_w == 0 || src_h == 0 || aspect_w == 0 || aspect_h == 0 {
        return (0, 0, src_w.max(1), src_h.max(1));
    }
    let rect = compute_center_crop_rect(src_w, src_h, aspect_w, aspect_h);
    let z = zoom.clamp(1.0, 4.0);
    let cw = ((rect.width as f32) / z).round().max(16.0).min(src_w as f32) as u32;
    let ch = ((rect.height as f32) / z).round().max(16.0).min(src_h as f32) as u32;

    let max_x = src_w.saturating_sub(cw);
    let max_y = src_h.saturating_sub(ch);
    let cx = ((max_x as f32) * pan_x.clamp(0.0, 1.0)).round() as u32;
    let cy = ((max_y as f32) * pan_y.clamp(0.0, 1.0)).round() as u32;
    (cx.min(max_x), cy.min(max_y), cw, ch)
}

/// Crops, optionally mirrors horizontally/vertically, and bilinearly scales `src` (`src_w x src_h`)
/// into `dst` (`dst_w x dst_h`) using Social Crop/Zoom parameters.
pub fn crop_zoom_image_social(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    mirror_h: bool,
    mirror_v: bool,
) -> (u32, u32, u32, u32) {
    let (cx, cy, cw, ch) = compute_social_crop_rect(src_w, src_h, dst_w, dst_h, zoom, pan_x, pan_y);
    if src.len() < (src_w * src_h * 4) as usize || dst.len() < (dst_w * dst_h * 4) as usize {
        return (cx, cy, cw, ch);
    }

    let max_sx = src_w.saturating_sub(1);
    let max_sy = src_h.saturating_sub(1);

    for y in 0..dst_h {
        let fy = (cy as f32) + ((y as f32) + 0.5) * (ch as f32) / (dst_h.max(1) as f32) - 0.5;
        let raw_y0 = (fy.floor().max(0.0) as u32).min(max_sy);
        let raw_y1 = (raw_y0 + 1).min(max_sy);
        let wy = ((fy - (raw_y0 as f32)).clamp(0.0, 1.0) * 256.0) as u32;
        let inv_wy = 256 - wy;
        let y0 = if mirror_v { max_sy - raw_y0 } else { raw_y0 };
        let y1 = if mirror_v { max_sy - raw_y1 } else { raw_y1 };

        for x in 0..dst_w {
            let fx = (cx as f32) + ((x as f32) + 0.5) * (cw as f32) / (dst_w.max(1) as f32) - 0.5;
            let raw_x0 = (fx.floor().max(0.0) as u32).min(max_sx);
            let raw_x1 = (raw_x0 + 1).min(max_sx);
            let wx = ((fx - (raw_x0 as f32)).clamp(0.0, 1.0) * 256.0) as u32;
            let inv_wx = 256 - wx;
            let x0 = if mirror_h { max_sx - raw_x0 } else { raw_x0 };
            let x1 = if mirror_h { max_sx - raw_x1 } else { raw_x1 };

            let w00 = inv_wx * inv_wy;
            let w10 = wx * inv_wy;
            let w01 = inv_wx * wy;
            let w11 = wx * wy;

            let i00 = ((y0 * src_w + x0) * 4) as usize;
            let i10 = ((y0 * src_w + x1) * 4) as usize;
            let i01 = ((y1 * src_w + x0) * 4) as usize;
            let i11 = ((y1 * src_w + x1) * 4) as usize;
            let d_idx = ((y * dst_w + x) * 4) as usize;

            for c in 0..3 {
                let val = (src[i00 + c] as u32) * w00
                    + (src[i10 + c] as u32) * w10
                    + (src[i01 + c] as u32) * w01
                    + (src[i11 + c] as u32) * w11;
                dst[d_idx + c] = (val >> 16) as u8;
            }
            dst[d_idx + 3] = 255;
        }
    }
    (cx, cy, cw, ch)
}

/// Bilinear ROI resizing supporting 4-channel BGRA/RGBA and 1-channel GRAY8 (Scalar path).
pub fn resize_bilinear_roi_scalar(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    src_roi: CropRect,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    dst_roi: CropRect,
    bpp: u32,
) -> bool {
    if src_roi.width == 0 || src_roi.height == 0 || dst_roi.width == 0 || dst_roi.height == 0 {
        return false;
    }
    if src_roi.x + src_roi.width > src_w || src_roi.y + src_roi.height > src_h {
        return false;
    }
    if dst_roi.x + dst_roi.width > dst_w || dst_roi.y + dst_roi.height > dst_h {
        return false;
    }

    let bpp = bpp as usize;
    if bpp != 4 && bpp != 1 {
        return false;
    }

    // Fast 1:1 scanline copy when source and destination ROI dimensions match exactly
    if src_roi.width == dst_roi.width && src_roi.height == dst_roi.height {
        let row_bytes = src_roi.width as usize * bpp;
        for y in 0..dst_roi.height {
            let s_off = ((src_roi.y + y) * stride_src + src_roi.x * bpp as u32) as usize;
            let d_off = ((dst_roi.y + y) * stride_dst + dst_roi.x * bpp as u32) as usize;
            dst[d_off..d_off + row_bytes].copy_from_slice(&src[s_off..s_off + row_bytes]);
        }
        return true;
    }

    // 16.16 fixed-point bilinear interpolation
    let scale_x = ((src_roi.width as u64) << 16) / (dst_roi.width as u64);
    let scale_y = ((src_roi.height as u64) << 16) / (dst_roi.height as u64);
    let max_src_x = src_roi.width.saturating_sub(1);
    let max_src_y = src_roi.height.saturating_sub(1);

    for dy in 0..dst_roi.height {
        let sy_fp = (dy as u64) * scale_y;
        let sy0 = ((sy_fp >> 16) as u32).min(max_src_y);
        let sy1 = (sy0 + 1).min(max_src_y);
        let wy = ((sy_fp >> 8) & 0xFF) as u32;
        let inv_wy = 256 - wy;

        let row0_off = ((src_roi.y + sy0) * stride_src + src_roi.x * bpp as u32) as usize;
        let row1_off = ((src_roi.y + sy1) * stride_src + src_roi.x * bpp as u32) as usize;
        let dst_row_off = ((dst_roi.y + dy) * stride_dst + dst_roi.x * bpp as u32) as usize;

        if bpp == 4 {
            for dx in 0..dst_roi.width {
                let sx_fp = (dx as u64) * scale_x;
                let sx0 = ((sx_fp >> 16) as u32).min(max_src_x);
                let sx1 = (sx0 + 1).min(max_src_x);
                let wx = ((sx_fp >> 8) & 0xFF) as u32;
                let inv_wx = 256 - wx;

                let w00 = inv_wx * inv_wy;
                let w10 = wx * inv_wy;
                let w01 = inv_wx * wy;
                let w11 = wx * wy;

                let p00 = row0_off + (sx0 as usize) * 4;
                let p10 = row0_off + (sx1 as usize) * 4;
                let p01 = row1_off + (sx0 as usize) * 4;
                let p11 = row1_off + (sx1 as usize) * 4;
                let out_px = dst_row_off + (dx as usize) * 4;

                dst[out_px + 0] = (((src[p00 + 0] as u32) * w00
                    + (src[p10 + 0] as u32) * w10
                    + (src[p01 + 0] as u32) * w01
                    + (src[p11 + 0] as u32) * w11)
                    >> 16) as u8;
                dst[out_px + 1] = (((src[p00 + 1] as u32) * w00
                    + (src[p10 + 1] as u32) * w10
                    + (src[p01 + 1] as u32) * w01
                    + (src[p11 + 1] as u32) * w11)
                    >> 16) as u8;
                dst[out_px + 2] = (((src[p00 + 2] as u32) * w00
                    + (src[p10 + 2] as u32) * w10
                    + (src[p01 + 2] as u32) * w01
                    + (src[p11 + 2] as u32) * w11)
                    >> 16) as u8;
                dst[out_px + 3] = (((src[p00 + 3] as u32) * w00
                    + (src[p10 + 3] as u32) * w10
                    + (src[p01 + 3] as u32) * w01
                    + (src[p11 + 3] as u32) * w11)
                    >> 16) as u8;
            }
        } else {
            for dx in 0..dst_roi.width {
                let sx_fp = (dx as u64) * scale_x;
                let sx0 = ((sx_fp >> 16) as u32).min(max_src_x);
                let sx1 = (sx0 + 1).min(max_src_x);
                let wx = ((sx_fp >> 8) & 0xFF) as u32;
                let inv_wx = 256 - wx;

                let val = ((src[row0_off + sx0 as usize] as u32 * inv_wx
                    + src[row0_off + sx1 as usize] as u32 * wx)
                    * inv_wy
                    + (src[row1_off + sx0 as usize] as u32 * inv_wx
                        + src[row1_off + sx1 as usize] as u32 * wx)
                        * wy)
                    >> 16;
                dst[dst_row_off + dx as usize] = val as u8;
            }
        }
    }

    true
}

/// Bilinear ROI resizing with AVX2 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn resize_bilinear_roi_avx2(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    src_roi: CropRect,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    dst_roi: CropRect,
    bpp: u32,
) -> bool {
    resize_bilinear_roi_scalar(
        src, src_w, src_h, stride_src, src_roi, dst, dst_w, dst_h, stride_dst, dst_roi, bpp,
    )
}

/// Bilinear ROI resizing with AVX-512 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
pub unsafe fn resize_bilinear_roi_avx512(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    src_roi: CropRect,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    dst_roi: CropRect,
    bpp: u32,
) -> bool {
    resize_bilinear_roi_scalar(
        src, src_w, src_h, stride_src, src_roi, dst, dst_w, dst_h, stride_dst, dst_roi, bpp,
    )
}

/// Bilinear ROI resizing with runtime SIMD dispatch.
pub fn resize_bilinear_roi(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    src_roi: CropRect,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    dst_roi: CropRect,
    bpp: u32,
    requested_tier: SimdTier,
) -> bool {
    let tier = match requested_tier {
        SimdTier::Auto => crate::detect_best_simd_tier(),
        t => t,
    };

    #[cfg(target_arch = "x86_64")]
    {
        if tier == SimdTier::Avx512
            && std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
        {
            return unsafe {
                resize_bilinear_roi_avx512(
                    src, src_w, src_h, stride_src, src_roi, dst, dst_w, dst_h, stride_dst, dst_roi,
                    bpp,
                )
            };
        }
        if (tier == SimdTier::Avx2 || tier == SimdTier::Avx512)
            && std::arch::is_x86_feature_detected!("avx2")
        {
            return unsafe {
                resize_bilinear_roi_avx2(
                    src, src_w, src_h, stride_src, src_roi, dst, dst_w, dst_h, stride_dst, dst_roi,
                    bpp,
                )
            };
        }
    }

    let _ = tier;
    resize_bilinear_roi_scalar(
        src, src_w, src_h, stride_src, src_roi, dst, dst_w, dst_h, stride_dst, dst_roi, bpp,
    )
}

/// Transforms `src` into `dst` applying CenterCrop or Letterbox strategy with 16.16 fixed-point bilinear interpolation (Scalar path).
pub fn crop_scale_bgra_scalar(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    aspect_w: u32,
    aspect_h: u32,
    strategy: CropStrategy,
) -> bool {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return false;
    }

    match strategy {
        CropStrategy::CenterCrop => {
            let src_crop = compute_center_crop_rect(src_w, src_h, aspect_w, aspect_h);
            let dst_full = CropRect {
                x: 0,
                y: 0,
                width: dst_w,
                height: dst_h,
            };
            resize_bilinear_roi_scalar(
                src, src_w, src_h, stride_src, src_crop, dst, dst_w, dst_h, stride_dst, dst_full,
                4,
            )
        }
        CropStrategy::Letterbox => {
            // Fill background with black
            for y in 0..dst_h {
                let off = (y * stride_dst) as usize;
                for x in 0..dst_w {
                    let px = off + (x as usize) * 4;
                    dst[px + 0] = 0;
                    dst[px + 1] = 0;
                    dst[px + 2] = 0;
                    dst[px + 3] = 255;
                }
            }

            let src_full = CropRect {
                x: 0,
                y: 0,
                width: src_w,
                height: src_h,
            };
            let dst_box = compute_center_crop_rect(dst_w, dst_h, src_w, src_h);
            resize_bilinear_roi_scalar(
                src, src_w, src_h, stride_src, src_full, dst, dst_w, dst_h, stride_dst, dst_box,
                4,
            )
        }
    }
}

/// Transforms `src` into `dst` with AVX2 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn crop_scale_bgra_avx2(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    aspect_w: u32,
    aspect_h: u32,
    strategy: CropStrategy,
) -> bool {
    crop_scale_bgra_scalar(
        src, src_w, src_h, stride_src, dst, dst_w, dst_h, stride_dst, aspect_w, aspect_h, strategy,
    )
}

/// Transforms `src` into `dst` with AVX-512 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
pub unsafe fn crop_scale_bgra_avx512(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    aspect_w: u32,
    aspect_h: u32,
    strategy: CropStrategy,
) -> bool {
    crop_scale_bgra_scalar(
        src, src_w, src_h, stride_src, dst, dst_w, dst_h, stride_dst, aspect_w, aspect_h, strategy,
    )
}

/// Transforms `src` into `dst` applying CenterCrop or Letterbox with runtime SIMD dispatch.
pub fn crop_scale_bgra(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride_src: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    stride_dst: u32,
    aspect_w: u32,
    aspect_h: u32,
    strategy: CropStrategy,
    requested_tier: SimdTier,
) -> bool {
    let tier = match requested_tier {
        SimdTier::Auto => crate::detect_best_simd_tier(),
        t => t,
    };

    #[cfg(target_arch = "x86_64")]
    {
        if tier == SimdTier::Avx512
            && std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
        {
            return unsafe {
                crop_scale_bgra_avx512(
                    src, src_w, src_h, stride_src, dst, dst_w, dst_h, stride_dst, aspect_w,
                    aspect_h, strategy,
                )
            };
        }
        if (tier == SimdTier::Avx2 || tier == SimdTier::Avx512)
            && std::arch::is_x86_feature_detected!("avx2")
        {
            return unsafe {
                crop_scale_bgra_avx2(
                    src, src_w, src_h, stride_src, dst, dst_w, dst_h, stride_dst, aspect_w,
                    aspect_h, strategy,
                )
            };
        }
    }

    let _ = tier;
    crop_scale_bgra_scalar(
        src, src_w, src_h, stride_src, dst, dst_w, dst_h, stride_dst, aspect_w, aspect_h, strategy,
    )
}
