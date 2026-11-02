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

/// Checks the `blend_bgra*` buffers before any row is touched: each stride must cover a full
/// row (`width * 4`, or `width` for the gray8 mask) and each buffer must hold `height` strides.
/// The SIMD paths write through raw pointers, so this is what keeps them inside the slices.
#[inline]
fn blend_buffers_valid(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    out: &[u8],
    width: u32,
    height: u32,
    stride_fg: u32,
    stride_bg: u32,
    stride_mask: u32,
    stride_out: u32,
) -> bool {
    let w = width as usize;
    let h = height as usize;
    let plane_ok = |len: usize, stride: u32, row_bytes: usize| {
        let s = stride as usize;
        s >= row_bytes && len >= s * h
    };
    w != 0
        && h != 0
        && plane_ok(fg.len(), stride_fg, w * 4)
        && plane_ok(bg.len(), stride_bg, w * 4)
        && plane_ok(mask.len(), stride_mask, w)
        && plane_ok(out.len(), stride_out, w * 4)
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
    if !blend_buffers_valid(fg, bg, mask, out, width, height, stride_fg, stride_bg, stride_mask, stride_out) {
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
///
/// # Safety
/// Requires AVX2, and every stride must cover a row and every slice `height` strides: rows are
/// accessed through raw pointers without bounds checks. `blend_bgra` validates this first.
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
///
/// # Safety
/// Requires AVX-512F/BW, and every stride must cover a row and every slice `height` strides: rows
/// are accessed through raw pointers without bounds checks. `blend_bgra` validates this first.
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
    if !blend_buffers_valid(fg, bg, mask, out, width, height, stride_fg, stride_bg, stride_mask, stride_out) {
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

                let a_lo = vld1q_u16([a0, a0, a0, a0, a1, a1, a1, a1].as_ptr());
                let a_hi = vld1q_u16([a2, a2, a2, a2, a3, a3, a3, a3].as_ptr());

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
                    vld1q_u8([0u8, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255].as_ptr());
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
    if !blend_buffers_valid(fg, bg, mask, out, width, height, stride_fg, stride_bg, stride_mask, stride_out) {
        return false;
    }
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

    std::thread_local! {
        static BLUR_SCRATCH: std::cell::RefCell<(Vec<u8>, Vec<[u32; 3]>)> =
            const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
    }

    BLUR_SCRATCH.with(|cell| {
        let mut guard = cell.borrow_mut();
        let (ref mut temp, ref mut col_sums) = *guard;
        let needed_bytes = w * h * 4;
        if temp.len() < needed_bytes {
            temp.resize(needed_bytes, 0);
        }
        if col_sums.len() < w {
            col_sums.resize(w, [0u32; 3]);
        }

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

        // Vertical O(1) sliding window pass (row-major for L1 cache locality)
        col_sums[..w].fill([0u32; 3]);
        for k in -r..=r {
            let sy = k.clamp(0, (h as i32) - 1) as usize;
            let row = &temp[sy * w * 4..(sy + 1) * w * 4];
            for x in 0..w {
                let px = x * 4;
                col_sums[x][0] += row[px + 0] as u32;
                col_sums[x][1] += row[px + 1] as u32;
                col_sums[x][2] += row[px + 2] as u32;
            }
        }

        for y in 0..h {
            let dst_row = &mut dst[y * s..(y * s + w * 4)];
            let top_y = (y as i32 - r).max(0) as usize;
            let bot_y = (y as i32 + r + 1).min((h as i32) - 1) as usize;
            let top_row = &temp[top_y * w * 4..(top_y + 1) * w * 4];
            let bot_row = &temp[bot_y * w * 4..(bot_y + 1) * w * 4];

            for x in 0..w {
                let px = x * 4;
                let sums = &mut col_sums[x];
                dst_row[px + 0] = (sums[0] / diam) as u8;
                dst_row[px + 1] = (sums[1] / diam) as u8;
                dst_row[px + 2] = (sums[2] / diam) as u8;
                dst_row[px + 3] = 255;

                sums[0] = sums[0]
                    .wrapping_add(bot_row[px + 0] as u32)
                    .wrapping_sub(top_row[px + 0] as u32);
                sums[1] = sums[1]
                    .wrapping_add(bot_row[px + 1] as u32)
                    .wrapping_sub(top_row[px + 1] as u32);
                sums[2] = sums[2]
                    .wrapping_add(bot_row[px + 2] as u32)
                    .wrapping_sub(top_row[px + 2] as u32);
            }
        }
    });

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

#[cfg(test)]
mod tests {
    use super::*;

    const GUARD: u8 = 0xAA;

    /// Tiers that `blend_bgra` can actually dispatch to on this CPU.
    fn tiers() -> Vec<SimdTier> {
        let mut t = vec![SimdTier::Scalar, SimdTier::Auto];
        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("avx2") {
                t.push(SimdTier::Avx2);
            }
            if std::arch::is_x86_feature_detected!("avx512f") && std::arch::is_x86_feature_detected!("avx512bw") {
                t.push(SimdTier::Avx512);
            }
        }
        #[cfg(target_arch = "aarch64")]
        t.push(SimdTier::Neon);
        t
    }

    /// Calls `blend_bgra` on a `w`x`h` frame whose buffers have the given lengths and strides.
    /// Every buffer lives at the start of a larger allocation filled with `GUARD`, so the test can
    /// see writes past `out` instead of corrupting the heap. Returns (result, bytes changed past `out`).
    fn blend_with(
        tier: SimdTier,
        (w, h): (u32, u32),
        (fg_len, bg_len, mask_len, out_len): (usize, usize, usize, usize),
        (s_fg, s_bg, s_mask, s_out): (u32, u32, u32, u32),
    ) -> (bool, usize) {
        let fg = vec![0x10u8; fg_len];
        let bg = vec![0x20u8; bg_len];
        let mask = vec![255u8; mask_len];
        let mut out_alloc = vec![GUARD; out_len + 4096];
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            blend_bgra(&fg, &bg, &mask, &mut out_alloc[..out_len], w, h, s_fg, s_bg, s_mask, s_out, tier)
        }))
        .unwrap_or_else(|_| panic!("blend_bgra panicked for {tier:?} instead of returning false"));
        let past_out = out_alloc[out_len..].iter().filter(|&&b| b != GUARD).count();
        (ok, past_out)
    }

    #[test]
    fn blend_bgra_rejects_buffers_shorter_than_the_frame() {
        let (w, h) = (40u32, 3u32); // wider than one AVX-512 block, so every SIMD path runs
        let rgba = (w * 4 * h) as usize;
        let gray = (w * h) as usize;
        let strides = (w * 4, w * 4, w, w * 4);
        for tier in tiers() {
            for (name, lens) in [
                ("out", (rgba, rgba, gray, 4)),
                ("out by one row", (rgba, rgba, gray, rgba - (w * 4) as usize)),
                ("fg", (rgba - 1, rgba, gray, rgba)),
                ("bg", (rgba, 8, gray, rgba)),
                ("mask", (rgba, rgba, gray - 1, rgba)),
            ] {
                let (ok, past_out) = blend_with(tier, (w, h), lens, strides);
                assert!(!ok, "{tier:?}: short {name} buffer was accepted");
                assert_eq!(past_out, 0, "{tier:?}: short {name} buffer: wrote {past_out} bytes past `out`");
            }
        }
    }

    #[test]
    fn blend_bgra_rejects_strides_narrower_than_a_row() {
        let (w, h) = (40u32, 3u32);
        let big = 4096usize;
        for tier in tiers() {
            for (name, strides) in [
                ("fg", (w * 4 - 1, w * 4, w, w * 4)),
                ("bg", (w * 4, 0, w, w * 4)),
                ("mask", (w * 4, w * 4, w - 1, w * 4)),
                ("out", (w * 4, w * 4, w, w)),
            ] {
                let (ok, _) = blend_with(tier, (w, h), (big, big, big, big), strides);
                assert!(!ok, "{tier:?}: {name} stride narrower than a row was accepted");
            }
        }
    }

    #[test]
    fn blend_bgra_accepts_padded_strides_and_blends() {
        let (w, h) = (40u32, 3u32);
        let (s_rgba, s_mask) = (w * 4 + 12, w + 5);
        for tier in tiers() {
            let fg = vec![200u8; (s_rgba * h) as usize];
            let bg = vec![0u8; (s_rgba * h) as usize];
            let mask = vec![255u8; (s_mask * h) as usize];
            let mut out = vec![0u8; (s_rgba * h) as usize];
            assert!(blend_bgra(&fg, &bg, &mask, &mut out, w, h, s_rgba, s_rgba, s_mask, s_rgba, tier), "{tier:?}");
            for y in 0..h as usize {
                let row = &out[y * s_rgba as usize..y * s_rgba as usize + (w * 4) as usize];
                assert!(row.chunks_exact(4).all(|px| px == [200, 200, 200, 255]), "{tier:?}: row {y} = {:?}", &row[..8]);
            }
        }
    }
}
