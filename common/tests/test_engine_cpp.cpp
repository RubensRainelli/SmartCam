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

#include <algorithm>
#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "../include/frame_buffer.h"
#include "../include/igpu_accelerator.h"
#include "../plugins/iplugin_source.h"
#include "../plugins/phone_camera_plugin.h"
#include "../engine/c_api.h"
#include "../engine/crop_scaler/crop_scaler.h"
#include "../engine/compositor/compositor.h"
#include "../engine/gpu/gpu_accelerator.h"
#include "../engine/segmentation/green_screen_segmenter.h"
#include "../engine/segmentation/ai_onnx_segmenter.h"

using namespace smartcam::common;
using namespace smartcam::common::engine;

namespace {

bool TestFrameBufferSimdAlignment() {
    FrameBuffer buf(1920, 1080, PixelFormat::BGRA32);
    if (!buf.IsValid()) return false;
    const uintptr_t addr = reinterpret_cast<uintptr_t>(buf.Data());
    if ((addr % FrameBuffer::kSimdAlignment) != 0) {
        std::fprintf(stderr, "[FAIL] FrameBuffer data pointer 0x%llx is not 64-byte aligned\n",
                     static_cast<unsigned long long>(addr));
        return false;
    }
    if (buf.Stride() != 1920 * 4 || buf.SizeBytes() != 1920 * 1080 * 4) {
        return false;
    }
    std::printf("[PASS] TestFrameBufferSimdAlignment (64-byte AVX-512 aligned)\n");
    return true;
}

bool TestCenterCrop16x9To4x3() {
    const CropRect rect = CropScaler::ComputeCenterCropRect(1920, 1080, 4, 3);
    if (rect.width != 1440 || rect.height != 1080 || rect.x != 240 || rect.y != 0) {
        std::fprintf(stderr, "[FAIL] Expected 1440x1080 at (240,0), got %ux%u at (%u,%u)\n",
                     rect.width, rect.height, rect.x, rect.y);
        return false;
    }

    FrameBuffer src(1920, 1080, PixelFormat::BGRA32);
    FrameBuffer dst(1440, 1080, PixelFormat::BGRA32);

    // Paint center pixel (960, 540) in source with distinct color (B=10, G=20, R=30, A=255)
    uint8_t* center_src = src.Data() + (540 * src.Stride()) + (960 * 4);
    center_src[0] = 10;
    center_src[1] = 20;
    center_src[2] = 30;
    center_src[3] = 255;

    if (!CropScaler::Process(src.View(), dst.View(), AspectRatioMode::RATIO_4_3, CropStrategy::CENTER_CROP)) {
        return false;
    }

    // In 1440x1080 center-cropped output, source (960, 540) maps to (960 - 240 = 720, 540)
    const uint8_t* center_dst = dst.Data() + (540 * dst.Stride()) + (720 * 4);
    if (center_dst[0] != 10 || center_dst[1] != 20 || center_dst[2] != 30) {
        std::fprintf(stderr, "[FAIL] Center pixel mismatch after 16:9->4:3 center crop\n");
        return false;
    }

    std::printf("[PASS] TestCenterCrop16x9To4x3 (1920x1080 -> 1440x1080 distortion-free)\n");
    return true;
}

bool TestGreenScreenHsvSegmentation() {
    const uint32_t w = 128, h = 128;
    FrameBuffer fg(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);

    // Fill left half with pure Chroma Green (R=20, G=220, B=20) -> background (mask ~ 0)
    // Fill right half with warm skin/subject tone (R=210, G=150, B=130) -> foreground (mask ~ 255)
    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = fg.Data() + y * fg.Stride();
        for (uint32_t x = 0; x < w; ++x) {
            if (x < w / 2) {
                row[x * 4 + 0] = 20;   // B
                row[x * 4 + 1] = 220;  // G
                row[x * 4 + 2] = 20;   // R
                row[x * 4 + 3] = 255;
            } else {
                row[x * 4 + 0] = 130;  // B
                row[x * 4 + 1] = 150;  // G
                row[x * 4 + 2] = 210;  // R
                row[x * 4 + 3] = 255;
            }
        }
    }

    GreenScreenConfig cfg;
    cfg.morph_kernel_size = 3;
    cfg.blur_radius       = 2;
    GreenScreenSegmenter segmenter(cfg);

    if (!segmenter.ComputeMask(fg.View(), mask.View())) {
        return false;
    }

    const uint8_t bg_sample   = mask.Data()[64 * mask.Stride() + 16];
    const uint8_t fg_sample   = mask.Data()[64 * mask.Stride() + 112];
    const uint8_t edge_sample = mask.Data()[64 * mask.Stride() + (w / 2)];

    if (bg_sample != 0 || fg_sample != 255 || edge_sample == 0 || edge_sample == 255) {
        std::fprintf(stderr, "[FAIL] GreenScreen mask values unexpected: bg=%u, fg=%u, edge=%u\n",
                     bg_sample, fg_sample, edge_sample);
        return false;
    }

    std::printf("[PASS] TestGreenScreenHsvSegmentation (bg=%u, fg=%u, feathered_edge=%u)\n",
                bg_sample, fg_sample, edge_sample);
    return true;
}

bool TestSimdCompositorEquivalence() {
    const uint32_t w = 333, h = 64;  // Non-multiple of 16 width to test SIMD + tail handling
    FrameBuffer fg(w, h, PixelFormat::BGRA32);
    FrameBuffer bg(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);
    FrameBuffer out_scalar(w, h, PixelFormat::BGRA32);
    FrameBuffer out_simd(w, h, PixelFormat::BGRA32);

    for (size_t i = 0; i < fg.SizeBytes(); ++i) {
        fg.Data()[i] = static_cast<uint8_t>((i * 37 + 13) & 0xFF);
        bg.Data()[i] = static_cast<uint8_t>((i * 73 + 91) & 0xFF);
    }
    for (size_t i = 0; i < mask.SizeBytes(); ++i) {
        mask.Data()[i] = static_cast<uint8_t>((i * 19) & 0xFF);
    }

    SimdCompositor comp_scalar(SimdTier::SCALAR);
    SimdCompositor comp_auto(SimdTier::AUTO);

    if (!comp_scalar.Blend(fg.View(), bg.View(), mask.View(), out_scalar.View())) return false;
    if (!comp_auto.Blend(fg.View(), bg.View(), mask.View(), out_simd.View())) return false;

    for (size_t i = 0; i < out_scalar.SizeBytes(); ++i) {
        const int diff = std::abs(static_cast<int>(out_scalar.Data()[i]) - static_cast<int>(out_simd.Data()[i]));
        if (diff > 1) {
            std::fprintf(stderr, "[FAIL] SIMD vs Scalar mismatch at byte %zu: scalar=%u, simd=%u\n",
                         i, out_scalar.Data()[i], out_simd.Data()[i]);
            return false;
        }
    }

    std::printf("[PASS] TestSimdCompositorEquivalence (Active SIMD Backend: %s)\n",
                comp_auto.GetActiveSimdBackend());
    return true;
}

bool TestAiSegmenterPostProcess() {
    AiSegmenterConfig cfg;
    cfg.input_width  = 64;
    cfg.input_height = 64;
    AiOnnxSegmenter ai_seg(cfg);

    FrameBuffer fg(256, 256, PixelFormat::BGRA32);
    SimdCompositor::FillSolidBackground(fg.View(), 200, 160, 140);
    FrameBuffer mask(256, 256, PixelFormat::GRAY8);

    if (!ai_seg.ComputeMask(fg.View(), mask.View())) {
        return false;
    }
    // Center of portrait heuristic mask should be opaque foreground (255)
    const uint8_t center_val = mask.Data()[128 * mask.Stride() + 128];
    if (center_val < 200) {
        std::fprintf(stderr, "[FAIL] AI segmenter center mask value too low: %u\n", center_val);
        return false;
    }

    std::printf("[PASS] TestAiSegmenterPostProcess (center_alpha=%u)\n", center_val);
    return true;
}

bool TestGreenScreenAutoCalibration() {
    const uint32_t w = 128, h = 128;
    FrameBuffer frame(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);

    // Paint upper background and left half with a Teal/Cyan-Green wall (R=15, G=200, B=185 -> Hue ~ 88)
    // and center-bottom subject with warm skin tone (R=220, G=150, B=120 -> Hue ~ 9)
    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = frame.Data() + y * frame.Stride();
        for (uint32_t x = 0; x < w; ++x) {
            const bool is_subject = (y > 40 && x >= 40 && x <= 88);
            if (!is_subject) {
                row[x * 4 + 0] = 185;  // B
                row[x * 4 + 1] = 200;  // G
                row[x * 4 + 2] = 15;   // R
                row[x * 4 + 3] = 255;
            } else {
                row[x * 4 + 0] = 120;  // B
                row[x * 4 + 1] = 150;  // G
                row[x * 4 + 2] = 220;  // R
                row[x * 4 + 3] = 255;
            }
        }
    }

    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    // 1. Auto-calibrate using 1-click automatic corner sampling (sample_x = -1, sample_y = -1)
    uint8_t h_min = 0, h_max = 0, s_min = 0, v_min = 0;
    if (!smartcam_engine_autocalibrate_greenscreen(
            engine, frame.Data(), w, h, frame.Stride(), -1, -1, &h_min, &h_max, &s_min, &v_min)) {
        smartcam_engine_destroy(engine);
        return false;
    }

    // Verify computed HSV range covers Hue ~ 88
    if (h_min > 85 || h_max < 90 || s_min == 0 || v_min == 0) {
        std::fprintf(stderr, "[FAIL] AutoCalibrate corner bounds unexpected: H=[%u..%u], S_min=%u, V_min=%u\n",
                     h_min, h_max, s_min, v_min);
        smartcam_engine_destroy(engine);
        return false;
    }

    // 2. Test Eyedropper mode at (16, 64)
    uint8_t eye_h_min = 0, eye_h_max = 0, eye_s_min = 0, eye_v_min = 0;
    if (!smartcam_engine_autocalibrate_greenscreen(
            engine, frame.Data(), w, h, frame.Stride(), 16, 64, &eye_h_min, &eye_h_max, &eye_s_min, &eye_v_min)) {
        smartcam_engine_destroy(engine);
        return false;
    }

    if (!smartcam_engine_compute_greenscreen_mask(
            engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    const uint8_t bg_alpha = mask.Data()[10 * mask.Stride() + 10];
    const uint8_t fg_alpha = mask.Data()[80 * mask.Stride() + 64];
    smartcam_engine_destroy(engine);

    if (bg_alpha != 0 || fg_alpha != 255) {
        std::fprintf(stderr, "[FAIL] Auto-calibrated mask unexpected: bg=%u, fg=%u\n", bg_alpha, fg_alpha);
        return false;
    }

    std::printf("[PASS] TestGreenScreenAutoCalibration (H=[%u..%u], S>=%u, V>=%u, bg=%u, fg=%u)\n",
                eye_h_min, eye_h_max, eye_s_min, eye_v_min, bg_alpha, fg_alpha);
    return true;
}

bool TestGreenSpillSuppressionPixelFormats() {
    GreenScreenSegmenter segmenter;  // default spill_suppression = 0.35 -> strength 89/256

    // Supported: BGRA32 / RGBA32 frame + GRAY8 mask. G=200 over (R+B)/2=55 -> 200 - (145*89 >> 8) = 150.
    for (PixelFormat fmt : {PixelFormat::BGRA32, PixelFormat::RGBA32}) {
        std::vector<uint8_t> px(4 * 4);
        for (size_t i = 0; i < px.size(); i += 4) {
            px[i + 0] = 50; px[i + 1] = 200; px[i + 2] = 60; px[i + 3] = 255;
        }
        std::vector<uint8_t> mask4(4, 255);
        FrameView px_view{px.data(), 4, 1, 4 * 4, fmt, 0, 0};
        FrameView mask4_view{mask4.data(), 4, 1, 4, PixelFormat::GRAY8, 0, 0};
        segmenter.SuppressGreenSpill(px_view, mask4_view);
        for (size_t i = 0; i < px.size(); i += 4) {
            if (px[i + 0] != 50 || px[i + 1] != 150 || px[i + 2] != 60 || px[i + 3] != 255) {
                std::fprintf(stderr, "[FAIL] %s spill suppression produced (%u,%u,%u,%u), expected (50,150,60,255)\n",
                             fmt == PixelFormat::BGRA32 ? "BGRA32" : "RGBA32", px[i + 0], px[i + 1], px[i + 2], px[i + 3]);
                return false;
            }
        }
    }

    // Unsupported layouts must be left untouched: the per-pixel loop assumes 4 bytes per pixel
    // and an 8-bit GRAY8 mask. Each frame is followed by guard bytes inside the same allocation.
    struct Case {
        const char* name;
        PixelFormat frame_format;
        PixelFormat mask_format;
    };
    const Case cases[] = {
        {"RGB24 frame", PixelFormat::RGB24, PixelFormat::GRAY8},
        {"BGR24 frame", PixelFormat::BGR24, PixelFormat::GRAY8},
        {"BGRA64 frame", PixelFormat::BGRA64, PixelFormat::GRAY8},
        {"BGRA32 mask", PixelFormat::BGRA32, PixelFormat::BGRA32},
    };
    constexpr uint32_t kWidth = 16;
    constexpr size_t kGuardBytes = 64;
    for (const Case& c : cases) {
        const uint32_t frame_stride = kWidth * GetBytesPerPixel(c.frame_format);
        std::vector<uint8_t> frame(frame_stride + kGuardBytes);
        for (size_t i = 0; i < frame.size(); ++i) {
            frame[i] = (i % 3 == 1) ? 200 : 50;  // green-dominant bytes wherever G could be read
        }
        const std::vector<uint8_t> frame_before = frame;
        const uint32_t mask_stride = kWidth * GetBytesPerPixel(c.mask_format);
        std::vector<uint8_t> mask(mask_stride, 255);
        FrameView frame_view{frame.data(), kWidth, 1, frame_stride, c.frame_format, 0, 0};
        FrameView mask_view{mask.data(), kWidth, 1, mask_stride, c.mask_format, 0, 0};

        segmenter.SuppressGreenSpill(frame_view, mask_view);

        if (frame != frame_before) {
            std::fprintf(stderr, "[FAIL] SuppressGreenSpill modified a %s it cannot process\n", c.name);
            return false;
        }
    }

    std::printf("[PASS] TestGreenSpillSuppressionPixelFormats (BGRA32+RGBA32 G 200->150, RGB24/BGR24/BGRA64/non-GRAY8 mask untouched)\n");
    return true;
}

bool TestCApiContextIsolationAndHelpers() {
    const uint32_t w = 64, h = 64;
    SmartCamEngineHandle ctx_green = smartcam_engine_create(1);  // Scalar
    SmartCamEngineHandle ctx_blue  = smartcam_engine_create(0);  // Auto SIMD
    if (!ctx_green || !ctx_blue) return false;

    // Configure ctx_green for standard green (Hue 45..75) and ctx_blue for blue screen (Hue 105..135)
    smartcam_engine_configure_greenscreen(ctx_green, 45, 75, 50, 255, 40, 255, 1, 0, 0.0f);
    smartcam_engine_configure_greenscreen(ctx_blue, 105, 135, 50, 255, 40, 255, 1, 0, 0.0f);

    FrameBuffer green_frame(w, h, PixelFormat::BGRA32);
    FrameBuffer mask_from_green_ctx(w, h, PixelFormat::GRAY8);
    FrameBuffer mask_from_blue_ctx(w, h, PixelFormat::GRAY8);

    // Fill frame with pure green (R=10, G=220, B=10 -> Hue = 60)
    if (!smartcam_engine_fill_solid_bg(green_frame.Data(), w, h, green_frame.Stride(), 10, 220, 10)) {
        smartcam_engine_destroy(ctx_green);
        smartcam_engine_destroy(ctx_blue);
        return false;
    }

    FrameBuffer green_copy(w, h, PixelFormat::BGRA32);
    smartcam_engine_fill_solid_bg(green_copy.Data(), w, h, green_copy.Stride(), 10, 220, 10);

    smartcam_engine_compute_greenscreen_mask(
        ctx_green, green_frame.Data(), w, h, green_frame.Stride(),
        mask_from_green_ctx.Data(), mask_from_green_ctx.Stride());
    smartcam_engine_compute_greenscreen_mask(
        ctx_blue, green_copy.Data(), w, h, green_copy.Stride(),
        mask_from_blue_ctx.Data(), mask_from_blue_ctx.Stride());

    // ctx_green must classify pure green as background (0), while ctx_blue classifies it as foreground (255)
    const uint8_t m_green = mask_from_green_ctx.Data()[32 * mask_from_green_ctx.Stride() + 32];
    const uint8_t m_blue  = mask_from_blue_ctx.Data()[32 * mask_from_blue_ctx.Stride() + 32];

    FrameBuffer blurred_bg(w, h, PixelFormat::BGRA32);
    const int blur_ok = smartcam_engine_generate_blurred_bg(
        green_copy.Data(), blurred_bg.Data(), w, h, green_copy.Stride(), 4);
    const int ai_ok = smartcam_engine_compute_ai_mask(
        ctx_blue, green_copy.Data(), w, h, green_copy.Stride(),
        mask_from_blue_ctx.Data(), mask_from_blue_ctx.Stride());

    smartcam_engine_destroy(ctx_green);
    smartcam_engine_destroy(ctx_blue);

    if (m_green != 0 || m_blue != 255 || !blur_ok || !ai_ok) {
        std::fprintf(stderr, "[FAIL] Context isolation or helper check failed (m_green=%u, m_blue=%u, blur=%d, ai=%d)\n",
                     m_green, m_blue, blur_ok, ai_ok);
        return false;
    }

    std::printf("[PASS] TestCApiContextIsolationAndHelpers (isolated ctx_green=%u vs ctx_blue=%u)\n",
                m_green, m_blue);
    return true;
}

bool TestDynamicTrackingAndReferenceBackground() {
    const uint32_t w = 256, h = 144;
    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    auto render_scene = [&](FrameBuffer& buf, float subj_cx_norm, bool include_subject) {
        const float subj_cx = subj_cx_norm * static_cast<float>(w);
        const float head_cy = 0.38f * static_cast<float>(h);
        const float torso_cy = 0.68f * static_cast<float>(h);

        for (uint32_t y = 0; y < h; ++y) {
            uint8_t* row = buf.Data() + y * buf.Stride();
            for (uint32_t x = 0; x < w; ++x) {
                // Cool slate-blue room wall background (R=55, G=70, B=95)
                uint8_t r = 55, g = 70, b = 95;

                if (include_subject) {
                    const float dx_h = (static_cast<float>(x) - subj_cx) / (0.10f * w);
                    const float dy_h = (static_cast<float>(y) - head_cy) / (0.18f * h);
                    const float dx_t = (static_cast<float>(x) - subj_cx) / (0.18f * w);
                    const float dy_t = (static_cast<float>(y) - torso_cy) / (0.26f * h);

                    if (dx_h * dx_h + dy_h * dy_h <= 1.0f) {
                        // Warm skin-tone head (R=215, G=155, B=125 -> YCbCr skin locus)
                        r = 215; g = 155; b = 125;
                    } else if (dx_t * dx_t + dy_t * dy_t <= 1.0f) {
                        // Distinct burgundy sweater/shoulders (R=170, G=45, B=55)
                        r = 170; g = 45; b = 55;
                    }
                }

                row[x * 4 + 0] = b;
                row[x * 4 + 1] = g;
                row[x * 4 + 2] = r;
                row[x * 4 + 3] = 255;
            }
        }
    };

    FrameBuffer frame(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);

    // 1. Dynamic Subject Tracking: Subject at Left (x = 0.25)
    render_scene(frame, 0.25f, true);
    if (!smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    float cx_left = 0.0f, cy_left = 0.0f;
    smartcam_engine_get_subject_center(engine, &cx_left, &cy_left);
    const uint8_t mask_left_on_left   = mask.Data()[static_cast<size_t>(0.38f * h) * mask.Stride() + static_cast<size_t>(0.25f * w)];
    const uint8_t mask_right_on_left  = mask.Data()[static_cast<size_t>(0.38f * h) * mask.Stride() + static_cast<size_t>(0.75f * w)];

    // 2. Dynamic Subject Tracking: Subject moves to Right (x = 0.75)
    render_scene(frame, 0.75f, true);
    if (!smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    float cx_right = 0.0f, cy_right = 0.0f;
    smartcam_engine_get_subject_center(engine, &cx_right, &cy_right);
    const uint8_t mask_left_on_right  = mask.Data()[static_cast<size_t>(0.38f * h) * mask.Stride() + static_cast<size_t>(0.25f * w)];
    const uint8_t mask_right_on_right = mask.Data()[static_cast<size_t>(0.38f * h) * mask.Stride() + static_cast<size_t>(0.75f * w)];

    if (cx_left > 0.36f || cx_right < 0.64f ||
        mask_left_on_left < 200 || mask_right_on_left > 25 ||
        mask_right_on_right < 200 || mask_left_on_right > 25) {
        std::fprintf(stderr,
                     "[FAIL] Dynamic tracking failed: cx_left=%.2f (mL=%u, mR=%u), cx_right=%.2f (mL=%u, mR=%u)\n",
                     cx_left, mask_left_on_left, mask_right_on_left,
                     cx_right, mask_left_on_right, mask_right_on_right);
        smartcam_engine_destroy(engine);
        return false;
    }

    // 3. Reference Background Subtraction Mode ("Capture Background Without User")
    FrameBuffer empty_bg(w, h, PixelFormat::BGRA32);
    render_scene(empty_bg, 0.50f, false);

    if (smartcam_engine_has_reference_background(engine) != 0) {
        smartcam_engine_destroy(engine);
        return false;
    }
    if (!smartcam_engine_set_reference_background(engine, empty_bg.Data(), w, h, empty_bg.Stride()) ||
        smartcam_engine_has_reference_background(engine) != 1) {
        std::fprintf(stderr, "[FAIL] Failed to set reference background\n");
        smartcam_engine_destroy(engine);
        return false;
    }

    // Render subject at x = 0.30 in front of the captured reference background
    render_scene(frame, 0.30f, true);
    if (!smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    const uint8_t ref_fg_head  = mask.Data()[static_cast<size_t>(0.38f * h) * mask.Stride() + static_cast<size_t>(0.30f * w)];
    const uint8_t ref_fg_torso = mask.Data()[static_cast<size_t>(0.68f * h) * mask.Stride() + static_cast<size_t>(0.30f * w)];
    const uint8_t ref_bg_empty = mask.Data()[static_cast<size_t>(0.50f * h) * mask.Stride() + static_cast<size_t>(0.80f * w)];

    smartcam_engine_clear_reference_background(engine);
    const int has_ref_after_clear = smartcam_engine_has_reference_background(engine);
    smartcam_engine_destroy(engine);

    if (ref_fg_head < 220 || ref_fg_torso < 220 || ref_bg_empty > 10 || has_ref_after_clear != 0) {
        std::fprintf(stderr,
                     "[FAIL] Reference BG subtraction failed: head=%u, torso=%u, bg=%u, cleared=%d\n",
                     ref_fg_head, ref_fg_torso, ref_bg_empty, has_ref_after_clear);
        return false;
    }

    std::printf("[PASS] TestDynamicTrackingAndReferenceBackground (cx: %.2f->%.2f, ref_fg=%u, ref_bg=%u)\n",
                cx_left, cx_right, ref_fg_head, ref_bg_empty);
    return true;
}

bool TestAntiLedTearingAndSpatialTolerance() {
    const uint32_t w = 256, h = 144;
    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    smartcam_engine_set_ai_mode_and_tolerance(engine, 2, 0.55f);

    // Helper to render room background with a high-contrast picture frame on the right wall
    auto sample_room_bg = [&](int x, int y, uint8_t& out_r, uint8_t& out_g, uint8_t& out_b) {
        const int cx = std::clamp(x, 0, static_cast<int>(w) - 1);
        const int cy = std::clamp(y, 0, static_cast<int>(h) - 1);
        // Base room wall: cool gray-blue (R=65, G=80, B=105)
        out_r = 65; out_g = 80; out_b = 105;
        // High-contrast bookshelf/frame on the right wall (x in [185..215], y in [30..90])
        if (cx >= 185 && cx <= 215 && cy >= 30 && cy <= 90) {
            out_r = 165; out_g = 120; out_b = 60;
        }
    };

    // 1. Capture Empty Reference Background
    FrameBuffer empty_bg(w, h, PixelFormat::BGRA32);
    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = empty_bg.Data() + y * empty_bg.Stride();
        for (uint32_t x = 0; x < w; ++x) {
            uint8_t r = 0, g = 0, b = 0;
            sample_room_bg(static_cast<int>(x), static_cast<int>(y), r, g, b);
            row[x * 4 + 0] = b;
            row[x * 4 + 1] = g;
            row[x * 4 + 2] = r;
            row[x * 4 + 3] = 255;
        }
    }

    if (!smartcam_engine_set_reference_background(engine, empty_bg.Data(), w, h, empty_bg.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    // 2. Render Live Frame with:
    //    - 1-pixel spatial micro-shift (dx = +1, dy = +1)
    //    - Global room brightness shift (+14 luma) + Horizontal LED PWM tearing bands (+-18 luma every 6 rows)
    //    - Subject at x = 0.40 whose shirt interior has a patch with the exact same color as the wall
    FrameBuffer live_frame(w, h, PixelFormat::BGRA32);
    const float subj_cx  = 0.40f * static_cast<float>(w);
    const float head_cy  = 0.36f * static_cast<float>(h);
    const float torso_cy = 0.70f * static_cast<float>(h);

    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = live_frame.Data() + y * live_frame.Stride();
        // Simulated rolling-shutter LED PWM horizontal banding + global exposure shift
        const int led_band_delta = ((y / 6) % 2 == 0) ? 22 : -12;

        for (uint32_t x = 0; x < w; ++x) {
            // 1-pixel camera micro-shift on background
            uint8_t r = 0, g = 0, b = 0;
            sample_room_bg(static_cast<int>(x) - 1, static_cast<int>(y) - 1, r, g, b);

            const float dx_h = (static_cast<float>(x) - subj_cx) / (0.10f * w);
            const float dy_h = (static_cast<float>(y) - head_cy) / (0.18f * h);
            const float dx_t = (static_cast<float>(x) - subj_cx) / (0.18f * w);
            const float dy_t = (static_cast<float>(y) - torso_cy) / (0.28f * h);

            if (dx_h * dx_h + dy_h * dy_h <= 1.0f) {
                r = 215; g = 155; b = 125;
            } else if (dx_t * dx_t + dy_t * dy_t <= 1.0f) {
                // Inner chest patch has wall-like color to test Scanline Silhouette Fill (zero internal holes!)
                if (std::abs(dx_t) < 0.45f && std::abs(dy_t) < 0.45f) {
                    r = 65; g = 80; b = 105;
                } else {
                    r = 180; g = 50; b = 60;
                }
            }

            row[x * 4 + 0] = static_cast<uint8_t>(std::clamp(static_cast<int>(b) + led_band_delta, 0, 255));
            row[x * 4 + 1] = static_cast<uint8_t>(std::clamp(static_cast<int>(g) + led_band_delta, 0, 255));
            row[x * 4 + 2] = static_cast<uint8_t>(std::clamp(static_cast<int>(r) + led_band_delta, 0, 255));
            row[x * 4 + 3] = 255;
        }
    }

    FrameBuffer mask(w, h, PixelFormat::GRAY8);
    if (!smartcam_engine_compute_ai_mask(engine, live_frame.Data(), w, h, live_frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    const uint8_t bg_wall_alpha      = mask.Data()[static_cast<size_t>(0.25f * h) * mask.Stride() + static_cast<size_t>(0.12f * w)];
    const uint8_t bg_shifted_edge    = mask.Data()[static_cast<size_t>(60) * mask.Stride() + 185];
    const uint8_t fg_head_alpha      = mask.Data()[static_cast<size_t>(0.36f * h) * mask.Stride() + static_cast<size_t>(0.40f * w)];
    const uint8_t fg_chest_hole_test = mask.Data()[static_cast<size_t>(0.70f * h) * mask.Stride() + static_cast<size_t>(0.40f * w)];

    smartcam_engine_destroy(engine);

    if (bg_wall_alpha != 0 || bg_shifted_edge != 0 || fg_head_alpha <= 240 || fg_chest_hole_test <= 240) {
        std::fprintf(stderr,
                     "[FAIL] Anti-LED / Spatial Tolerance test failed: bg_wall=%u, bg_edge=%u, head=%u, chest_fill=%u\n",
                     bg_wall_alpha, bg_shifted_edge, fg_head_alpha, fg_chest_hole_test);
        return false;
    }

    std::printf("[PASS] TestAntiLedTearingAndSpatialTolerance (bg_wall=%u, bg_shifted_edge=%u, head=%u, chest_fill=%u)\n",
                bg_wall_alpha, bg_shifted_edge, fg_head_alpha, fg_chest_hole_test);
    return true;
}

bool TestSolidSilhouetteWithColoredFurniture() {
    const uint32_t w = 256, h = 144;
    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    // Force Mode 1: Face / Solid Silhouette Tracker Only (no reference background)
    smartcam_engine_set_ai_mode_and_tolerance(engine, 1, 0.50f);

    FrameBuffer frame(w, h, PixelFormat::BGRA32);
    const float subj_cx  = 0.35f * static_cast<float>(w);
    const float head_cy  = 0.38f * static_cast<float>(h);
    const float torso_cy = 0.68f * static_cast<float>(h);

    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = frame.Data() + y * frame.Stride();
        for (uint32_t x = 0; x < w; ++x) {
            // Gray-blue room wall (R=60, G=75, B=100)
            uint8_t r = 60, g = 75, b = 100;

            // Bright colorful furniture far from the subject on the right side (x in [180..235], y in [25..120])
            if (x >= 180 && x <= 235 && y >= 25 && y <= 120) {
                r = 40; g = 210; b = 220;  // Bright cyan/teal cabinet
            }

            const float dx_h = (static_cast<float>(x) - subj_cx) / (0.10f * w);
            const float dy_h = (static_cast<float>(y) - head_cy) / (0.18f * h);
            const float dx_t = (static_cast<float>(x) - subj_cx) / (0.18f * w);
            const float dy_t = (static_cast<float>(y) - torso_cy) / (0.26f * h);

            if (dx_h * dx_h + dy_h * dy_h <= 1.0f) {
                // Skin-tone face with internal horizontal contrast (eyes/brows/mouth)
                r = 215; g = 155; b = 125;
                if (std::abs(dy_h + 0.15f) < 0.12f && std::abs(dx_h) < 0.55f) {
                    r = 95; g = 65; b = 55;  // Eyes/eyebrows horizontal contrast
                }
            } else if (dx_t * dx_t + dy_t * dy_t <= 1.0f) {
                // Subject wears a shirt with the EXACT SAME COLOR as the background wall!
                r = 60; g = 75; b = 100;
            }

            row[x * 4 + 0] = b;
            row[x * 4 + 1] = g;
            row[x * 4 + 2] = r;
            row[x * 4 + 3] = 255;
        }
    }

    FrameBuffer mask(w, h, PixelFormat::GRAY8);
    if (!smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    const uint8_t furniture_alpha = mask.Data()[static_cast<size_t>(0.50f * h) * mask.Stride() + 210];
    const uint8_t head_alpha      = mask.Data()[static_cast<size_t>(0.38f * h) * mask.Stride() + static_cast<size_t>(0.35f * w)];
    const uint8_t shirt_alpha     = mask.Data()[static_cast<size_t>(0.68f * h) * mask.Stride() + static_cast<size_t>(0.35f * w)];

    smartcam_engine_destroy(engine);

    if (furniture_alpha != 0 || head_alpha != 255 || shirt_alpha != 255) {
        std::fprintf(stderr,
                     "[FAIL] Solid Silhouette test failed: furniture=%u (expected 0), head=%u (expected 255), wall_colored_shirt=%u (expected 255)\n",
                     furniture_alpha, head_alpha, shirt_alpha);
        return false;
    }

    std::printf("[PASS] TestSolidSilhouetteWithColoredFurniture (furniture=%u, head=%u, wall_colored_shirt=%u)\n",
                furniture_alpha, head_alpha, shirt_alpha);
    return true;
}

bool TestGpuProbeAndHardwarePolicy() {
    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    const int dev_count = smartcam_engine_get_compute_device_count(engine);
    if (dev_count < 5) {
        std::fprintf(stderr, "[FAIL] Expected at least 5 compute devices (CPU, Vulkan, OpenCL, CUDA, ROCm), got %d\n",
                     dev_count);
        smartcam_engine_destroy(engine);
        return false;
    }

    bool seen_cpu = false;
    bool seen_vk  = false;
    bool seen_ocl = false;
    bool seen_cu  = false;
    bool seen_hip = false;

    for (int i = 0; i < dev_count; ++i) {
        uint32_t backend_id = 0;
        char backend_name[64] = {0};
        char device_name[128] = {0};
        uint64_t vram_mb = 0;
        int available = 0;
        int is_igpu = 0;

        if (!smartcam_engine_get_compute_device_info(
                engine, i, &backend_id,
                backend_name, sizeof(backend_name),
                device_name, sizeof(device_name),
                &vram_mb, &available, &is_igpu)) {
            std::fprintf(stderr, "[FAIL] smartcam_engine_get_compute_device_info failed at index %d\n", i);
            smartcam_engine_destroy(engine);
            return false;
        }

        if (std::strlen(backend_name) == 0 || std::strlen(device_name) == 0) {
            std::fprintf(stderr, "[FAIL] Empty backend_name or device_name at index %d\n", i);
            smartcam_engine_destroy(engine);
            return false;
        }

        if (backend_id == 1) {
            seen_cpu = true;
            if (available != 1) {
                std::fprintf(stderr, "[FAIL] CPU backend must always be available\n");
                smartcam_engine_destroy(engine);
                return false;
            }
        } else if (backend_id == 2) {
            seen_vk = true;
        } else if (backend_id == 3) {
            seen_ocl = true;
        } else if (backend_id == 4) {
            seen_cu = true;
        } else if (backend_id == 5) {
            seen_hip = true;
        }
    }

    if (!seen_cpu || !seen_vk || !seen_ocl || !seen_cu || !seen_hip) {
        std::fprintf(stderr, "[FAIL] Missing one of the 5 compute backends (CPU=%d, VK=%d, OCL=%d, CUDA=%d, ROCm=%d)\n",
                     seen_cpu ? 1 : 0, seen_vk ? 1 : 0, seen_ocl ? 1 : 0, seen_cu ? 1 : 0, seen_hip ? 1 : 0);
        smartcam_engine_destroy(engine);
        return false;
    }

    // Verify live switching of SIMD mode (3=Scalar, 2=AVX2, 1=AVX512, 0=Auto)
    if (!smartcam_engine_set_hardware_policy(engine, 1, 3, 1)) {
        smartcam_engine_destroy(engine);
        return false;
    }
    const std::string simd_scalar = smartcam_engine_get_simd_backend(engine);
    if (simd_scalar != "Scalar") {
        std::fprintf(stderr, "[FAIL] Expected 'Scalar' after simd_mode=3, got '%s'\n", simd_scalar.c_str());
        smartcam_engine_destroy(engine);
        return false;
    }

    if (!smartcam_engine_set_hardware_policy(engine, 1, 2, 1)) {
        smartcam_engine_destroy(engine);
        return false;
    }
    const std::string simd_avx2 = smartcam_engine_get_simd_backend(engine);
    if (SimdCompositor::DetectBestSimdTier() >= SimdTier::AVX2 && simd_avx2 != "AVX2") {
        std::fprintf(stderr, "[FAIL] Expected 'AVX2' after simd_mode=2, got '%s'\n", simd_avx2.c_str());
        smartcam_engine_destroy(engine);
        return false;
    }

    if (!smartcam_engine_set_hardware_policy(engine, 1, 1, 1)) {
        smartcam_engine_destroy(engine);
        return false;
    }
    const std::string simd_avx512 = smartcam_engine_get_simd_backend(engine);

    if (!smartcam_engine_set_hardware_policy(engine, 0, 0, 1)) {
        smartcam_engine_destroy(engine);
        return false;
    }
    const std::string simd_auto = smartcam_engine_get_simd_backend(engine);

    // Verify live switching across all compute backends (0..5) and AI enabled (0 vs 1)
    const uint32_t w = 256, h = 144;
    FrameBuffer fg(w, h, PixelFormat::BGRA32);
    FrameBuffer bg(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);
    FrameBuffer out(w, h, PixelFormat::BGRA32);

    for (uint32_t y = 0; y < h; ++y) {
        uint8_t* row = fg.Data() + y * fg.Stride();
        for (uint32_t x = 0; x < w; ++x) {
            const float dx = (static_cast<float>(x) - 0.50f * w) / (0.12f * w);
            const float dy = (static_cast<float>(y) - 0.40f * h) / (0.20f * h);
            if (dx * dx + dy * dy <= 1.0f) {
                row[x * 4 + 0] = 125; row[x * 4 + 1] = 155; row[x * 4 + 2] = 215; row[x * 4 + 3] = 255;
            } else {
                row[x * 4 + 0] = 95;  row[x * 4 + 1] = 70;  row[x * 4 + 2] = 55;  row[x * 4 + 3] = 255;
            }
        }
    }
    SimdCompositor::FillSolidBackground(bg.View(), 20, 40, 80);

    // Test AI Disabled (ai_enabled = 0 -> Ultra-Fast Non-AI path) vs AI Enabled (ai_enabled = 1)
    if (!smartcam_engine_set_hardware_policy(engine, 1, 0, 0) || smartcam_engine_is_ai_enabled(engine) != 0) {
        smartcam_engine_destroy(engine);
        return false;
    }
    if (!smartcam_engine_compute_ai_mask(engine, fg.Data(), w, h, fg.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }
    const uint8_t non_ai_center = mask.Data()[(h / 2) * mask.Stride() + (w / 2)];
    const uint8_t non_ai_corner = mask.Data()[4 * mask.Stride() + 4];
    if (non_ai_center < 200 || non_ai_corner != 0) {
        std::fprintf(stderr, "[FAIL] Fast Non-AI mask unexpected: center=%u, corner=%u\n",
                     non_ai_center, non_ai_corner);
        smartcam_engine_destroy(engine);
        return false;
    }

    for (uint32_t backend_id = 0; backend_id <= 5; ++backend_id) {
        if (!smartcam_engine_set_hardware_policy(engine, backend_id, 0, 1)) {
            smartcam_engine_destroy(engine);
            return false;
        }
        if (!smartcam_engine_compute_ai_mask(engine, fg.Data(), w, h, fg.Stride(), mask.Data(), mask.Stride())) {
            smartcam_engine_destroy(engine);
            return false;
        }
        if (!smartcam_engine_blend_bgra(engine, fg.Data(), bg.Data(), mask.Data(), out.Data(),
                                        w, h, fg.Stride(), mask.Stride())) {
            smartcam_engine_destroy(engine);
            return false;
        }
    }

    const std::string active_backend = smartcam_engine_get_active_compute_backend(engine);
    const std::string active_device  = smartcam_engine_get_active_compute_device_name(engine);
    smartcam_engine_destroy(engine);

    std::printf("[PASS] TestGpuProbeAndHardwarePolicy (devices=%d, simd: %s/%s/%s/%s, active=%s [%s])\n",
                dev_count,
                simd_scalar.c_str(), simd_avx2.c_str(), simd_avx512.c_str(), simd_auto.c_str(),
                active_backend.c_str(), active_device.c_str());
    return true;
}

bool TestPhoneCameraPluginAndCatalogue() {
    using namespace smartcam::common::plugins;

    const auto& catalogue = GetBuiltInPluginsCatalogue();
    if (catalogue.size() < 4) {
        std::fprintf(stderr, "[FAIL] Expected at least 4 built-in plugins in catalogue, got %zu\n", catalogue.size());
        return false;
    }

    PhoneCameraPlugin phone_plugin;
    if (std::strcmp(phone_plugin.GetPluginId(), "phone_camera_source") != 0) {
        return false;
    }

    const ParsedPhoneEndpoint ep = PhoneCameraPlugin::ParseEndpointUri("http://192.168.1.42:8080/video");
    if (ep.host != "192.168.1.42" || ep.port != 8080 || ep.path != "/video" || ep.is_usb) {
        std::fprintf(stderr, "[FAIL] PhoneCameraPlugin URI parsing failed\n");
        return false;
    }

    CameraCaptureConfig cap_cfg{};
    cap_cfg.width  = 640;
    cap_cfg.height = 360;
    cap_cfg.target_fps = 30;
    if (!phone_plugin.Open(cap_cfg) || !phone_plugin.IsOpen() || !phone_plugin.IsEnabled()) {
        return false;
    }

    FrameBuffer pairing_frame;
    if (!phone_plugin.ReadFrame(pairing_frame, 100) ||
        !pairing_frame.IsValid() ||
        pairing_frame.Width() != 640 ||
        pairing_frame.Height() != 360) {
        std::fprintf(stderr, "[FAIL] PhoneCameraPlugin standby pairing card frame failed\n");
        return false;
    }

    const SmartCamPluginDescriptorV1* desc = GetPhoneCameraPluginDescriptorV1();
    if (!desc || desc->abi_version != SMARTCAM_PLUGIN_ABI_VERSION_V1 || !desc->init || !desc->destroy) {
        return false;
    }

    phone_plugin.Close();
    std::printf("[PASS] TestPhoneCameraPluginAndCatalogue (plugins=%zu, phone_endpoint=%s:%u%s)\n",
                catalogue.size(), ep.host.c_str(), ep.port, ep.path.c_str());
    return true;
}

bool TestWoodenStairsYellowHoodieZeroParallax() {
    // Reproduces the EXACT RGB colors and facial landmark coordinates measured from the user's live UVC webcam:
    //   - Upper white room wall: (R=175, G=175, B=170)
    //   - Warm wooden staircase on left border (x < 0.43, y in [0.45..0.88]): (R=125, G=77, B=28) & (R=131, G=73, B=35)
    //   - Mustard-yellow hoodie (y in [0.60..1.00]): (R=100, G=72, B=16)
    //   - User's face centered at (cx=0.548, cy=0.382) with:
    //       * Left cheek (lit): (R=139, G=111, B=93)
    //       * Right cheek & forehead (shaded indoor UVC): (R=91, G=54, B=38) & (R=86, G=50, B=34)
    //       * Left eye at (0.489, 0.333), Right eye at (0.609, 0.336)
    //       * Nostrils at (0.553, 0.429), Mouth line at (0.545, 0.521)
    const uint32_t w = 256, h = 144;
    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    smartcam_engine_set_ai_mode_and_tolerance(engine, 1, 0.50f);

    auto render_wooden_room_scene = [&](FrameBuffer& buf, float face_cx_norm) {
        const float face_cx = face_cx_norm * static_cast<float>(w);
        const float face_cy = 0.385f * static_cast<float>(h);
        const float face_rx = 0.125f * static_cast<float>(w);
        const float face_ry = 0.235f * static_cast<float>(h);
        const float body_cy = 0.840f * static_cast<float>(h);
        const float body_rx = 0.220f * static_cast<float>(w);
        const float body_ry = 0.280f * static_cast<float>(h);

        for (uint32_t y = 0; y < h; ++y) {
            const float ny = static_cast<float>(y) / static_cast<float>(h);
            uint8_t* row = buf.Data() + y * buf.Stride();
            for (uint32_t x = 0; x < w; ++x) {
                const float nx = static_cast<float>(x) / static_cast<float>(w);

                // Default upper white room wall
                uint8_t r = 175, g = 175, b = 170;

                // Warm wooden staircase on the left (x < 0.43, y in [0.45 .. 0.88])
                if (nx < 0.43f && ny >= 0.45f && ny <= 0.88f) {
                    if ((y / 4) % 2 == 0) {
                        r = 125; g = 77; b = 28;
                    } else {
                        r = 131; g = 73; b = 35;
                    }
                }

                const float dx_f = (static_cast<float>(x) - face_cx) / face_rx;
                const float dy_f = (static_cast<float>(y) - face_cy) / face_ry;
                const float dx_b = (static_cast<float>(x) - face_cx) / body_rx;
                const float dy_b = (static_cast<float>(y) - body_cy) / body_ry;

                if (dx_f * dx_f + dy_f * dy_f <= 1.0f) {
                    // Left side of face is lit (139,111,93), right side & forehead are shaded (91,54,38)
                    if (dx_f < -0.10f) {
                        r = 139; g = 111; b = 93;
                    } else {
                        r = 91; g = 54; b = 38;
                    }
                    // Eyes at dy_f ~ -0.21, nostrils at dy_f ~ +0.18, mustache/mouth at dy_f ~ +0.55
                    if ((std::abs(dy_f + 0.21f) < 0.10f && std::abs(dx_f) > 0.20f && std::abs(dx_f) < 0.65f) ||
                        (std::abs(dy_f - 0.18f) < 0.08f && std::abs(dx_f) < 0.28f) ||
                        (std::abs(dy_f - 0.55f) < 0.10f && std::abs(dx_f) < 0.48f)) {
                        r = 52; g = 28; b = 20;
                    }
                } else if (ny >= 0.60f && (dx_b * dx_b + dy_b * dy_b <= 1.0f)) {
                    // Mustard-yellow hoodie measured from screenshot (R=100, G=72, B=16)
                    r = 100; g = 72; b = 16;
                }

                row[x * 4 + 0] = b;
                row[x * 4 + 1] = g;
                row[x * 4 + 2] = r;
                row[x * 4 + 3] = 255;
            }
        }
    };

    FrameBuffer frame(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);

    // 1. Subject at exact screenshot position (face_cx = 0.548)
    render_wooden_room_scene(frame, 0.548f);
    if (!smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride())) {
        smartcam_engine_destroy(engine);
        return false;
    }

    float tracked_cx = 0.0f, tracked_cy = 0.0f;
    smartcam_engine_get_subject_center(engine, &tracked_cx, &tracked_cy);

    // Verify exact white-marker landmarks from user's second screenshot:
    //   - Left eye (0.489, 0.333), Right eye (0.609, 0.336), Nostrils (0.553, 0.429), Mouth (0.545, 0.521) -> MUST be 255!
    //   - Wooden stairs on left (0.22, 0.62) & upper white wall on left (0.34, 0.35) -> MUST be 0!
    const uint8_t left_eye_alpha    = mask.Data()[static_cast<size_t>(0.333f * h) * mask.Stride() + static_cast<size_t>(0.489f * w)];
    const uint8_t right_eye_alpha   = mask.Data()[static_cast<size_t>(0.336f * h) * mask.Stride() + static_cast<size_t>(0.609f * w)];
    const uint8_t nostrils_alpha    = mask.Data()[static_cast<size_t>(0.429f * h) * mask.Stride() + static_cast<size_t>(0.553f * w)];
    const uint8_t mouth_alpha       = mask.Data()[static_cast<size_t>(0.521f * h) * mask.Stride() + static_cast<size_t>(0.545f * w)];
    const uint8_t wood_stairs_alpha = mask.Data()[static_cast<size_t>(0.620f * h) * mask.Stride() + static_cast<size_t>(0.220f * w)];
    const uint8_t upper_wall_alpha  = mask.Data()[static_cast<size_t>(0.350f * h) * mask.Stride() + static_cast<size_t>(0.340f * w)];

    // 2. Subject moves to 0.66 -> verify 1:1 tracking with zero parallax lag
    render_wooden_room_scene(frame, 0.660f);
    smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride());
    float moved_cx = 0.0f, moved_cy = 0.0f;
    smartcam_engine_get_subject_center(engine, &moved_cx, &moved_cy);

    smartcam_engine_destroy(engine);

    if (std::abs(tracked_cx - 0.548f) > 0.05f || std::abs(tracked_cy - 0.385f) > 0.06f ||
        left_eye_alpha < 240 || right_eye_alpha < 240 || nostrils_alpha < 240 || mouth_alpha < 240 ||
        wood_stairs_alpha != 0 || upper_wall_alpha != 0 ||
        std::abs(moved_cx - 0.660f) > 0.05f) {
        std::fprintf(stderr,
                     "[FAIL] WoodenStairs/YellowHoodie test failed: center=(%.2f,%.2f), moved_cx=%.2f, "
                     "eyeL=%u, eyeR=%u, nose=%u, mouth=%u, wood=%u, wall=%u\n",
                     tracked_cx, tracked_cy, moved_cx,
                     left_eye_alpha, right_eye_alpha, nostrils_alpha, mouth_alpha,
                     wood_stairs_alpha, upper_wall_alpha);
        return false;
    }

    std::printf("[PASS] TestWoodenStairsYellowHoodieZeroParallax (center=(%.2f,%.2f)->%.2f, eyeL=%u, eyeR=%u, nose=%u, mouth=%u, wood=%u, wall=%u)\n",
                tracked_cx, tracked_cy, moved_cx,
                left_eye_alpha, right_eye_alpha, nostrils_alpha, mouth_alpha,
                wood_stairs_alpha, upper_wall_alpha);
    return true;
}

bool TestGuidedFaceCalibrationSilhouetteModeAndMultiPerson() {
    const uint32_t w = 256, h = 144;
    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) return false;

    smartcam_engine_set_ai_mode_and_tolerance(engine, 1, 0.50f);

    auto render_portrait_scene = [&](FrameBuffer& buf, float p1_cx_norm, bool two_persons) {
        const float p2_cx_norm = 0.72f;
        const float head_cy = 0.36f * static_cast<float>(h);
        const float head_rx = 0.095f * static_cast<float>(w);
        const float head_ry = 0.165f * static_cast<float>(h);
        const float body_cy = 0.74f * static_cast<float>(h);
        const float body_rx = 0.175f * static_cast<float>(w);
        const float body_ry = 0.250f * static_cast<float>(h);

        for (uint32_t y = 0; y < h; ++y) {
            uint8_t* row = buf.Data() + y * buf.Stride();
            for (uint32_t x = 0; x < w; ++x) {
                // Cool slate room wall background
                uint8_t r = 58, g = 72, b = 96;

                auto draw_person = [&](float cx_norm) {
                    const float pcx  = cx_norm * static_cast<float>(w);
                    const float dx_h = (static_cast<float>(x) - pcx) / head_rx;
                    const float dy_h = (static_cast<float>(y) - head_cy) / head_ry;
                    const float dx_b = (static_cast<float>(x) - pcx) / body_rx;
                    const float dy_b = (static_cast<float>(y) - body_cy) / body_ry;

                    if (dx_h * dx_h + dy_h * dy_h <= 1.0f) {
                        r = 212; g = 152; b = 122;
                        // Eyes, nose tip, and lips horizontal contrast
                        if ((std::abs(dy_h + 0.20f) < 0.10f && std::abs(dx_h) > 0.18f && std::abs(dx_h) < 0.62f) ||
                            (std::abs(dy_h - 0.15f) < 0.08f && std::abs(dx_h) < 0.25f) ||
                            (std::abs(dy_h - 0.48f) < 0.09f && std::abs(dx_h) < 0.45f)) {
                            r = 70; g = 42; b = 34;
                        }
                    } else if (y >= static_cast<uint32_t>(0.54f * h) && (dx_b * dx_b + dy_b * dy_b <= 1.0f)) {
                        r = 45; g = 110; b = 165;
                    }
                };

                draw_person(p1_cx_norm);
                if (two_persons) {
                    draw_person(p2_cx_norm);
                }

                row[x * 4 + 0] = b;
                row[x * 4 + 1] = g;
                row[x * 4 + 2] = r;
                row[x * 4 + 3] = 255;
            }
        }
    };

    FrameBuffer frame(w, h, PixelFormat::BGRA32);
    FrameBuffer mask(w, h, PixelFormat::GRAY8);

    // 1. Test 5-Point Guided Face Calibration (Left Eye, Right Eye, Nose Tip, Lips, Chin)
    render_portrait_scene(frame, 0.50f, false);
    float calib_cb = 0.0f, calib_cr = 0.0f, calib_rx = 0.0f, calib_ry = 0.0f;
    if (!smartcam_engine_calibrate_face_landmarks(
            engine, frame.Data(), w, h, frame.Stride(),
            0.465f, 0.325f,  // Left Eye
            0.535f, 0.325f,  // Right Eye
            0.500f, 0.385f,  // Nose Tip
            0.500f, 0.440f,  // Lips
            0.500f, 0.505f,  // Chin
            &calib_cb, &calib_cr, &calib_rx, &calib_ry)) {
        std::fprintf(stderr, "[FAIL] smartcam_engine_calibrate_face_landmarks returned 0\n");
        smartcam_engine_destroy(engine);
        return false;
    }

    if (calib_cb < 95.0f || calib_cb > 135.0f || calib_cr < 130.0f || calib_cr > 175.0f ||
        calib_rx < 0.065f || calib_ry < 0.11f) {
        std::fprintf(stderr, "[FAIL] Unexpected calibrated face profile: Cb=%.1f, Cr=%.1f, rx=%.3f, ry=%.3f\n",
                     calib_cb, calib_cr, calib_rx, calib_ry);
        smartcam_engine_destroy(engine);
        return false;
    }

    // 1b. Verify Soluzione 3 (5-Point Visual Template Tracking):
    //     Even if reddish-brown wooden stairs are immediately to the left of the face (x in [0.18..0.37])
    //     and the right side of the face is in shadow, the calibrated 5-point template must stay centered at x ~ 0.50!
    const float lm10[10] = {
        0.535f, 0.325f,  // Right Eye
        0.465f, 0.325f,  // Left Eye
        0.500f, 0.385f,  // Nose Tip
        0.500f, 0.440f,  // Lips
        0.500f, 0.505f   // Chin
    };
    smartcam_engine_set_face_calibration_landmarks(engine, lm10);
    for (int f = 0; f < 5; ++f) {
        smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride());
    }
    float cal_tracked_cx = 0.0f, cal_tracked_cy = 0.0f;
    smartcam_engine_get_subject_center(engine, &cal_tracked_cx, &cal_tracked_cy);
    if (std::abs(cal_tracked_cx - 0.50f) > 0.035f) {
        std::fprintf(stderr, "[FAIL] Calibrated 5-point template drifted off center: cx=%.3f (expected ~0.500)\n",
                     cal_tracked_cx);
        smartcam_engine_destroy(engine);
        return false;
    }

    // 2. Test Silhouette Mode 0 (Volto + Busto) vs Silhouette Mode 1 (Solo Volto)
    smartcam_engine_set_silhouette_mode(engine, 0);
    if (smartcam_engine_get_silhouette_mode(engine) != 0) {
        smartcam_engine_destroy(engine);
        return false;
    }
    smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride());
    const uint8_t head_mode0  = mask.Data()[static_cast<size_t>(0.36f * h) * mask.Stride() + static_cast<size_t>(0.50f * w)];
    const uint8_t chest_mode0 = mask.Data()[static_cast<size_t>(0.74f * h) * mask.Stride() + static_cast<size_t>(0.50f * w)];

    smartcam_engine_set_silhouette_mode(engine, 1);
    if (smartcam_engine_get_silhouette_mode(engine) != 1) {
        smartcam_engine_destroy(engine);
        return false;
    }
    smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride());
    const uint8_t head_mode1  = mask.Data()[static_cast<size_t>(0.36f * h) * mask.Stride() + static_cast<size_t>(0.50f * w)];
    const uint8_t chest_mode1 = mask.Data()[static_cast<size_t>(0.74f * h) * mask.Stride() + static_cast<size_t>(0.50f * w)];

    if (head_mode0 <= 200 || chest_mode0 <= 200 || head_mode1 <= 200 || chest_mode1 != 0) {
        std::fprintf(stderr,
                     "[FAIL] Silhouette mode check failed: Mode0(head=%u, chest=%u), Mode1(head=%u, chest=%u)\n",
                     head_mode0, chest_mode0, head_mode1, chest_mode1);
        smartcam_engine_destroy(engine);
        return false;
    }

    // 3. Test Multi-Person Tracking (2 people at x=0.28 and x=0.72)
    smartcam_engine_set_face_calibration_profile(engine, 0, 113.0f, 148.0f, 0.105f, 0.175f);
    smartcam_engine_set_silhouette_mode(engine, 0);
    smartcam_engine_set_multi_person_mode(engine, 1);

    render_portrait_scene(frame, 0.28f, true);
    smartcam_engine_compute_ai_mask(engine, frame.Data(), w, h, frame.Stride(), mask.Data(), mask.Stride());

    const uint8_t p1_head_alpha   = mask.Data()[static_cast<size_t>(0.36f * h) * mask.Stride() + static_cast<size_t>(0.28f * w)];
    const uint8_t p2_head_alpha   = mask.Data()[static_cast<size_t>(0.36f * h) * mask.Stride() + static_cast<size_t>(0.72f * w)];
    const uint8_t center_bg_alpha = mask.Data()[static_cast<size_t>(0.22f * h) * mask.Stride() + static_cast<size_t>(0.50f * w)];
    const uint8_t outer_bg_alpha  = mask.Data()[static_cast<size_t>(0.36f * h) * mask.Stride() + static_cast<size_t>(0.04f * w)];

    float bench_wall_ms = 0.0f, bench_cpu_ms = 0.0f;
    const int bench_ok = smartcam_engine_benchmark_profile(
        engine, 1, 3, 1, 320, 180, 2, &bench_wall_ms, &bench_cpu_ms);

    smartcam_engine_destroy(engine);

    if (p1_head_alpha <= 200 || p2_head_alpha <= 200 || center_bg_alpha >= 15 || outer_bg_alpha >= 15 ||
        !bench_ok || bench_wall_ms <= 0.0f) {
        std::fprintf(stderr,
                     "[FAIL] Multi-person or benchmark failed: p1=%u, p2=%u, center_bg=%u, outer_bg=%u, bench=%d (%.2f ms)\n",
                     p1_head_alpha, p2_head_alpha, center_bg_alpha, outer_bg_alpha, bench_ok, bench_wall_ms);
        return false;
    }

    std::printf("[PASS] TestGuidedFaceCalibrationSilhouetteModeAndMultiPerson (Cb=%.1f, Cr=%.1f, Mode0_chest=%u, Mode1_chest=%u, MultiP=[%u,%u], Bench=%.2fms)\n",
                calib_cb, calib_cr, chest_mode0, chest_mode1, p1_head_alpha, p2_head_alpha, bench_wall_ms);
    return true;
}

bool Test64BitDeepColorSourceConversion() {
    const uint32_t w = 64, h = 32;
    FrameBuffer fb32(w, h, PixelFormat::BGRA32);
    FrameBuffer fb64(w, h, PixelFormat::BGRA64);
    FrameBuffer fb32_roundtrip(w, h, PixelFormat::BGRA32);

    if (!fb32.IsValid() || !fb64.IsValid() || !fb32_roundtrip.IsValid() ||
        GetBytesPerPixel(PixelFormat::BGRA64) != 8 ||
        GetBytesPerPixel(PixelFormat::RGBA64) != 8 ||
        fb64.Stride() != w * 8) {
        std::fprintf(stderr, "[FAIL] 64-bit BGRA64 FrameBuffer allocation failed\n");
        return false;
    }

    for (uint32_t i = 0; i < w * h * 4; ++i) {
        fb32.Data()[i] = static_cast<uint8_t>((i * 37 + 13) & 0xFF);
    }

    ConvertBgra32ToBgra64(
        fb32.Data(),
        fb32.Stride(),
        reinterpret_cast<uint16_t*>(fb64.Data()),
        fb64.Stride(),
        w,
        h);

    ConvertBgra64ToBgra32(
        reinterpret_cast<const uint16_t*>(fb64.Data()),
        fb64.Stride(),
        fb32_roundtrip.Data(),
        fb32_roundtrip.Stride(),
        w,
        h);

    for (uint32_t i = 0; i < w * h * 4; ++i) {
        if (fb32.Data()[i] != fb32_roundtrip.Data()[i]) {
            std::fprintf(stderr, "[FAIL] BGRA32 <-> BGRA64 roundtrip mismatch at %u: %u vs %u\n",
                         i, fb32.Data()[i], fb32_roundtrip.Data()[i]);
            return false;
        }
    }

    std::printf("[PASS] Test64BitDeepColorSourceConversion (BGRA64 8-BPP / 16-bit per channel lossless roundtrip)\n");
    return true;
}

}  // namespace

int main() {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    std::setvbuf(stderr, nullptr, _IONBF, 0);
    std::printf("=== SmartCam Common Engine Unit Tests (PEFL v1.0) ===\n");
    bool ok = true;
    ok &= TestFrameBufferSimdAlignment();
    ok &= TestCenterCrop16x9To4x3();
    ok &= TestGreenScreenHsvSegmentation();
    ok &= TestGreenScreenAutoCalibration();
    ok &= TestGreenSpillSuppressionPixelFormats();
    ok &= TestCApiContextIsolationAndHelpers();
    ok &= TestSimdCompositorEquivalence();
    ok &= TestAiSegmenterPostProcess();
    ok &= TestDynamicTrackingAndReferenceBackground();
    ok &= TestAntiLedTearingAndSpatialTolerance();
    ok &= TestSolidSilhouetteWithColoredFurniture();
    ok &= TestWoodenStairsYellowHoodieZeroParallax();
    ok &= TestGuidedFaceCalibrationSilhouetteModeAndMultiPerson();
    ok &= Test64BitDeepColorSourceConversion();
    ok &= TestGpuProbeAndHardwarePolicy();
    ok &= TestPhoneCameraPluginAndCatalogue();

    if (!ok) {
        std::fprintf(stderr, "One or more SmartCam unit tests FAILED.\n");
        return EXIT_FAILURE;
    }
    std::printf("=== ALL SMARTCAM COMMON ENGINE TESTS PASSED ===\n");
    return EXIT_SUCCESS;
}
