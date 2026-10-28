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

//! SmartCam v2.0 Pure Rust Video Processing Engine (`smartcam-engine`).
//! Provides runtime SIMD dispatch (`AVX-512`, `AVX2`, `NEON`, and `Scalar`),
//! Crop & Scale, HSV Chroma Green Screen segmentation, AI/Portrait & Reference
//! Background segmentation, Box Blur Bokeh, and dynamic GPU/ONNX runtime probing.

pub mod ai_segmenter;
pub mod compositor;
pub mod crop_scaler;
pub mod gpu;
pub mod green_screen;

pub use ai_segmenter::{AiOnnxSegmenter, AiSegmenterConfig};
pub use compositor::{blend_bgra, fill_solid_bg, generate_blurred_bg};
pub use crop_scaler::{
    compute_center_crop_rect, compute_social_crop_rect, crop_scale_bgra, crop_zoom_image_social,
    resolve_aspect_ratio, CropRect,
};
pub use gpu::{
    BackendBenchmarkResult, ComputeBackend, GpuAccelerator, ProbedComputeDeviceInfo, SafeLibrary,
};
pub use green_screen::{
    compute_greenscreen_mask, rgb_to_hsv_fast, GreenScreenConfig, GreenScreenSegmenter,
};

/// Convenience alias for free-standing green screen auto-calibration.
pub fn autocalibrate_greenscreen_direct(
    input_frame: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    is_bgra: bool,
    sample_x: i32,
    sample_y: i32,
) -> (u8, u8, u8, u8) {
    let mut segmenter = GreenScreenSegmenter::new(GreenScreenConfig::default());
    segmenter.autocalibrate_greenscreen(input_frame, width, height, stride, is_bgra, sample_x, sample_y)
}

/// Supported pixel formats across the SmartCam pipeline.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Unknown = 0,
    Rgb24 = 1,
    Bgr24 = 2,
    Rgba32 = 3,
    Bgra32 = 4,
    Nv12 = 5,
    Yuy2 = 6,
    Gray8 = 7,
    Rgba64 = 8,
    Bgra64 = 9,
}

/// Target aspect ratio mode for output framing.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AspectRatioMode {
    Native = 0,
    Ratio16x9 = 1,
    Ratio4x3 = 2,
    Ratio1x1 = 3,
    Ratio9x16 = 4,
    Custom = 5,
}

/// Scaling / framing strategy when transforming between different aspect ratios.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropStrategy {
    CenterCrop = 0,
    Letterbox = 1,
}

/// Active or requested SIMD instruction set tier.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdTier {
    Auto = 0,
    Scalar = 1,
    Avx2 = 2,
    Avx512 = 3,
    Neon = 4,
}

impl SimdTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            SimdTier::Auto => "Auto",
            SimdTier::Scalar => "Scalar",
            SimdTier::Avx2 => "AVX2",
            SimdTier::Avx512 => "AVX-512",
            SimdTier::Neon => "NEON",
        }
    }
}

/// Returns the best SIMD tier supported by the current CPU at runtime.
pub fn detect_best_simd_tier() -> SimdTier {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
        {
            return SimdTier::Avx512;
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            return SimdTier::Avx2;
        }
        SimdTier::Scalar
    }
    #[cfg(target_arch = "aarch64")]
    {
        SimdTier::Neon
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        SimdTier::Scalar
    }
}

/// Unified High-Performance Pure Rust Video Processing Engine (`SmartCamEngine`).
pub struct SmartCamEngine {
    active_simd: SimdTier,
    gpu_accel: GpuAccelerator,
    green_screen: GreenScreenSegmenter,
    ai_segmenter: AiOnnxSegmenter,
}

impl SmartCamEngine {
    pub fn new(requested_simd: SimdTier) -> Self {
        let active_simd = match requested_simd {
            SimdTier::Auto => detect_best_simd_tier(),
            t => t,
        };
        let gpu_accel = GpuAccelerator::new(ComputeBackend::Auto, active_simd);
        let green_screen = GreenScreenSegmenter::new(GreenScreenConfig::default());
        let ai_segmenter = AiOnnxSegmenter::new(AiSegmenterConfig::default());

        Self {
            active_simd,
            gpu_accel,
            green_screen,
            ai_segmenter,
        }
    }

    pub fn set_simd_tier(&mut self, requested_tier: SimdTier) {
        let tier = match requested_tier {
            SimdTier::Auto => detect_best_simd_tier(),
            t => t,
        };
        self.active_simd = tier;
        self.gpu_accel.set_simd_tier(tier);
    }

    pub fn get_simd_tier(&self) -> SimdTier {
        self.active_simd
    }

    pub fn get_active_simd_name(&self) -> &'static str {
        self.active_simd.as_str()
    }

    pub fn set_hardware_policy(
        &mut self,
        backend: ComputeBackend,
        simd: SimdTier,
        _ai_enabled: bool,
    ) {
        self.gpu_accel.set_active_backend(backend);
        self.set_simd_tier(simd);
    }

    pub fn get_active_compute_backend(&self) -> ComputeBackend {
        self.gpu_accel.get_active_backend()
    }

    pub fn get_active_compute_backend_name(&self) -> &'static str {
        self.gpu_accel.get_active_backend_name()
    }

    pub fn get_active_compute_device_name(&self) -> String {
        self.gpu_accel.get_active_device_name()
    }

    pub fn probe_compute_devices(&mut self) -> &[ProbedComputeDeviceInfo] {
        self.gpu_accel.probe_available_backends()
    }

    pub fn configure_greenscreen(&mut self, config: GreenScreenConfig) {
        self.green_screen.update_config(config);
    }

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
        self.green_screen.autocalibrate_greenscreen(
            input_frame,
            width,
            height,
            stride,
            is_bgra,
            sample_x,
            sample_y,
        )
    }

    pub fn compute_greenscreen_mask(
        &mut self,
        input_frame: &[u8],
        width: u32,
        height: u32,
        stride_in: u32,
        out_mask: &mut [u8],
        stride_out: u32,
        is_bgra: bool,
    ) -> bool {
        self.green_screen.compute_mask(
            input_frame,
            width,
            height,
            stride_in,
            out_mask,
            stride_out,
            is_bgra,
            self.active_simd,
        )
    }

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
        self.green_screen.suppress_green_spill(
            frame,
            mask,
            width,
            height,
            stride_frame,
            stride_mask,
            is_bgra,
        );
    }

    pub fn compute_ai_mask(
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
        self.ai_segmenter.compute_mask(
            input_frame,
            width,
            height,
            stride_in,
            out_mask,
            stride_out,
            is_bgra,
            ai_enabled,
        )
    }

    pub fn calibrate_face_landmarks(
        &mut self,
        snapshot: &[u8],
        width: u32,
        height: u32,
        pts: [(f32, f32); 5],
    ) -> (f32, f32, f32, f32) {
        self.ai_segmenter.calibrate_face_landmarks(snapshot, width, height, pts)
    }

    pub fn set_face_calibration_profile(
        &mut self,
        enabled: bool,
        skin_cb: f32,
        skin_cr: f32,
        head_rx: f32,
        head_ry: f32,
    ) {
        self.ai_segmenter
            .set_face_calibration_profile(enabled, skin_cb, skin_cr, head_rx, head_ry);
    }

    pub fn set_face_calibration_landmarks(&mut self, landmarks: &[f32; 10]) {
        self.ai_segmenter.set_face_calibration_landmarks(landmarks);
    }

    pub fn clear_face_calibration(&mut self) {
        self.ai_segmenter.clear_face_calibration();
    }

    pub fn is_face_calibrated(&self) -> bool {
        self.ai_segmenter.is_face_calibrated()
    }

    pub fn set_silhouette_mode(&mut self, mode: i32) {
        self.ai_segmenter.set_silhouette_mode(mode);
    }

    pub fn get_silhouette_mode(&self) -> i32 {
        self.ai_segmenter.get_silhouette_mode()
    }

    pub fn set_multi_person_mode(&mut self, enabled: bool) {
        self.ai_segmenter.set_multi_person_mode(enabled);
    }

    pub fn is_multi_person_mode(&self) -> bool {
        self.ai_segmenter.is_multi_person_mode()
    }

    pub fn set_reference_background(&mut self, bg: &[u8], width: u32, height: u32) -> bool {
        self.ai_segmenter.set_reference_background(bg, width, height)
    }

    pub fn clear_reference_background(&mut self) {
        self.ai_segmenter.clear_reference_background();
    }

    pub fn has_reference_background(&self) -> bool {
        self.ai_segmenter.has_reference_background()
    }

    pub fn get_subject_center(&self) -> (f32, f32) {
        self.ai_segmenter.get_tracked_subject_center()
    }

    pub fn crop_scale_bgra(
        &self,
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
        crop_scaler::crop_scale_bgra(
            src,
            src_w,
            src_h,
            stride_src,
            dst,
            dst_w,
            dst_h,
            stride_dst,
            aspect_w,
            aspect_h,
            strategy,
            self.active_simd,
        )
    }

    pub fn fill_solid_bg(
        &self,
        dst: &mut [u8],
        width: u32,
        height: u32,
        stride: u32,
        r: u8,
        g: u8,
        b: u8,
        is_bgra: bool,
    ) -> bool {
        compositor::fill_solid_bg(dst, width, height, stride, r, g, b, is_bgra)
    }

    pub fn generate_blurred_bg(
        &self,
        src: &[u8],
        dst: &mut [u8],
        width: u32,
        height: u32,
        stride: u32,
        radius: u32,
    ) -> bool {
        compositor::generate_blurred_bg(src, dst, width, height, stride, radius, self.active_simd)
    }

    pub fn blend_bgra(
        &self,
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
        compositor::blend_bgra(
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
            self.active_simd,
        )
    }

    pub fn benchmark_profile(
        &mut self,
        width: u32,
        height: u32,
        ai_enabled: bool,
    ) -> BackendBenchmarkResult {
        self.gpu_accel.get_recommended_profile(width, height, ai_enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_detection() {
        let tier = detect_best_simd_tier();
        assert!(matches!(
            tier,
            SimdTier::Scalar | SimdTier::Avx2 | SimdTier::Avx512 | SimdTier::Neon
        ));
    }

    #[test]
    fn test_blend_bgra_scalar_vs_avx2_and_avx512() {
        let (w, h) = (67u32, 23u32); // Intentionally non-multiple of 8 and 16 to stress tail handling
        let total_bytes = (w * h * 4) as usize;
        let mask_bytes = (w * h) as usize;

        let fg: Vec<u8> = (0..total_bytes).map(|i| (i * 7 + 13) as u8).collect();
        let bg: Vec<u8> = (0..total_bytes).map(|i| (i * 11 + 97) as u8).collect();
        let mask: Vec<u8> = (0..mask_bytes).map(|i| (i * 17) as u8).collect();

        let mut out_scalar = vec![0u8; total_bytes];
        let ok_scalar = compositor::blend_bgra_scalar(
            &fg, &bg, &mask, &mut out_scalar, w, h, w * 4, w * 4, w, w * 4,
        );
        assert!(ok_scalar);

        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("avx2") {
                let mut out_avx2 = vec![0u8; total_bytes];
                let ok_avx2 = unsafe {
                    compositor::blend_bgra_avx2(
                        &fg, &bg, &mask, &mut out_avx2, w, h, w * 4, w * 4, w, w * 4,
                    )
                };
                assert!(ok_avx2);
                assert_eq!(
                    out_scalar, out_avx2,
                    "AVX2 blend output must match Scalar path exactly with zero artifacts"
                );
            }

            if std::arch::is_x86_feature_detected!("avx512f")
                && std::arch::is_x86_feature_detected!("avx512bw")
            {
                let mut out_avx512 = vec![0u8; total_bytes];
                let ok_avx512 = unsafe {
                    compositor::blend_bgra_avx512(
                        &fg, &bg, &mask, &mut out_avx512, w, h, w * 4, w * 4, w, w * 4,
                    )
                };
                assert!(ok_avx512);
                assert_eq!(
                    out_scalar, out_avx512,
                    "AVX-512 blend output must match Scalar path exactly"
                );
            }
        }
    }

    #[test]
    fn test_crop_scale_bgra_scalar_vs_avx2() {
        let (sw, sh) = (1920u32, 1080u32);
        let (dw, dh) = (1440u32, 1080u32);
        let src_bytes = (sw * sh * 4) as usize;
        let dst_bytes = (dw * dh * 4) as usize;

        let src: Vec<u8> = (0..src_bytes).map(|i| (i & 0xFF) as u8).collect();
        let mut dst_scalar = vec![0u8; dst_bytes];

        let ok_scalar = crop_scaler::crop_scale_bgra_scalar(
            &src,
            sw,
            sh,
            sw * 4,
            &mut dst_scalar,
            dw,
            dh,
            dw * 4,
            4,
            3,
            CropStrategy::CenterCrop,
        );
        assert!(ok_scalar);

        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("avx2") {
                let mut dst_avx2 = vec![0u8; dst_bytes];
                let ok_avx2 = unsafe {
                    crop_scaler::crop_scale_bgra_avx2(
                        &src,
                        sw,
                        sh,
                        sw * 4,
                        &mut dst_avx2,
                        dw,
                        dh,
                        dw * 4,
                        4,
                        3,
                        CropStrategy::CenterCrop,
                    )
                };
                assert!(ok_avx2);
                assert_eq!(
                    dst_scalar, dst_avx2,
                    "AVX2 crop_scale must match Scalar path exactly"
                );
            }
        }
    }

    #[test]
    fn test_generate_blurred_bg_scalar_vs_avx2() {
        let (w, h) = (160u32, 90u32);
        let total = (w * h * 4) as usize;
        let src: Vec<u8> = (0..total).map(|i| (i * 3 + 25) as u8).collect();

        let mut dst_scalar = vec![0u8; total];
        let ok_scalar = compositor::generate_blurred_bg_scalar(
            &src, &mut dst_scalar, w, h, w * 4, 8,
        );
        assert!(ok_scalar);

        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("avx2") {
                let mut dst_avx2 = vec![0u8; total];
                let ok_avx2 = unsafe {
                    compositor::generate_blurred_bg_avx2(
                        &src, &mut dst_avx2, w, h, w * 4, 8,
                    )
                };
                assert!(ok_avx2);
                assert_eq!(
                    dst_scalar, dst_avx2,
                    "AVX2 blurred bg must match Scalar path exactly"
                );
            }
        }
    }

    #[test]
    fn test_compute_greenscreen_mask_scalar_vs_avx2() {
        let (w, h) = (128u32, 64u32);
        let total = (w * h * 4) as usize;
        let mut input = vec![0u8; total];
        for i in (0..total).step_by(4) {
            input[i + 0] = 30;  // B
            input[i + 1] = 210; // G
            input[i + 2] = 30;  // R
            input[i + 3] = 255;
        }

        let cfg = GreenScreenConfig::default();
        let mut mask_scalar = vec![0u8; (w * h) as usize];
        let ok_scalar = green_screen::compute_greenscreen_mask_scalar(
            &input, w, h, w * 4, &mut mask_scalar, w, true, &cfg,
        );
        assert!(ok_scalar);

        #[cfg(target_arch = "x86_64")]
        {
            if std::arch::is_x86_feature_detected!("avx2") {
                let mut mask_avx2 = vec![0u8; (w * h) as usize];
                let ok_avx2 = unsafe {
                    green_screen::compute_greenscreen_mask_avx2(
                        &input, w, h, w * 4, &mut mask_avx2, w, true, &cfg,
                    )
                };
                assert!(ok_avx2);
                assert_eq!(
                    mask_scalar, mask_avx2,
                    "AVX2 green screen mask must match Scalar path exactly"
                );
            }
        }
    }

    #[test]
    fn test_smartcam_engine_lifecycle() {
        let engine = SmartCamEngine::new(SimdTier::Auto);
        assert!(!engine.get_active_simd_name().is_empty());

        let mut dst = vec![0u8; 160 * 90 * 4];
        assert!(engine.fill_solid_bg(&mut dst, 160, 90, 160 * 4, 12, 34, 56, true));
        assert_eq!(dst[0], 56); // B
        assert_eq!(dst[1], 34); // G
        assert_eq!(dst[2], 12); // R
        assert_eq!(dst[3], 255);
    }
}
