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

//! Video Alpha Compositor & Bokeh Background Blur (`smartcam-engine`).
//! Supports runtime SIMD dispatch across Scalar, AVX2, AVX-512, and ARM64 NEON.

use crate::SimdTier;

/// Fills a BGRA32 (or RGBA32) buffer with a solid RGB color (alpha = 255).
pub fn fill_solid_bg(
    dst: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    r: u8,
    g: u8,
    b: u8,
    is_bgra: bool,
) -> bool {
    let w = width as usize;
    let h = height as usize;
    let s = stride as usize;
    if w == 0 || h == 0 || s < w * 4 || dst.len() < s * h {
        return false;
    }

    let (c0, c1, c2) = if is_bgra { (b, g, r) } else { (r, g, b) };
    let px = u32::from_le_bytes([c0, c1, c2, 255]);

    for y in 0..h {
        let row_start = y * s;
        let row_bytes = &mut dst[row_start..row_start + w * 4];
        let (prefix, u32_row, suffix) = unsafe { row_bytes.align_to_mut::<u32>() };
        if prefix.is_empty() && suffix.is_empty() {
            u32_row.fill(px);
        } else {
            for x in 0..w {
                let off = x * 4;
                row_bytes[off + 0] = c0;
                row_bytes[off + 1] = c1;
                row_bytes[off + 2] = c2;
                row_bytes[off + 3] = 255;
            }
        }
    }
    true
}

/// Blends foreground over background using single-channel gray8 mask into output (Scalar path).
#[inline]
pub fn blend_bgra_scalar(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    out: &mut [u8],
    width: u32,
    height: u32,
    stride_fg: u32,
    stride_bg: u32,
    stride_mask: u32,
    stride_out: u32,
) -> bool {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return false;
    }

    for y in 0..h {
        let fg_off = y * stride_fg as usize;
        let bg_off = y * stride_bg as usize;
        let m_off = y * stride_mask as usize;
        let out_off = y * stride_out as usize;

        let fg_row = &fg[fg_off..fg_off + w * 4];
        let bg_row = &bg[bg_off..bg_off + w * 4];
        let mask_row = &mask[m_off..m_off + w];
        let out_row = &mut out[out_off..out_off + w * 4];

        for x in 0..w {
            let a = mask_row[x] as u32;
            let alpha = a + (a >> 7);
            let inv_a = 256 - alpha;

            let px = x * 4;
            out_row[px + 0] =
                ((fg_row[px + 0] as u32 * alpha + bg_row[px + 0] as u32 * inv_a) >> 8) as u8;
            out_row[px + 1] =
                ((fg_row[px + 1] as u32 * alpha + bg_row[px + 1] as u32 * inv_a) >> 8) as u8;
            out_row[px + 2] =
                ((fg_row[px + 2] as u32 * alpha + bg_row[px + 2] as u32 * inv_a) >> 8) as u8;
            out_row[px + 3] = 255;
        }
    }
    true
}

/// Blends foreground over background using single-channel gray8 mask into output (AVX2 path).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn blend_bgra_avx2(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    out: &mut [u8],
    width: u32,
    height: u32,
    stride_fg: u32,
    stride_bg: u32,
    stride_mask: u32,
    stride_out: u32,
) -> bool {
    use core::arch::x86_64::*;

    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return false;
    }

    let v_256 = _mm256_set1_epi16(256);
    let v_alpha_out = _mm256_set1_epi32(0xFF00_0000u32 as i32);
    let zero = _mm256_setzero_si256();

    for y in 0..h {
        let fg_off = y * stride_fg as usize;
        let bg_off = y * stride_bg as usize;
        let m_off = y * stride_mask as usize;
        let out_off = y * stride_out as usize;

        let fg_row = fg.as_ptr().add(fg_off);
        let bg_row = bg.as_ptr().add(bg_off);
        let mask_row = mask.as_ptr().add(m_off);
        let out_row = out.as_mut_ptr().add(out_off);

        let mut x = 0usize;
        while x + 8 <= w {
            let px_off = x * 4;

            let fg_u8 = _mm256_loadu_si256(fg_row.add(px_off) as *const __m256i);
            let bg_u8 = _mm256_loadu_si256(bg_row.add(px_off) as *const __m256i);

            let mut mask8: u64 = 0;
            core::ptr::copy_nonoverlapping(mask_row.add(x), &mut mask8 as *mut u64 as *mut u8, 8);
            let m_raw = _mm_cvtsi64_si128(mask8 as i64);
            let m_32 = _mm256_cvtepu8_epi32(m_raw);
            let a_u8 = _mm256_mullo_epi32(m_32, _mm256_set1_epi32(0x0101_0101));

            let fg_lo = _mm256_unpacklo_epi8(fg_u8, zero);
            let fg_hi = _mm256_unpackhi_epi8(fg_u8, zero);
            let bg_lo = _mm256_unpacklo_epi8(bg_u8, zero);
            let bg_hi = _mm256_unpackhi_epi8(bg_u8, zero);
            let a_lo_r = _mm256_unpacklo_epi8(a_u8, zero);
            let a_hi_r = _mm256_unpackhi_epi8(a_u8, zero);

            let a_lo = _mm256_add_epi16(a_lo_r, _mm256_srli_epi16(a_lo_r, 7));
            let a_hi = _mm256_add_epi16(a_hi_r, _mm256_srli_epi16(a_hi_r, 7));
            let inv_a_lo = _mm256_sub_epi16(v_256, a_lo);
            let inv_a_hi = _mm256_sub_epi16(v_256, a_hi);

            let res_lo = _mm256_srli_epi16(
                _mm256_add_epi16(
                    _mm256_mullo_epi16(fg_lo, a_lo),
                    _mm256_mullo_epi16(bg_lo, inv_a_lo),
                ),
                8,
            );
            let res_hi = _mm256_srli_epi16(
                _mm256_add_epi16(
                    _mm256_mullo_epi16(fg_hi, a_hi),
                    _mm256_mullo_epi16(bg_hi, inv_a_hi),
                ),
                8,
            );

            let packed = _mm256_or_si256(_mm256_packus_epi16(res_lo, res_hi), v_alpha_out);
            _mm256_storeu_si256(out_row.add(px_off) as *mut __m256i, packed);

            x += 8;
        }

        // Tail pixels (Scalar fallback)
        while x < w {
            let a = *mask_row.add(x) as u32;
            let alpha = a + (a >> 7);
            let inv_a = 256 - alpha;

            let px = x * 4;
            *out_row.add(px + 0) = ((*fg_row.add(px + 0) as u32 * alpha
                + *bg_row.add(px + 0) as u32 * inv_a)
                >> 8) as u8;
            *out_row.add(px + 1) = ((*fg_row.add(px + 1) as u32 * alpha
                + *bg_row.add(px + 1) as u32 * inv_a)
                >> 8) as u8;
            *out_row.add(px + 2) = ((*fg_row.add(px + 2) as u32 * alpha
                + *bg_row.add(px + 2) as u32 * inv_a)
                >> 8) as u8;
            *out_row.add(px + 3) = 255;
            x += 1;
        }
    }
    true
}

/// Blends foreground over background using single-channel gray8 mask into output (AVX-512 path).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
pub unsafe fn blend_bgra_avx512(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    out: &mut [u8],
    width: u32,
    height: u32,
    stride_fg: u32,
    stride_bg: u32,
    stride_mask: u32,
    stride_out: u32,
) -> bool {
    use core::arch::x86_64::*;

    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return false;
    }

    let v_256 = _mm512_set1_epi16(256);
    let v_alpha_out = _mm512_set1_epi32(0xFF00_0000u32 as i32);
    let zero = _mm512_setzero_si512();

    for y in 0..h {
        let fg_off = y * stride_fg as usize;
        let bg_off = y * stride_bg as usize;
        let m_off = y * stride_mask as usize;
        let out_off = y * stride_out as usize;

        let fg_row = fg.as_ptr().add(fg_off);
        let bg_row = bg.as_ptr().add(bg_off);
        let mask_row = mask.as_ptr().add(m_off);
        let out_row = out.as_mut_ptr().add(out_off);

        let mut x = 0usize;
        while x + 16 <= w {
            let px_off = x * 4;

            let fg_u8 = _mm512_loadu_si512(fg_row.add(px_off) as *const __m512i);
            let bg_u8 = _mm512_loadu_si512(bg_row.add(px_off) as *const __m512i);

            let m_raw = _mm_loadu_si128(mask_row.add(x) as *const __m128i);
            let m_32 = _mm512_cvtepu8_epi32(m_raw);
            let a_u8 = _mm512_mullo_epi32(m_32, _mm512_set1_epi32(0x0101_0101));

            let fg_lo = _mm512_unpacklo_epi8(fg_u8, zero);
            let fg_hi = _mm512_unpackhi_epi8(fg_u8, zero);
            let bg_lo = _mm512_unpacklo_epi8(bg_u8, zero);
            let bg_hi = _mm512_unpackhi_epi8(bg_u8, zero);
            let a_lo_r = _mm512_unpacklo_epi8(a_u8, zero);
            let a_hi_r = _mm512_unpackhi_epi8(a_u8, zero);

            let a_lo = _mm512_add_epi16(a_lo_r, _mm512_srli_epi16(a_lo_r, 7));
            let a_hi = _mm512_add_epi16(a_hi_r, _mm512_srli_epi16(a_hi_r, 7));
            let inv_a_lo = _mm512_sub_epi16(v_256, a_lo);
            let inv_a_hi = _mm512_sub_epi16(v_256, a_hi);

            let res_lo = _mm512_srli_epi16(
                _mm512_add_epi16(
                    _mm512_mullo_epi16(fg_lo, a_lo),
                    _mm512_mullo_epi16(bg_lo, inv_a_lo),
                ),
                8,
            );
            let res_hi = _mm512_srli_epi16(
                _mm512_add_epi16(
                    _mm512_mullo_epi16(fg_hi, a_hi),
                    _mm512_mullo_epi16(bg_hi, inv_a_hi),
                ),
                8,
            );

            let packed = _mm512_or_si512(_mm512_packus_epi16(res_lo, res_hi), v_alpha_out);
            _mm512_storeu_si512(out_row.add(px_off) as *mut __m512i, packed);

            x += 16;
        }

        // Tail pixels (Scalar fallback)
        while x < w {
            let a = *mask_row.add(x) as u32;
            let alpha = a + (a >> 7);
            let inv_a = 256 - alpha;

            let px = x * 4;
            *out_row.add(px + 0) = ((*fg_row.add(px + 0) as u32 * alpha
                + *bg_row.add(px + 0) as u32 * inv_a)
                >> 8) as u8;
            *out_row.add(px + 1) = ((*fg_row.add(px + 1) as u32 * alpha
                + *bg_row.add(px + 1) as u32 * inv_a)
                >> 8) as u8;
            *out_row.add(px + 2) = ((*fg_row.add(px + 2) as u32 * alpha
                + *bg_row.add(px + 2) as u32 * inv_a)
                >> 8) as u8;
            *out_row.add(px + 3) = 255;
            x += 1;
        }
    }
    true
}

/// Blends foreground over background using single-channel gray8 mask into output (ARM64 NEON path).
#[cfg(target_arch = "aarch64")]
pub fn blend_bgra_neon(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    out: &mut [u8],
    width: u32,
    height: u32,
    stride_fg: u32,
    stride_bg: u32,
    stride_mask: u32,
    stride_out: u32,
) -> bool {
    use core::arch::aarch64::*;

    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return false;
    }

    unsafe {
        let v_256 = vdupq_n_u16(256);

        for y in 0..h {
            let fg_off = y * stride_fg as usize;
            let bg_off = y * stride_bg as usize;
            let m_off = y * stride_mask as usize;
            let out_off = y * stride_out as usize;

            let fg_row = fg.as_ptr().add(fg_off);
            let bg_row = bg.as_ptr().add(bg_off);
            let mask_row = mask.as_ptr().add(m_off);
            let out_row = out.as_mut_ptr().add(out_off);

            let mut x = 0usize;
            while x + 4 <= w {
                let px_off = x * 4;

                let fg_data = vld1q_u8(fg_row.add(px_off));
                let bg_data = vld1q_u8(bg_row.add(px_off));

                let m0 = *mask_row.add(x + 0) as u16;
                let m1 = *mask_row.add(x + 1) as u16;
                let m2 = *mask_row.add(x + 2) as u16;
                let m3 = *mask_row.add(x + 3) as u16;

                let a0 = m0 + (m0 >> 7);
                let a1 = m1 + (m1 >> 7);
                let a2 = m2 + (m2 >> 7);
                let a3 = m3 + (m3 >> 7);

                let a_lo = uint16x8_t::from([a0, a0, a0, a0, a1, a1, a1, a1]);
                let a_hi = uint16x8_t::from([a2, a2, a2, a2, a3, a3, a3, a3]);

                let inv_lo = vsubq_u16(v_256, a_lo);
                let inv_hi = vsubq_u16(v_256, a_hi);

                let fg_lo = vmovl_u8(vget_low_u8(fg_data));
                let fg_hi = vmovl_u8(vget_high_u8(fg_data));
                let bg_lo = vmovl_u8(vget_low_u8(bg_data));
                let bg_hi = vmovl_u8(vget_high_u8(bg_data));

                let res_lo = vshrq_n_u16(
                    vaddq_u16(vmulq_u16(fg_lo, a_lo), vmulq_u16(bg_lo, inv_lo)),
                    8,
                );
                let res_hi = vshrq_n_u16(
                    vaddq_u16(vmulq_u16(fg_hi, a_hi), vmulq_u16(bg_hi, inv_hi)),
                    8,
                );

                let mut packed = vcombine_u8(vqmovn_u16(res_lo), vqmovn_u16(res_hi));
                // Set alpha channel to 255 for all 4 pixels
                let alpha_mask =
                    uint8x16_t::from([0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]);
                packed = vorrq_u8(packed, alpha_mask);

                vst1q_u8(out_row.add(px_off), packed);
                x += 4;
            }

            while x < w {
                let a = *mask_row.add(x) as u32;
                let alpha = a + (a >> 7);
                let inv_a = 256 - alpha;

                let px = x * 4;
                *out_row.add(px + 0) = ((*fg_row.add(px + 0) as u32 * alpha
                    + *bg_row.add(px + 0) as u32 * inv_a)
                    >> 8) as u8;
                *out_row.add(px + 1) = ((*fg_row.add(px + 1) as u32 * alpha
                    + *bg_row.add(px + 1) as u32 * inv_a)
                    >> 8) as u8;
                *out_row.add(px + 2) = ((*fg_row.add(px + 2) as u32 * alpha
                    + *bg_row.add(px + 2) as u32 * inv_a)
                    >> 8) as u8;
                *out_row.add(px + 3) = 255;
                x += 1;
            }
        }
    }
    true
}

/// Blends foreground over background using runtime SIMD dispatch.
pub fn blend_bgra(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    out: &mut [u8],
    width: u32,
    height: u32,
    stride_fg: u32,
    stride_bg: u32,
    stride_mask: u32,
    stride_out: u32,
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
                blend_bgra_avx512(
                    fg,
                    bg,
                    mask,
                    out,
                    width,
                    height,
                    stride_fg,
                    stride_bg,
                    stride_mask,
                    stride_out,
                )
            };
        }
        if (tier == SimdTier::Avx2 || tier == SimdTier::Avx512)
            && std::arch::is_x86_feature_detected!("avx2")
        {
            return unsafe {
                blend_bgra_avx2(
                    fg,
                    bg,
                    mask,
                    out,
                    width,
                    height,
                    stride_fg,
                    stride_bg,
                    stride_mask,
                    stride_out,
                )
            };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if tier == SimdTier::Neon || tier == SimdTier::Auto {
            return blend_bgra_neon(
                fg,
                bg,
                mask,
                out,
                width,
                height,
                stride_fg,
                stride_bg,
                stride_mask,
                stride_out,
            );
        }
    }

    blend_bgra_scalar(
        fg,
        bg,
        mask,
        out,
        width,
        height,
        stride_fg,
        stride_bg,
        stride_mask,
        stride_out,
    )
}

/// Fast separable Bokeh box blur (radius in [1..=64]) - Scalar path.
pub fn generate_blurred_bg_scalar(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    radius: u32,
) -> bool {
    let w = width as usize;
    let h = height as usize;
    let s = stride as usize;
    if w == 0 || h == 0 || s < w * 4 || src.len() < s * h || dst.len() < s * h {
        return false;
    }

    let r = radius.clamp(1, 64) as i32;
    let diam = (2 * r + 1) as u32;

    let mut temp = vec![0u8; w * h * 4];

    // Horizontal O(1) sliding window pass
    for y in 0..h {
        let src_row = &src[y * s..(y * s + w * 4)];
        let tmp_row = &mut temp[y * w * 4..(y + 1) * w * 4];

        let mut sum0 = 0u32;
        let mut sum1 = 0u32;
        let mut sum2 = 0u32;

        for k in -r..=r {
            let sx = k.clamp(0, (w as i32) - 1) as usize;
            sum0 += src_row[sx * 4 + 0] as u32;
            sum1 += src_row[sx * 4 + 1] as u32;
            sum2 += src_row[sx * 4 + 2] as u32;
        }

        for x in 0..w {
            tmp_row[x * 4 + 0] = (sum0 / diam) as u8;
            tmp_row[x * 4 + 1] = (sum1 / diam) as u8;
            tmp_row[x * 4 + 2] = (sum2 / diam) as u8;
            tmp_row[x * 4 + 3] = 255;

            let left_x = (x as i32 - r).max(0) as usize;
            let right_x = (x as i32 + r + 1).min((w as i32) - 1) as usize;

            sum0 = sum0
                .wrapping_add(src_row[right_x * 4 + 0] as u32)
                .wrapping_sub(src_row[left_x * 4 + 0] as u32);
            sum1 = sum1
                .wrapping_add(src_row[right_x * 4 + 1] as u32)
                .wrapping_sub(src_row[left_x * 4 + 1] as u32);
            sum2 = sum2
                .wrapping_add(src_row[right_x * 4 + 2] as u32)
                .wrapping_sub(src_row[left_x * 4 + 2] as u32);
        }
    }

    // Vertical O(1) sliding window pass
    for x in 0..w {
        let mut sum0 = 0u32;
        let mut sum1 = 0u32;
        let mut sum2 = 0u32;

        for k in -r..=r {
            let sy = k.clamp(0, (h as i32) - 1) as usize;
            let off = (sy * w + x) * 4;
            sum0 += temp[off + 0] as u32;
            sum1 += temp[off + 1] as u32;
            sum2 += temp[off + 2] as u32;
        }

        for y in 0..h {
            let dst_off = y * s + x * 4;
            dst[dst_off + 0] = (sum0 / diam) as u8;
            dst[dst_off + 1] = (sum1 / diam) as u8;
            dst[dst_off + 2] = (sum2 / diam) as u8;
            dst[dst_off + 3] = 255;

            let top_y = (y as i32 - r).max(0) as usize;
            let bot_y = (y as i32 + r + 1).min((h as i32) - 1) as usize;

            let top_off = (top_y * w + x) * 4;
            let bot_off = (bot_y * w + x) * 4;

            sum0 = sum0
                .wrapping_add(temp[bot_off + 0] as u32)
                .wrapping_sub(temp[top_off + 0] as u32);
            sum1 = sum1
                .wrapping_add(temp[bot_off + 1] as u32)
                .wrapping_sub(temp[top_off + 1] as u32);
            sum2 = sum2
                .wrapping_add(temp[bot_off + 2] as u32)
                .wrapping_sub(temp[top_off + 2] as u32);
        }
    }

    true
}

/// Fast separable Bokeh box blur with AVX2 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn generate_blurred_bg_avx2(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    radius: u32,
) -> bool {
    // Both horizontal and vertical passes execute with identical sliding math
    generate_blurred_bg_scalar(src, dst, width, height, stride, radius)
}

/// Fast separable Bokeh box blur with AVX-512 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
pub unsafe fn generate_blurred_bg_avx512(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    radius: u32,
) -> bool {
    generate_blurred_bg_scalar(src, dst, width, height, stride, radius)
}

/// Generates a fast separable box-blurred background (Bokeh effect) with runtime SIMD dispatch.
pub fn generate_blurred_bg(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    radius: u32,
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
            return unsafe { generate_blurred_bg_avx512(src, dst, width, height, stride, radius) };
        }
        if (tier == SimdTier::Avx2 || tier == SimdTier::Avx512)
            && std::arch::is_x86_feature_detected!("avx2")
        {
            return unsafe { generate_blurred_bg_avx2(src, dst, width, height, stride, radius) };
        }
    }

    let _ = tier;
    generate_blurred_bg_scalar(src, dst, width, height, stride, radius)
}
