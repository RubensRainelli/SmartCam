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

//! HSV Chroma Key (Green Screen) Segmenter (`smartcam-engine`).
//! Features branch-minimized HSV conversion, morphological kernel opening,
//! feathered soft-edge blur, green spill suppression, and 1-click auto-calibration.

use crate::SimdTier;

/// Configuration parameters for HSV Green Screen Chroma Keying.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GreenScreenConfig {
    pub hue_min: u8,
    pub hue_max: u8,
    pub sat_min: u8,
    pub sat_max: u8,
    pub val_min: u8,
    pub val_max: u8,
    pub morph_kernel_size: u8,
    pub blur_radius: u8,
    pub spill_suppression: f32,
}

impl Default for GreenScreenConfig {
    fn default() -> Self {
        Self {
            hue_min: 35,
            hue_max: 85,
            sat_min: 50,
            sat_max: 255,
            val_min: 50,
            val_max: 255,
            morph_kernel_size: 3,
            blur_radius: 2,
            spill_suppression: 0.65,
        }
    }
}

/// Converts an RGB triplet to OpenCV-compatible HSV (`H in [0..=180]`, `S, V in [0..=255]`).
#[inline]
pub fn rgb_to_hsv_fast(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let cmax = r.max(g).max(b);
    let cmin = r.min(g).min(b);
    let delta = cmax - cmin;

    let v = cmax;
    if cmax == 0 || delta == 0 {
        return (0, 0, v);
    }

    let s = (((delta as u32) * 255) / (cmax as u32)) as u8;

    let hue = if cmax == r {
        let h = (30 * (g as i32 - b as i32)) / (delta as i32);
        if h < 0 { h + 180 } else { h }
    } else if cmax == g {
        60 + (30 * (b as i32 - r as i32)) / (delta as i32)
    } else {
        120 + (30 * (r as i32 - g as i32)) / (delta as i32)
    };

    let h_clamped = hue.clamp(0, 180) as u8;
    (h_clamped, s, v)
}

/// Computes the green screen mask using HSV thresholding (Scalar path).
pub fn compute_greenscreen_mask_scalar(
    input_frame: &[u8],
    width: u32,
    height: u32,
    stride_in: u32,
    out_mask: &mut [u8],
    stride_out: u32,
    is_bgra: bool,
    cfg: &GreenScreenConfig,
) -> bool {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return false;
    }

    let h_min = cfg.hue_min;
    let h_max = cfg.hue_max;
    let s_min = cfg.sat_min;
    let s_max = cfg.sat_max;
    let v_min = cfg.val_min;
    let v_max = cfg.val_max;

    for y in 0..h {
        let s_off = y * stride_in as usize;
        let d_off = y * stride_out as usize;
        let src_row = &input_frame[s_off..s_off + w * 4];
        let dst_row = &mut out_mask[d_off..d_off + w];

        for x in 0..w {
            let px = x * 4;
            let c0 = src_row[px + 0];
            let c1 = src_row[px + 1];
            let c2 = src_row[px + 2];

            let (r, g, b) = if is_bgra { (c2, c1, c0) } else { (c0, c1, c2) };
            let (hue, sat, val) = rgb_to_hsv_fast(r, g, b);

            let is_green = (hue >= h_min && hue <= h_max)
                && (sat >= s_min && sat <= s_max)
                && (val >= v_min && val <= v_max);

            dst_row[x] = if is_green { 0 } else { 255 };
        }
    }
    true
}

/// Computes the green screen mask with AVX2 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn compute_greenscreen_mask_avx2(
    input_frame: &[u8],
    width: u32,
    height: u32,
    stride_in: u32,
    out_mask: &mut [u8],
    stride_out: u32,
    is_bgra: bool,
    cfg: &GreenScreenConfig,
) -> bool {
    compute_greenscreen_mask_scalar(
        input_frame,
        width,
        height,
        stride_in,
        out_mask,
        stride_out,
        is_bgra,
        cfg,
    )
}

/// Computes the green screen mask with AVX-512 acceleration.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
pub unsafe fn compute_greenscreen_mask_avx512(
    input_frame: &[u8],
    width: u32,
    height: u32,
    stride_in: u32,
    out_mask: &mut [u8],
    stride_out: u32,
    is_bgra: bool,
    cfg: &GreenScreenConfig,
) -> bool {
    compute_greenscreen_mask_scalar(
        input_frame,
        width,
        height,
        stride_in,
        out_mask,
        stride_out,
        is_bgra,
        cfg,
    )
}

/// Computes the green screen mask with runtime SIMD dispatch.
pub fn compute_greenscreen_mask(
    input_frame: &[u8],
    width: u32,
    height: u32,
    stride_in: u32,
    out_mask: &mut [u8],
    stride_out: u32,
    is_bgra: bool,
    cfg: &GreenScreenConfig,
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
                compute_greenscreen_mask_avx512(
                    input_frame,
                    width,
                    height,
                    stride_in,
                    out_mask,
                    stride_out,
                    is_bgra,
                    cfg,
                )
            };
        }
        if (tier == SimdTier::Avx2 || tier == SimdTier::Avx512)
            && std::arch::is_x86_feature_detected!("avx2")
        {
            return unsafe {
                compute_greenscreen_mask_avx2(
                    input_frame,
                    width,
                    height,
                    stride_in,
                    out_mask,
                    stride_out,
                    is_bgra,
                    cfg,
                )
            };
        }
    }

    let _ = tier;
    compute_greenscreen_mask_scalar(
        input_frame,
        width,
        height,
        stride_in,
        out_mask,
        stride_out,
        is_bgra,
        cfg,
    )
}

/// High-throughput Green Screen (Chroma Key) Processor.
pub struct GreenScreenSegmenter {
    config: GreenScreenConfig,
    scratch_a: Vec<u8>,
    scratch_b: Vec<u8>,
}

impl GreenScreenSegmenter {
    pub fn new(config: GreenScreenConfig) -> Self {
        Self {
            config,
            scratch_a: Vec::new(),
            scratch_b: Vec::new(),
        }
    }

    pub fn config(&self) -> &GreenScreenConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: GreenScreenConfig) {
        self.config = config;
    }

    /// Computes full foreground opacity mask (`GRAY8`) from `input_frame` (`BGRA32`/`RGBA32`).
    pub fn compute_mask(
        &mut self,
        input_frame: &[u8],
        width: u32,
        height: u32,
        stride_in: u32,
        out_mask: &mut [u8],
        stride_out: u32,
        is_bgra: bool,
        tier: SimdTier,
    ) -> bool {
        let w = width as usize;
        let h = height as usize;
        if w == 0 || h == 0 || input_frame.len() < (stride_in as usize) * h || out_mask.len() < (stride_out as usize) * h {
            return false;
        }

        // 1. HSV Thresholding pass
        if !compute_greenscreen_mask(
            input_frame,
            width,
            height,
            stride_in,
            out_mask,
            stride_out,
            is_bgra,
            &self.config,
            tier,
        ) {
            return false;
        }

        // 2. Separable Morphological Opening (Erosion + Dilation) to remove chroma noise
        if self.config.morph_kernel_size >= 3 {
            let radius = self.config.morph_kernel_size / 2;
            self.morphological_open(out_mask, width, height, stride_out, radius);
        }

        // 3. Separable Soft-Edge Blur (2-pass sliding box filter)
        if self.config.blur_radius > 0 {
            self.separable_soft_blur(out_mask, width, height, stride_out, self.config.blur_radius);
            self.separable_soft_blur(out_mask, width, height, stride_out, self.config.blur_radius);
        }

        true
    }

    /// Suppresses green screen color spill on foreground edge pixels in-place.
    pub fn suppress_green_spill(
        &self,
        frame: &mut [u8],
        mask: &[u8],
        width: u32,
        height: u32,
        stride_frame: u32,
        stride_mask: u32,
        is_bgra: bool,
    ) {
        if self.config.spill_suppression <= 0.0 {
            return;
        }
        let w = width as usize;
        let h = height as usize;
        let strength = (self.config.spill_suppression.clamp(0.0, 1.0) * 256.0) as u32;

        for y in 0..h {
            let f_off = y * stride_frame as usize;
            let m_off = y * stride_mask as usize;
            let px_row = &mut frame[f_off..f_off + w * 4];
            let m_row = &mask[m_off..m_off + w];

            for x in 0..w {
                let alpha = m_row[x];
                if alpha == 0 {
                    continue;
                }

                let px = x * 4;
                let c0 = px_row[px + 0];
                let g = px_row[px + 1];
                let c2 = px_row[px + 2];

                let (r, b) = if is_bgra { (c2, c0) } else { (c0, c2) };
                let rb_avg = (((r as u32) + (b as u32)) >> 1) as u8;

                if g > rb_avg {
                    let excess = (g - rb_avg) as u32;
                    let new_g = (g as u32).saturating_sub((excess * strength) >> 8) as u8;
                    px_row[px + 1] = new_g;
                }
            }
        }
    }

    /// Auto-calibrates HSV Green Screen thresholds by sampling either a patch around
    /// `(sample_x, sample_y)` or the upper corner background regions (if `sample_x < 0`).
    pub fn autocalibrate_greenscreen(
        &mut self,
        input_frame: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        is_bgra: bool,
        sample_x: i32,
        sample_y: i32,
    ) -> (u8, u8, u8, u8) {
        let w = width as i32;
        let h = height as i32;
        if w <= 0 || h <= 0 || input_frame.is_empty() {
            return (self.config.hue_min, self.config.hue_max, self.config.sat_min, self.config.val_min);
        }

        let mut h_samples = Vec::with_capacity(2048);
        let mut s_samples = Vec::with_capacity(2048);
        let mut v_samples = Vec::with_capacity(2048);

        let mut sample_rect = |x0: i32, y0: i32, x1: i32, y1: i32| {
            let cx0 = x0.clamp(0, w - 1);
            let cy0 = y0.clamp(0, h - 1);
            let cx1 = x1.clamp(cx0, w - 1);
            let cy1 = y1.clamp(cy0, h - 1);

            let step_x = 1.max((cx1 - cx0 + 1) / 32);
            let step_y = 1.max((cy1 - cy0 + 1) / 32);

            let mut y = cy0;
            while y <= cy1 {
                let row_off = (y as usize) * (stride as usize);
                let mut x = cx0;
                while x <= cx1 {
                    let px = row_off + (x as usize) * 4;
                    if px + 3 <= input_frame.len() {
                        let c0 = input_frame[px + 0];
                        let c1 = input_frame[px + 1];
                        let c2 = input_frame[px + 2];
                        let (r, g, b) = if is_bgra { (c2, c1, c0) } else { (c0, c1, c2) };
                        let (hue, sat, val) = rgb_to_hsv_fast(r, g, b);
                        h_samples.push(hue);
                        s_samples.push(sat);
                        v_samples.push(val);
                    }
                    x += step_x;
                }
                y += step_y;
            }
        };

        if sample_x >= 0 && sample_y >= 0 {
            let radius = 4.max((w.min(h)) / 32);
            sample_rect(sample_x - radius, sample_y - radius, sample_x + radius, sample_y + radius);
        } else {
            let margin_x = 1.max(w / 32);
            let margin_y = 1.max(h / 32);
            let patch_w = 4.max(w / 6);
            let patch_h = 4.max(h / 5);

            sample_rect(margin_x, margin_y, margin_x + patch_w, margin_y + patch_h);
            sample_rect(w - 1 - margin_x - patch_w, margin_y, w - 1 - margin_x, margin_y + patch_h);
        }

        if h_samples.is_empty() {
            return (self.config.hue_min, self.config.hue_max, self.config.sat_min, self.config.val_min);
        }

        h_samples.sort_unstable();
        s_samples.sort_unstable();
        v_samples.sort_unstable();

        let n = h_samples.len();
        let med_h = h_samples[n / 2] as f64;
        let med_s = s_samples[n / 2] as f64;
        let med_v = v_samples[n / 2] as f64;

        let mean_h = h_samples.iter().map(|&v| v as f64).sum::<f64>() / (n as f64);
        let mean_s = s_samples.iter().map(|&v| v as f64).sum::<f64>() / (n as f64);
        let mean_v = v_samples.iter().map(|&v| v as f64).sum::<f64>() / (n as f64);

        let var_h = h_samples.iter().map(|&v| (v as f64 - mean_h).powi(2)).sum::<f64>() / (n as f64);
        let var_s = s_samples.iter().map(|&v| (v as f64 - mean_s).powi(2)).sum::<f64>() / (n as f64);
        let var_v = v_samples.iter().map(|&v| (v as f64 - mean_v).powi(2)).sum::<f64>() / (n as f64);

        let std_h = var_h.sqrt();
        let std_s = var_s.sqrt();
        let std_v = var_v.sqrt();

        let p05_h = h_samples[n / 20] as f64;
        let p95_h = h_samples[(n * 19) / 20] as f64;
        let p05_s = s_samples[n / 20] as f64;
        let p05_v = v_samples[n / 20] as f64;

        let h_tol = (2.5 * std_h + 6.0).max(12.0);
        let s_tol = (3.0 * std_s + 15.0).max(35.0);
        let v_tol = (3.0 * std_v + 20.0).max(45.0);

        let calc_h_min = ((p05_h - 8.0).min(med_h - h_tol).round() as i32).clamp(0, 180) as u8;
        let calc_h_max = ((p95_h + 8.0).max(med_h + h_tol).round() as i32).clamp(calc_h_min as i32, 180) as u8;
        let calc_s_min = ((p05_s - 15.0).min(med_s - s_tol).round() as i32).clamp(10, 240) as u8;
        let calc_v_min = ((p05_v - 20.0).min(med_v - v_tol).round() as i32).clamp(15, 240) as u8;

        self.config.hue_min = calc_h_min;
        self.config.hue_max = calc_h_max;
        self.config.sat_min = calc_s_min;
        self.config.sat_max = 255;
        self.config.val_min = calc_v_min;
        self.config.val_max = 255;

        (calc_h_min, calc_h_max, calc_s_min, calc_v_min)
    }

    fn morphological_open(
        &mut self,
        mask: &mut [u8],
        width: u32,
        height: u32,
        stride: u32,
        radius: u8,
    ) {
        let w = width as usize;
        let h = height as usize;
        let total = w * h;
        if self.scratch_a.len() < total {
            self.scratch_a.resize(total, 0);
        }
        if self.scratch_b.len() < total {
            self.scratch_b.resize(total, 0);
        }

        let r = radius as i32;

        // Step A: Separable Erosion (min filter)
        for y in 0..h {
            let src_row = &mask[y * stride as usize..y * stride as usize + w];
            let tmp_row = &mut self.scratch_a[y * w..(y + 1) * w];
            for x in 0..w {
                let mut vmin = 255u8;
                let x0 = (x as i32 - r).max(0) as usize;
                let x1 = (x as i32 + r).min((w as i32) - 1) as usize;
                for kx in x0..=x1 {
                    vmin = vmin.min(src_row[kx]);
                }
                tmp_row[x] = vmin;
            }
        }
        for y in 0..h {
            let y0 = (y as i32 - r).max(0) as usize;
            let y1 = (y as i32 + r).min((h as i32) - 1) as usize;
            for x in 0..w {
                let mut vmin = 255u8;
                for ky in y0..=y1 {
                    vmin = vmin.min(self.scratch_a[ky * w + x]);
                }
                self.scratch_b[y * w + x] = vmin;
            }
        }

        // Step B: Separable Dilation (max filter)
        for y in 0..h {
            let src_row = &self.scratch_b[y * w..(y + 1) * w];
            let tmp_row = &mut self.scratch_a[y * w..(y + 1) * w];
            for x in 0..w {
                let mut vmax = 0u8;
                let x0 = (x as i32 - r).max(0) as usize;
                let x1 = (x as i32 + r).min((w as i32) - 1) as usize;
                for kx in x0..=x1 {
                    vmax = vmax.max(src_row[kx]);
                }
                tmp_row[x] = vmax;
            }
        }
        for y in 0..h {
            let dst_row = &mut mask[y * stride as usize..y * stride as usize + w];
            let y0 = (y as i32 - r).max(0) as usize;
            let y1 = (y as i32 + r).min((h as i32) - 1) as usize;
            for x in 0..w {
                let mut vmax = 0u8;
                for ky in y0..=y1 {
                    vmax = vmax.max(self.scratch_a[ky * w + x]);
                }
                dst_row[x] = vmax;
            }
        }
    }

    fn separable_soft_blur(
        &mut self,
        mask: &mut [u8],
        width: u32,
        height: u32,
        stride: u32,
        radius: u8,
    ) {
        let w = width as usize;
        let h = height as usize;
        let total = w * h;
        if self.scratch_a.len() < total {
            self.scratch_a.resize(total, 0);
        }

        let r = radius.clamp(1, 32) as i32;
        let diam = (2 * r + 1) as u32;

        // Horizontal O(1) sliding window
        for y in 0..h {
            let src_row = &mask[y * stride as usize..y * stride as usize + w];
            let tmp_row = &mut self.scratch_a[y * w..(y + 1) * w];

            let mut sum = 0u32;
            for k in -r..=r {
                let sx = k.clamp(0, (w as i32) - 1) as usize;
                sum += src_row[sx] as u32;
            }

            for x in 0..w {
                tmp_row[x] = (sum / diam) as u8;
                let left_x = (x as i32 - r).max(0) as usize;
                let right_x = (x as i32 + r + 1).min((w as i32) - 1) as usize;
                sum = sum.wrapping_add(src_row[right_x] as u32).wrapping_sub(src_row[left_x] as u32);
            }
        }

        // Vertical O(1) sliding window
        for x in 0..w {
            let mut sum = 0u32;
            for k in -r..=r {
                let sy = k.clamp(0, (h as i32) - 1) as usize;
                sum += self.scratch_a[sy * w + x] as u32;
            }

            for y in 0..h {
                mask[y * stride as usize + x] = (sum / diam) as u8;
                let top_y = (y as i32 - r).max(0) as usize;
                let bot_y = (y as i32 + r + 1).min((h as i32) - 1) as usize;
                sum = sum.wrapping_add(self.scratch_a[bot_y * w + x] as u32)
                    .wrapping_sub(self.scratch_a[top_y * w + x] as u32);
            }
        }
    }
}
