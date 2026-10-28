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

#ifndef SMARTCAM_COMMON_ENGINE_AI_ONNX_SEGMENTER_H_
#define SMARTCAM_COMMON_ENGINE_AI_ONNX_SEGMENTER_H_

#include <cstdint>
#include <memory>
#include <vector>
#include "../../include/frame_buffer.h"
#include "../../include/isegmenter.h"

namespace smartcam {
namespace common {
namespace engine {

/**
 * @brief Mode 2: Local AI Background Removal (ONNX Runtime / MediaPipe Selfie Segmentation).
 *
 * Designed specifically for laptops with integrated GPUs (Intel Iris Xe / AMD Radeon iGPU / Qualcomm Adreno)
 * and AVX2/AVX-512 CPUs:
 *   1. Downsamples full-HD input frame to lightweight inference resolution (`256x144` or `256x256`).
 *   2. Normalizes RGB channels to `[0.0f .. 1.0f]` NCHW/NHWC float32 tensor.
 *   3. Executes local neural network inference via ONNX Runtime (or accepts externally inferred probability map).
 *   4. Applies Exponential Moving Average (EMA) temporal smoothing in tensor space to eliminate edge jitter.
 *   5. Upsamples the smoothed soft mask to full output resolution using 16.16 fixed-point bilinear interpolation.
 */
class AiOnnxSegmenter final : public ISegmenter {
public:
    explicit AiOnnxSegmenter(const AiSegmenterConfig& config = AiSegmenterConfig{});
    ~AiOnnxSegmenter() override;

    /**
     * @brief Loads or reloads the ONNX model session.
     * If `SMARTCAM_WITH_ONNXRUNTIME` is not enabled at compile time or model file is absent,
     * initializes the deterministic portrait heuristic fallback so the pipeline remains testable.
     */
    bool InitializeModel();

    /**
     * @brief Computes foreground opacity mask (`out_mask`, `GRAY8`) from `input_frame`.
     */
    bool ComputeMask(const FrameView& input_frame, FrameView& out_mask) override;

    /**
     * @brief Ingests an externally computed low-resolution float probability tensor `[0.0..1.0]`
     *        (e.g., from Python ONNX Runtime / MediaPipe), applies EMA temporal smoothing,
     *        and bilinearly upsamples it into `out_mask`.
     */
    bool PostProcessProbabilityMap(const float* prob_map,
                                   uint32_t map_width,
                                   uint32_t map_height,
                                   FrameView& out_mask);

    /**
     * @brief Downsamples and normalizes `input_frame` (`BGRA32`/`RGBA32`) into a contiguous float32 RGB tensor `[0.0..1.0]`.
     */
    static bool PrepareInputTensorNHWC(const FrameView& input_frame,
                                       uint32_t tensor_width,
                                       uint32_t tensor_height,
                                       float* out_tensor_nhwc) noexcept;

    /**
     * @brief Captures and stores an empty background reference frame (without the user)
     *        for ultra-fast Reference Background Subtraction at `tw x th` resolution.
     */
    bool SetReferenceBackground(const FrameView& empty_bg_frame) noexcept;

    /**
     * @brief Clears the stored reference background and reverts to real-time dynamic tracking.
     */
    void ClearReferenceBackground() noexcept;

    /**
     * @brief Returns true if a valid reference background frame is currently active.
     */
    bool HasReferenceBackground() const noexcept { return has_reference_bg_; }

    /**
     * @brief Retrieves the normalized center `(cx, cy)` in `[0.0 .. 1.0]` of the tracked subject.
     */
    void GetTrackedSubjectCenter(float* out_cx, float* out_cy) const noexcept {
        if (out_cx) *out_cx = tracked_cx_;
        if (out_cy) *out_cy = tracked_cy_;
    }

    /**
     * @brief Configures AI engine mode (`0 = Auto/Hybrid`, `1 = Face/Silhouette Tracker Only`,
     *        `2 = Reference Background Difference Priority`) and spatial/chromatic tolerance `[0.0 .. 1.0]`.
     */
    void SetAiModeAndTolerance(int engine_mode, float bg_diff_tolerance) noexcept;

    int GetEngineMode() const noexcept { return config_.engine_mode; }
    float GetBgDiffTolerance() const noexcept { return config_.bg_diff_tolerance; }
    const AiSegmenterConfig& GetConfig() const noexcept { return config_; }

    /**
     * @brief Calibrates personalized facial skin YCbCr locus and anatomical head dimensions
     *        from 5 normalized user-selected landmarks (`left_eye`, `right_eye`, `nose_tip`, `lips`, `chin`).
     */
    bool CalibrateFaceFromLandmarks(const FrameView& snapshot_bgra,
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
                                    float* out_skin_cb = nullptr,
                                    float* out_skin_cr = nullptr,
                                    float* out_head_rx = nullptr,
                                    float* out_head_ry = nullptr) noexcept;

    /**
     * @brief Restores or updates a saved facial calibration profile directly.
     */
    void SetFaceCalibrationProfile(bool enabled,
                                   float skin_cb,
                                   float skin_cr,
                                   float head_rx,
                                   float head_ry) noexcept;

    /**
     * @brief Sets the 5 calibrated landmarks (`[rx,ry, lx,ly, nx,ny, mx,my, cx,cy]` in `[0.0 .. 1.0]`)
     *        and anchors the 5-point template constellation tracker.
     */
    void SetFaceCalibrationLandmarks(const float landmarks_10[10]) noexcept;

    /**
     * @brief Clears personalized facial calibration and reverts to universal portrait tracking.
     */
    void ClearFaceCalibration() noexcept;

    bool IsFaceCalibrated() const noexcept { return config_.face_calibrated; }

    /**
     * @brief Sets the silhouette cutout mode (`0 = Volto + Busto`, `1 = Solo Volto`).
     */
    void SetSilhouetteMode(int mode) noexcept;
    int GetSilhouetteMode() const noexcept { return config_.silhouette_mode; }

    /**
     * @brief Enables or disables multi-person tracking (up to 4 non-overlapping people).
     */
    void SetMultiPersonMode(bool enabled) noexcept;
    bool IsMultiPersonMode() const noexcept { return config_.multi_person_enabled; }

    SegmentationMode GetMode() const noexcept override { return SegmentationMode::AI_SELFIE; }
    const char* GetName() const noexcept override;
    bool IsNativeOrtLoaded() const noexcept { return ort_loaded_; }

private:
    void CaptureTemplateFromGrid(const float* input_nhwc,
                                 uint32_t tw,
                                 uint32_t th) noexcept;

    bool TrackCalibratedFaceTemplate(const float* input_nhwc,
                                     const float* luma_curr,
                                     const float* feat_grad_map,
                                     uint32_t tw,
                                     uint32_t th,
                                     float& out_cx,
                                     float& out_cy,
                                     float& out_rx) noexcept;

    void RunPortraitHeuristicFallback(const float* input_nhwc,
                                      uint32_t tw,
                                      uint32_t th,
                                      float* out_prob) noexcept;

    void RunReferenceBackgroundSubtraction(const float* input_nhwc,
                                           uint32_t tw,
                                           uint32_t th,
                                           float* out_prob) noexcept;

    static void ApplyEdgeAwareGuidedFilter3x3(const float* guide_rgb_nhwc,
                                              uint32_t tw,
                                              uint32_t th,
                                              float* inout_prob,
                                              float* scratch_prob) noexcept;

    AiSegmenterConfig    config_{};
    bool                 ort_loaded_              = false;
    bool                 has_reference_bg_        = false;
    bool                 tracker_initialized_     = false;
    bool                 has_calib_template_      = false;
    bool                 need_template_from_live_ = false;
    float                tracked_cx_              = 0.485f;
    float                tracked_cy_              = 0.490f;
    float                tracked_radius_          = 0.135f;
    float                pending_jump_cx_         = 0.485f;
    float                pending_jump_cy_         = 0.490f;
    int                  pending_jump_frames_     = 0;
    // 5-Point Calibrated Visual Template (`Occhio DX`, `Occhio SX`, `Naso`, `Labbra`, `Mento`)
    float                calib_rel_offsets_[5][2] = {{0.0f, 0.0f}};
    float                calib_lm_luma_[5][49]    = {{0.0f}};
    float                calib_lm_grad_[5][49]    = {{0.0f}};
    float                calib_lm_cb_[5]          = {113.0f, 113.0f, 113.0f, 113.0f, 113.0f};
    float                calib_lm_cr_[5]          = {148.0f, 148.0f, 148.0f, 148.0f, 148.0f};
    std::vector<float>   calib_tzone_luma_;
    std::vector<float>   calib_tzone_grad_;
    int                  calib_tzone_w_           = 0;
    int                  calib_tzone_h_           = 0;
    int                  calib_tzone_off_x_       = 0;
    int                  calib_tzone_off_y_       = 0;
    std::vector<float>   input_tensor_;
    std::vector<float>   curr_prob_map_;
    std::vector<float>   prev_prob_map_;
    std::vector<float>   scratch_prob_map_;
    std::vector<float>   prev_luma_map_;
    std::vector<float>   ref_bg_ycbcr_;
    std::vector<float>   ref_bg_raw_ycbcr_;
    // Pre-allocated reusable per-frame scratch buffers to avoid VirtualAlloc/heap churn
    std::vector<float>   bg_row_y_L_;
    std::vector<float>   bg_row_cb_L_;
    std::vector<float>   bg_row_cr_L_;
    std::vector<float>   bg_row_y_R_;
    std::vector<float>   bg_row_cb_R_;
    std::vector<float>   bg_row_cr_R_;
    std::vector<float>   bg_col_y_T_;
    std::vector<float>   bg_col_cb_T_;
    std::vector<float>   bg_col_cr_T_;
    std::vector<float>   skin_map_;
    std::vector<float>   luma_curr_;
    std::vector<float>   bg_diff_map_;
    std::vector<float>   feat_grad_map_;
    std::vector<float>   sat_skin_buf_;
    std::vector<float>   sat_grad_buf_;
    std::vector<float>   sat_luma_buf_;
    std::vector<float>   curr_ycbcr_;
    std::vector<float>   raw_row_dy_;
    std::vector<float>   row_dy_;
    std::vector<float>   density_map_;
    std::vector<int>     scan_left_;
    std::vector<int>     scan_right_;
    FrameBuffer          lowres_mask_;

    struct OrtImpl;
    std::unique_ptr<OrtImpl> ort_impl_;
};

}  // namespace engine
}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ENGINE_AI_ONNX_SEGMENTER_H_
