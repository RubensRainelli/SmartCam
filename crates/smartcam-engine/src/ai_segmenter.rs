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

//! AI Background Removal, Dynamic Subject Tracking & 5-Point Face Calibration (`smartcam-engine`).
//! Provides portrait silhouette estimation, reference background subtraction,
//! multi-person tracking, and zero-parallax EMA temporal smoothing.

use crate::crop_scaler::{resize_bilinear_roi_scalar, CropRect};

/// Configuration parameters for AI segmentation and facial calibration.
#[derive(Clone, Debug, PartialEq)]
pub struct AiSegmenterConfig {
    pub engine_mode: i32,
    pub bg_diff_tolerance: f32,
    pub confidence_thresh: f32,
    pub temporal_smooth: f32,
    pub face_calibrated: bool,
    pub silhouette_mode: i32,
    pub multi_person_enabled: bool,
    pub calib_skin_cb: f32,
    pub calib_skin_cr: f32,
    pub calib_head_rx: f32,
    pub calib_head_ry: f32,
    pub calib_landmarks: [f32; 10],
}

impl Default for AiSegmenterConfig {
    fn default() -> Self {
        Self {
            engine_mode: 0,
            bg_diff_tolerance: 0.35,
            confidence_thresh: 0.50,
            temporal_smooth: 0.25,
            face_calibrated: false,
            silhouette_mode: 0,
            multi_person_enabled: false,
            calib_skin_cb: 113.0,
            calib_skin_cr: 148.0,
            calib_head_rx: 0.135,
            calib_head_ry: 0.220,
            calib_landmarks: [
                0.40, 0.40, // right eye
                0.60, 0.40, // left eye
                0.50, 0.50, // nose
                0.50, 0.62, // mouth
                0.50, 0.75, // chin
            ],
        }
    }
}

/// Dynamic Portrait, Reference Background & Neural Network Segmenter.
pub struct AiOnnxSegmenter {
    config: AiSegmenterConfig,
    has_reference_bg: bool,
    ref_bg_buf: Vec<u8>,
    ref_bg_w: u32,
    ref_bg_h: u32,
    tracked_cx: f32,
    tracked_cy: f32,
    prev_prob_map: Vec<f32>,
    lowres_mask: Vec<u8>,
    lowres_w: u32,
    lowres_h: u32,
}

impl AiOnnxSegmenter {
    pub fn new(config: AiSegmenterConfig) -> Self {
        Self {
            config,
            has_reference_bg: false,
            ref_bg_buf: Vec::new(),
            ref_bg_w: 0,
            ref_bg_h: 0,
            tracked_cx: 0.50,
            tracked_cy: 0.45,
            prev_prob_map: Vec::new(),
            lowres_mask: Vec::new(),
            lowres_w: 0,
            lowres_h: 0,
        }
    }

    pub fn config(&self) -> &AiSegmenterConfig {
        &self.config
    }

    pub fn update_config(&mut self, cfg: AiSegmenterConfig) {
        self.config = cfg;
    }

    pub fn set_ai_mode_and_tolerance(&mut self, engine_mode: i32, bg_diff_tolerance: f32) {
        self.config.engine_mode = engine_mode;
        self.config.bg_diff_tolerance = bg_diff_tolerance.clamp(0.05, 0.95);
    }

    pub fn set_silhouette_mode(&mut self, mode: i32) {
        self.config.silhouette_mode = mode;
    }

    pub fn get_silhouette_mode(&self) -> i32 {
        self.config.silhouette_mode
    }

    pub fn set_multi_person_mode(&mut self, enabled: bool) {
        self.config.multi_person_enabled = enabled;
    }

    pub fn is_multi_person_mode(&self) -> bool {
        self.config.multi_person_enabled
    }

    pub fn get_tracked_subject_center(&self) -> (f32, f32) {
        (self.tracked_cx, self.tracked_cy)
    }

    pub fn has_reference_background(&self) -> bool {
        self.has_reference_bg
    }

    pub fn set_reference_background(&mut self, bg_bgra: &[u8], width: u32, height: u32) -> bool {
        let expected = (width * height * 4) as usize;
        if expected == 0 || bg_bgra.len() < expected {
            return false;
        }
        self.ref_bg_buf.clear();
        self.ref_bg_buf.extend_from_slice(&bg_bgra[..expected]);
        self.ref_bg_w = width;
        self.ref_bg_h = height;
        self.has_reference_bg = true;
        true
    }

    pub fn clear_reference_background(&mut self) {
        self.ref_bg_buf.clear();
        self.ref_bg_w = 0;
        self.ref_bg_h = 0;
        self.has_reference_bg = false;
    }

    pub fn is_face_calibrated(&self) -> bool {
        self.config.face_calibrated
    }

    pub fn set_face_calibration_profile(
        &mut self,
        enabled: bool,
        skin_cb: f32,
        skin_cr: f32,
        head_rx: f32,
        head_ry: f32,
    ) {
        self.config.face_calibrated = enabled;
        if enabled {
            self.config.calib_skin_cb = skin_cb.clamp(65.0, 170.0);
            self.config.calib_skin_cr = skin_cr.clamp(118.0, 205.0);
            self.config.calib_head_rx = head_rx.clamp(0.065, 0.280);
            self.config.calib_head_ry = head_ry.clamp(0.110, 0.360);
        }
    }

    pub fn set_face_calibration_landmarks(&mut self, landmarks_10: &[f32; 10]) {
        for i in 0..10 {
            self.config.calib_landmarks[i] = landmarks_10[i].clamp(0.02, 0.98);
        }
    }

    pub fn clear_face_calibration(&mut self) {
        self.config.face_calibrated = false;
    }

    /// Calibrates personalized facial skin YCbCr locus and head dimensions from 5 normalized landmarks:
    /// `pts[0]` = Left Eye, `pts[1]` = Right Eye, `pts[2]` = Nose Tip, `pts[3]` = Lips, `pts[4]` = Chin.
    pub fn calibrate_face_landmarks(
        &mut self,
        snapshot_bgra: &[u8],
        width: u32,
        height: u32,
        pts: [(f32, f32); 5],
    ) -> (f32, f32, f32, f32) {
        let (lex, ley) = (pts[0].0.clamp(0.02, 0.98), pts[0].1.clamp(0.02, 0.98));
        let (rex, rey) = (pts[1].0.clamp(0.02, 0.98), pts[1].1.clamp(0.02, 0.98));
        let (nx, ny) = (pts[2].0.clamp(0.02, 0.98), pts[2].1.clamp(0.02, 0.98));
        let (lx, ly) = (pts[3].0.clamp(0.02, 0.98), pts[3].1.clamp(0.02, 0.98));
        let (cx, cy) = (pts[4].0.clamp(0.02, 0.98), pts[4].1.clamp(0.02, 0.98));

        let eye_dist = ((rex - lex).hypot(rey - ley)).max(0.035);
        let eye_mid_x = 0.5 * (lex + rex);
        let eye_mid_y = 0.5 * (ley + rey);
        let eye_to_chin = (cy - eye_mid_y).abs().max(0.055);

        let head_rx = (eye_dist * 1.22).clamp(0.065, 0.28);
        let head_ry = (eye_to_chin * 0.96).clamp(0.110, 0.36);

        let forehead_x = eye_mid_x;
        let forehead_y = (eye_mid_y - eye_to_chin * 0.36).clamp(0.03, 0.95);
        let left_cheek_x = (0.55 * lex + 0.45 * nx).clamp(0.03, 0.97);
        let left_cheek_y = (0.45 * ley + 0.55 * ny).clamp(0.03, 0.97);
        let right_cheek_x = (0.55 * rex + 0.45 * nx).clamp(0.03, 0.97);
        let right_cheek_y = (0.45 * rey + 0.55 * ny).clamp(0.03, 0.97);

        let sample_centers = [
            (nx, ny),
            (left_cheek_x, left_cheek_y),
            (right_cheek_x, right_cheek_y),
            (forehead_x, forehead_y),
            (0.5 * (lx + cx), 0.5 * (ly + cy)),
        ];

        let mut sum_cb = 0.0f32;
        let mut sum_cr = 0.0f32;
        let mut count = 0.0f32;

        for (sx_norm, sy_norm) in sample_centers {
            let px_c = ((sx_norm * (width as f32)).round() as i32)
                .clamp(2, (width as i32).saturating_sub(3));
            let py_c = ((sy_norm * (height as f32)).round() as i32)
                .clamp(2, (height as i32).saturating_sub(3));
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let x = (px_c + dx) as u32;
                    let y = (py_c + dy) as u32;
                    let idx = ((y * width + x) * 4) as usize;
                    if idx + 3 < snapshot_bgra.len() {
                        let b = snapshot_bgra[idx] as f32;
                        let g = snapshot_bgra[idx + 1] as f32;
                        let r = snapshot_bgra[idx + 2] as f32;
                        let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.500000 * b;
                        let cr = 128.0 + 0.500000 * r - 0.418688 * g - 0.081312 * b;
                        sum_cb += cb;
                        sum_cr += cr;
                        count += 1.0;
                    }
                }
            }
        }

        let (skin_cb, skin_cr) = if count > 0.0 {
            (
                (sum_cb / count).clamp(65.0, 170.0),
                (sum_cr / count).clamp(118.0, 205.0),
            )
        } else {
            (113.0, 148.0)
        };

        self.set_face_calibration_profile(true, skin_cb, skin_cr, head_rx, head_ry);
        let landmarks_flat = [
            rex, rey, lex, ley, nx, ny, lx, ly, cx, cy,
        ];
        self.set_face_calibration_landmarks(&landmarks_flat);

        (skin_cb, skin_cr, head_rx, head_ry)
    }

    /// Post-processes an externally or internally generated probability map:
    /// applies Zero-Parallax Motion-Adaptive EMA temporal smoothing and bilinearly upsamples to `out_mask`.
    pub fn postprocess_ai_mask(
        &mut self,
        prob_map: &[f32],
        map_width: u32,
        map_height: u32,
        out_mask: &mut [u8],
        out_width: u32,
        out_height: u32,
        out_stride: u32,
    ) -> bool {
        let num_pixels = (map_width * map_height) as usize;
        if prob_map.len() < num_pixels || out_mask.len() < (out_stride * out_height) as usize {
            return false;
        }

        if self.lowres_w != map_width || self.lowres_h != map_height {
            self.lowres_mask.resize(num_pixels, 0);
            self.lowres_w = map_width;
            self.lowres_h = map_height;
        }

        let smooth_alpha = self.config.temporal_smooth.clamp(0.0, 0.90);
        let thresh = self.config.confidence_thresh.clamp(0.05, 0.95);
        let feather_band = 0.15f32;
        let low_edge = (thresh - feather_band).max(0.0);
        let inv_band = 1.0 / (2.0 * feather_band);

        let has_history = self.prev_prob_map.len() == num_pixels && smooth_alpha > 0.0;
        if !has_history {
            self.prev_prob_map.clear();
            self.prev_prob_map.extend_from_slice(prob_map);
        }

        for y in 0..map_height {
            for x in 0..map_width {
                let idx = (y * map_width + x) as usize;
                let mut p = prob_map[idx];

                if has_history {
                    let delta = (p - self.prev_prob_map[idx]).abs();
                    let eff_alpha = if delta > 0.18 {
                        0.0
                    } else {
                        smooth_alpha * (1.0 - delta / 0.18) * 0.55
                    };
                    p = (1.0 - eff_alpha) * p + eff_alpha * self.prev_prob_map[idx];
                    self.prev_prob_map[idx] = p;
                }

                let t = ((p - low_edge) * inv_band).clamp(0.0, 1.0);
                let smooth_t = t * t * (3.0 - 2.0 * t);
                self.lowres_mask[idx] = (smooth_t * 255.0 + 0.5) as u8;
            }
        }

        let src_roi = CropRect {
            x: 0,
            y: 0,
            width: map_width,
            height: map_height,
        };
        let dst_roi = CropRect {
            x: 0,
            y: 0,
            width: out_width,
            height: out_height,
        };

        resize_bilinear_roi_scalar(
            &self.lowres_mask,
            map_width,
            map_height,
            map_width,
            src_roi,
            out_mask,
            out_width,
            out_height,
            out_stride,
            dst_roi,
            1,
        )
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
        ai_enabled: bool,
    ) -> bool {
        let w = width as usize;
        let h = height as usize;
        if w == 0 || h == 0 || input_frame.len() < (stride_in as usize) * h || out_mask.len() < (stride_out as usize) * h {
            return false;
        }

        let tol = self.config.bg_diff_tolerance.clamp(0.10, 0.95);
        let thresh = self.config.confidence_thresh;
        let face_only = self.config.silhouette_mode == 1;
        let cal_active = self.config.face_calibrated;
        let target_cb = self.config.calib_skin_cb;
        let target_cr = self.config.calib_skin_cr;

        let head_rx = if cal_active {
            self.config.calib_head_rx.clamp(0.08, 0.28)
        } else {
            0.135f32
        };
        let head_ry = if cal_active {
            self.config.calib_head_ry.clamp(0.12, 0.36)
        } else {
            0.220f32
        };
        let body_rx = (head_rx * 2.15).clamp(0.22, 0.46);
        let body_ry = (head_ry * 1.45).clamp(0.24, 0.46);

        // 1. Dynamic Subject Tracking: estimate true skin centroid in upper-middle frame
        let mut sum_wx = 0.0f32;
        let mut sum_wy = 0.0f32;
        let mut sum_w = 0.0f32;
        let step = 8usize;
        let max_face_y = (h * 64) / 100;

        for y in (step..max_face_y).step_by(step) {
            let ny = (y as f32) / (h.max(1) as f32);
            let row_off = y * stride_in as usize;

            let l_c0 = input_frame[row_off + 16];
            let l_c1 = input_frame[row_off + 17];
            let l_c2 = input_frame[row_off + 18];
            let (rl, gl, bl) = if is_bgra { (l_c2 as i32, l_c1 as i32, l_c0 as i32) } else { (l_c0 as i32, l_c1 as i32, l_c2 as i32) };

            let r_idx = row_off + (w.saturating_sub(5)) * 4;
            let r_c0 = input_frame[r_idx];
            let r_c1 = input_frame[r_idx + 1];
            let r_c2 = input_frame[r_idx + 2];
            let (rr, gr, br) = if is_bgra { (r_c2 as i32, r_c1 as i32, r_c0 as i32) } else { (r_c0 as i32, r_c1 as i32, r_c2 as i32) };

            for x in (step..w.saturating_sub(step)).step_by(step) {
                let nx = (x as f32) / (w.max(1) as f32);
                let px = row_off + x * 4;
                let c0 = input_frame[px + 0];
                let c1 = input_frame[px + 1];
                let c2 = input_frame[px + 2];
                let (r, g, b) = if is_bgra { (c2 as i32, c1 as i32, c0 as i32) } else { (c0 as i32, c1 as i32, c2 as i32) };

                let is_green = g > 135 && g > r + 40 && g > b + 40;
                if is_green {
                    continue;
                }
                let diff_l = (r - rl).abs() + (g - gl).abs() + (b - bl).abs();
                let diff_r = (r - rr).abs() + (g - gr).abs() + (b - br).abs();
                if diff_l < 38 || diff_r < 38 {
                    continue;
                }

                if cal_active {
                    let rf = r as f32;
                    let gf = g as f32;
                    let bf = b as f32;
                    let cb = 128.0 - 0.168736 * rf - 0.331264 * gf + 0.500000 * bf;
                    let cr = 128.0 + 0.500000 * rf - 0.418688 * gf - 0.081312 * bf;
                    let dist_c = (cb - target_cb).hypot(cr - target_cr);
                    if dist_c < 22.0 && r > 55 {
                        let weight = (24.0 - dist_c).max(1.0) * 0.12;
                        sum_wx += nx * weight;
                        sum_wy += ny * weight;
                        sum_w += weight;
                    }
                } else {
                    let rb = r - b;
                    let rg = r - g;
                    let true_skin = r > 85
                        && g > 45
                        && b > 25
                        && rb >= 18
                        && rg >= (rb * 24) / 100
                        && rg <= (rb * 85) / 100
                        && rb * 2 <= r;
                    if true_skin {
                        let weight = (rb as f32) * 0.025;
                        sum_wx += nx * weight;
                        sum_wy += ny * weight;
                        sum_w += weight;
                    }
                }
            }
        }

        let (target_cx, target_cy) = if sum_w > 1.2 {
            (
                (sum_wx / sum_w).clamp(0.15, 0.85),
                (sum_wy / sum_w).clamp(0.18, 0.62),
            )
        } else {
            (self.tracked_cx, self.tracked_cy)
        };

        let jump = ((target_cx - self.tracked_cx).powi(2) + (target_cy - self.tracked_cy).powi(2)).sqrt();
        let alpha_track = if jump > 0.04 { 0.85 } else { 0.35 };
        self.tracked_cx = self.tracked_cx * (1.0 - alpha_track) + target_cx * alpha_track;
        self.tracked_cy = self.tracked_cy * (1.0 - alpha_track) + target_cy * alpha_track;
        let cx = self.tracked_cx;
        let cy = self.tracked_cy;

        // Fast path when AI is disabled and no reference background is set
        if !ai_enabled && !self.has_reference_bg {
            let block = 8usize;
            for by in (0..h).step_by(block) {
                let y_end = (by + block).min(h);
                let ny = (by as f32) / (h.max(1) as f32);
                for bx in (0..w).step_by(block) {
                    let x_end = (bx + block).min(w);
                    let nx = (bx as f32) / (w.max(1) as f32);
                    let px = by * (stride_in as usize) + bx * 4;
                    let c0 = input_frame[px + 0];
                    let c1 = input_frame[px + 1];
                    let c2 = input_frame[px + 2];
                    let (r, g, b) = if is_bgra { (c2, c1, c0) } else { (c0, c1, c2) };

                    let is_bright_green = g > 140 && g > r.saturating_add(45) && g > b.saturating_add(45);
                    let alpha_val = if is_bright_green {
                        0u8
                    } else {
                        let head_dx = (nx - cx) / head_rx;
                        let head_dy = (ny - (cy - 0.03)) / head_ry;
                        let head_d2 = head_dx * head_dx + head_dy * head_dy;
                        let min_d2 = if face_only {
                            head_d2
                        } else {
                            let body_dx = (nx - cx) / body_rx;
                            let body_dy = (ny - (cy + 0.36)) / body_ry;
                            head_d2.min(body_dx * body_dx + body_dy * body_dy)
                        };
                        if min_d2 <= 1.05 {
                            255u8
                        } else if min_d2 <= 1.35 {
                            128u8
                        } else {
                            0u8
                        }
                    };

                    for y in by..y_end {
                        let row = y * (stride_out as usize);
                        for x in bx..x_end {
                            out_mask[row + x] = alpha_val;
                        }
                    }
                }
            }
            return true;
        }

        // Reference Background Subtraction or Dynamic Silhouette Mask
        for y in 0..h {
            let ny = (y as f32) / (h.max(1) as f32);
            let s_row = y * (stride_in as usize);
            let m_row = y * (stride_out as usize);

            for x in 0..w {
                let nx = (x as f32) / (w.max(1) as f32);
                let px = s_row + x * 4;
                let c0 = input_frame[px + 0];
                let c1 = input_frame[px + 1];
                let c2 = input_frame[px + 2];
                let (r, g, b) = if is_bgra { (c2, c1, c0) } else { (c0, c1, c2) };

                let is_bright_green = g > 140 && g > r.saturating_add(45) && g > b.saturating_add(45);
                if is_bright_green {
                    out_mask[m_row + x] = 0;
                    continue;
                }

                if self.has_reference_bg && !self.ref_bg_buf.is_empty() {
                    let rw = self.ref_bg_w as usize;
                    let rh = self.ref_bg_h as usize;
                    let rx = ((x * rw) / w.max(1)).min(rw.saturating_sub(1));
                    let ry = ((y * rh) / h.max(1)).min(rh.saturating_sub(1));
                    let rbase = (ry * rw + rx) * 4;

                    let (r_ref, g_ref, b_ref) = if is_bgra {
                        (self.ref_bg_buf[rbase + 2], self.ref_bg_buf[rbase + 1], self.ref_bg_buf[rbase])
                    } else {
                        (self.ref_bg_buf[rbase], self.ref_bg_buf[rbase + 1], self.ref_bg_buf[rbase + 2])
                    };

                    let y_cur = (r as i32 * 77 + g as i32 * 150 + b as i32 * 29) >> 8;
                    let y_ref_val = (r_ref as i32 * 77 + g_ref as i32 * 150 + b_ref as i32 * 29) >> 8;
                    let cb_cur = b as i32 - y_cur;
                    let cr_cur = r as i32 - y_cur;
                    let cb_ref = b_ref as i32 - y_ref_val;
                    let cr_ref = r_ref as i32 - y_ref_val;

                    let d_chroma = ((cb_cur - cb_ref).abs() + (cr_cur - cr_ref).abs()) as f32 / 255.0;
                    let d_luma = (y_cur - y_ref_val).abs() as f32 / 255.0;
                    let luma_gate = (0.04 + tol * 0.14).clamp(0.04, 0.20);
                    let eff_luma = (d_luma - luma_gate).max(0.0);

                    let diff = d_chroma * 1.45 + eff_luma * 0.85;
                    let gain = (5.2 - tol * 3.0).clamp(1.8, 5.0);
                    let mut prob = (diff * gain).clamp(0.0, 1.0);

                    if face_only {
                        let head_dx = (nx - cx) / (head_rx * 1.15);
                        let head_dy = (ny - (cy - 0.03)) / (head_ry * 1.15);
                        let head_d2 = head_dx * head_dx + head_dy * head_dy;
                        let head_gate = (1.35 - head_d2).clamp(0.0, 1.0);
                        prob *= head_gate;
                    }

                    let cutoff = (thresh * 0.30 + tol * 0.22).clamp(0.10, 0.65);
                    let t = ((prob - cutoff) / 0.22).clamp(0.0, 1.0);
                    out_mask[m_row + x] = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
                } else {
                    let head_dx = (nx - cx) / head_rx;
                    let head_dy = (ny - (cy - 0.03)) / head_ry;
                    let head_d2 = head_dx * head_dx + head_dy * head_dy;

                    let min_d2 = if face_only {
                        head_d2
                    } else {
                        let body_dx = (nx - cx) / body_rx;
                        let body_dy = (ny - (cy + 0.36)) / body_ry;
                        let body_d2 = body_dx * body_dx + body_dy * body_dy;
                        head_d2.min(body_d2)
                    };

                    let prob = (1.32 - min_d2).clamp(0.0, 1.0);
                    let t = ((prob - (thresh - 0.15)) / 0.30).clamp(0.0, 1.0);
                    out_mask[m_row + x] = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
                }
            }
        }

        true
    }
}
