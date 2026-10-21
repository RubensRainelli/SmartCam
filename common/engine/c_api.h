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

#ifndef SMARTCAM_COMMON_ENGINE_C_API_H_
#define SMARTCAM_COMMON_ENGINE_C_API_H_

#include <stdint.h>

#if defined(_WIN32) || defined(__CYGWIN__)
    #if defined(SMARTCAM_ENGINE_STATIC)
        #define SMARTCAM_API
    #elif defined(SMARTCAM_ENGINE_EXPORTS)
        #define SMARTCAM_API __declspec(dllexport)
    #else
        #define SMARTCAM_API __declspec(dllimport)
    #endif
#elif defined(__GNUC__) && __GNUC__ >= 4
    #define SMARTCAM_API __attribute__((visibility("default")))
#else
    #define SMARTCAM_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SmartCamEngineHandle_t* SmartCamEngineHandle;

/**
 * @brief Creates a platform-agnostic SmartCam core processing engine instance.
 * @param simd_tier 0 = AUTO, 1 = SCALAR, 2 = AVX2, 3 = AVX512
 */
SMARTCAM_API SmartCamEngineHandle smartcam_engine_create(uint32_t simd_tier);

/**
 * @brief Destroys a SmartCam core processing engine instance.
 */
SMARTCAM_API void smartcam_engine_destroy(SmartCamEngineHandle handle);

/**
 * @brief Returns the active SIMD instruction set string ("AVX-512", "AVX2", or "Scalar").
 */
SMARTCAM_API const char* smartcam_engine_get_simd_backend(SmartCamEngineHandle handle);

/**
 * @brief Configures Mode 1 (HSV Green Screen) parameters on the engine instance.
 */
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
    float spill_suppression);

/**
 * @brief Auto-calibrates Mode 1 (HSV Green Screen) thresholds in 1 click or via eyedropper.
 * @param sample_x -1 = automatically sample upper background corners; >= 0 = sample around (sample_x, sample_y)
 * @param sample_y -1 = automatically sample upper background corners; >= 0 = sample around (sample_x, sample_y)
 */
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
    uint8_t* out_val_min);

/**
 * @brief Performs distortion-free Center Crop or Letterbox from `src_bgra` into `dst_bgra`.
 */
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
    uint32_t strategy);

/**
 * @brief Computes an 8-bit alpha mask (`out_mask_gray8`) using Mode 1 (HSV Green Screen).
 */
SMARTCAM_API int smartcam_engine_compute_greenscreen_mask(
    SmartCamEngineHandle handle,
    uint8_t* inout_fg_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t fg_stride,
    uint8_t* out_mask_gray8,
    uint32_t mask_stride);

/**
 * @brief Computes an 8-bit alpha mask (`out_mask_gray8`) directly using Mode 2 (`AiOnnxSegmenter::ComputeMask`).
 */
SMARTCAM_API int smartcam_engine_compute_ai_mask(
    SmartCamEngineHandle handle,
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t src_stride,
    uint8_t* out_mask_gray8,
    uint32_t mask_stride);

/**
 * @brief Post-processes an AI probability map (`float32 [0..1]`) with EMA temporal smoothing
 *        and bilinear upsampling into a full-resolution `GRAY8` alpha mask.
 */
SMARTCAM_API int smartcam_engine_postprocess_ai_mask(
    SmartCamEngineHandle handle,
    const float* prob_map,
    uint32_t map_width,
    uint32_t map_height,
    uint8_t* out_mask_gray8,
    uint32_t out_width,
    uint32_t out_height,
    uint32_t out_stride);

/**
 * @brief Fills `dst_bgra` (`BGRA32`) with a solid RGB color `(r, g, b)`.
 */
SMARTCAM_API int smartcam_engine_fill_solid_bg(
    uint8_t* dst_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t dst_stride,
    uint8_t r,
    uint8_t g,
    uint8_t b);

/**
 * @brief Generates a fast separable box-blurred Bokeh background from `src_bgra` into `dst_bgra`.
 */
SMARTCAM_API int smartcam_engine_generate_blurred_bg(
    const uint8_t* src_bgra,
    uint8_t* dst_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride,
    uint32_t blur_radius);

/**
 * @brief Alpha-blends `fg_bgra` and `bg_bgra` using `mask_gray8` into `out_bgra` via AVX-512 / AVX2 / Scalar.
 */
SMARTCAM_API int smartcam_engine_blend_bgra(
    SmartCamEngineHandle handle,
    const uint8_t* fg_bgra,
    const uint8_t* bg_bgra,
    const uint8_t* mask_gray8,
    uint8_t* out_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t rgba_stride,
    uint32_t mask_stride);

/**
 * @brief Captures and stores an empty reference background frame (`BGRA32`) for Reference Background Subtraction.
 */
SMARTCAM_API int smartcam_engine_set_reference_background(
    SmartCamEngineHandle handle,
    const uint8_t* bg_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t stride);

/**
 * @brief Clears the stored reference background frame and reverts to real-time dynamic subject tracking.
 */
SMARTCAM_API void smartcam_engine_clear_reference_background(SmartCamEngineHandle handle);

/**
 * @brief Returns 1 if a reference background frame is currently stored, 0 otherwise.
 */
SMARTCAM_API int smartcam_engine_has_reference_background(SmartCamEngineHandle handle);

/**
 * @brief Retrieves the normalized center `(out_cx, out_cy)` in `[0.0 .. 1.0]` of the tracked subject.
 */
SMARTCAM_API int smartcam_engine_get_subject_center(
    SmartCamEngineHandle handle,
    float* out_cx,
    float* out_cy);

/**
 * @brief Configures Mode 2 AI / Reference Background engine mode and spatial/chromatic tolerance.
 * @param engine_mode 0 = Auto/Hybrid, 1 = Face/Silhouette Tracker Only, 2 = Reference Background Difference Priority
 * @param bg_diff_tolerance Spatial and chromatic tolerance in [0.0 .. 1.0] (default 0.50f)
 */
SMARTCAM_API int smartcam_engine_set_ai_mode_and_tolerance(
    SmartCamEngineHandle handle,
    int engine_mode,
    float bg_diff_tolerance);

// compute_backend: 0=Auto, 1=CPU, 2=Vulkan, 3=OpenCL, 4=CUDA, 5=ROCm
// simd_mode: 0=Auto, 1=AVX512, 2=AVX2, 3=Scalar (No AVX)
// ai_enabled: 0=Disabled (Fast Non-AI / Classic Chroma / BgDiff), 1=Enabled
SMARTCAM_API int smartcam_engine_set_hardware_policy(
    SmartCamEngineHandle handle,
    uint32_t compute_backend,
    uint32_t simd_mode,
    int ai_enabled);

SMARTCAM_API int smartcam_engine_get_compute_device_count(SmartCamEngineHandle handle);

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
    int* out_is_igpu);

/**
 * @brief Returns the active compute backend name ("CPU", "Vulkan", "OpenCL", "CUDA", or "ROCm").
 */
SMARTCAM_API const char* smartcam_engine_get_active_compute_backend(SmartCamEngineHandle handle);

/**
 * @brief Returns the active hardware device model name for the selected compute backend.
 */
SMARTCAM_API const char* smartcam_engine_get_active_compute_device_name(SmartCamEngineHandle handle);

/**
 * @brief Returns 1 if neural/portrait AI segmentation is enabled on the engine handle, 0 if in Fast Non-AI mode.
 */
SMARTCAM_API int smartcam_engine_is_ai_enabled(SmartCamEngineHandle handle);

/**
 * @brief Calibrates personalized facial skin YCbCr locus and anatomical head dimensions
 *        from 5 normalized landmarks (`left_eye`, `right_eye`, `nose_tip`, `lips`, `chin`) in `[0.0 .. 1.0]`.
 */
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
    float* out_head_ry);

/**
 * @brief Directly sets or clears the personalized facial calibration profile.
 */
SMARTCAM_API int smartcam_engine_set_face_calibration_profile(
    SmartCamEngineHandle handle,
    int enabled,
    float skin_cb,
    float skin_cr,
    float head_rx,
    float head_ry);

/**
 * @brief Sets the 5 normalized facial calibration landmarks (`10` floats: `[rx,ry, lx,ly, nx,ny, mx,my, cx,cy]`)
 *        and updates the 5-point template constellation anchor.
 */
SMARTCAM_API int smartcam_engine_set_face_calibration_landmarks(
    SmartCamEngineHandle handle,
    const float* landmarks_10);

/**
 * @brief Sets the silhouette cutout mode (`0 = Volto + Busto`, `1 = Solo Volto`).
 */
SMARTCAM_API int smartcam_engine_set_silhouette_mode(
    SmartCamEngineHandle handle,
    int silhouette_mode);

/**
 * @brief Returns the active silhouette cutout mode (`0 = Volto + Busto`, `1 = Solo Volto`).
 */
SMARTCAM_API int smartcam_engine_get_silhouette_mode(
    SmartCamEngineHandle handle);

/**
 * @brief Enables (`1`) or disables (`0`) multi-person tracking (up to 4 people).
 */
SMARTCAM_API int smartcam_engine_set_multi_person_mode(
    SmartCamEngineHandle handle,
    int multi_person_enabled);

/**
 * @brief Executes a real end-to-end hardware pipeline benchmark (`CropScale` + `Blur` + `AiMask` + `Blend`)
 *        for the specified `(compute_backend, simd_mode, ai_enabled)` profile, returning both wall-clock
 *        average frame time (`out_avg_frame_ms`) and OS thread CPU time (`out_cpu_time_ms`).
 * @return 1 if the profile is supported and benchmarked, 0 if unsupported on the current machine.
 */
SMARTCAM_API int smartcam_engine_benchmark_profile(
    SmartCamEngineHandle handle,
    uint32_t compute_backend,
    uint32_t simd_mode,
    int ai_enabled,
    uint32_t width,
    uint32_t height,
    uint32_t iterations,
    float* out_avg_frame_ms,
    float* out_cpu_time_ms);

#ifdef __cplusplus
}
#endif

#endif  // SMARTCAM_COMMON_ENGINE_C_API_H_
