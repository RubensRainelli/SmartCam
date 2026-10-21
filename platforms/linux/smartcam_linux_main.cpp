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

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

#include "../../common/engine/c_api.h"
#include "linux_bridge_c_api.h"

namespace {

void PrintHelp() {
    std::printf("SmartCam Linux v1.0.0 — Native V4L2 & v4l2loopback Virtual Camera (PEFL v1.0)\n");
    std::printf("Copyright (c) 2026 Rubens Rainelli — https://github.com/RubensRainelli/\n\n");
    std::printf("Usage:\n");
    std::printf("  SmartCam                      Start live V4L2 -> AI/SIMD -> v4l2loopback pipeline\n");
    std::printf("  SmartCam --status             Show V4L2 cameras (with IR detection), GPU backends & SIMD\n");
    std::printf("  SmartCam --benchmark          Run real-time CPU (AVX-512/AVX2) & GPU benchmark\n");
    std::printf("  SmartCam --frames <N>         Process N frames and exit\n");
    std::printf("  SmartCam --mode <blur|ai|gs|passthrough>\n");
    std::printf("  SmartCam --aspect-ratio <4:3|16:9|1:1|9:16>\n");
    std::printf("  SmartCam --accept-pefl        Accept PEFL v1.0 license non-interactively\n");
}

}  // namespace

int main(int argc, char** argv) {
    bool status_only   = false;
    bool run_benchmark = false;
    uint64_t max_frames = 0;
    std::string mode   = "blur";
    uint32_t aspect_w  = 4;
    uint32_t aspect_h  = 3;
    uint32_t out_w     = 1440;
    uint32_t out_h     = 1080;
    uint32_t fps       = 30;

    for (int i = 1; i < argc; ++i) {
        const std::string arg = argv[i];
        if (arg == "-h" || arg == "--help") {
            PrintHelp();
            return 0;
        } else if (arg == "--status") {
            status_only = true;
        } else if (arg == "--benchmark") {
            run_benchmark = true;
        } else if (arg == "--frames" && i + 1 < argc) {
            max_frames = static_cast<uint64_t>(std::strtoull(argv[++i], nullptr, 10));
        } else if ((arg == "--mode" || arg == "--set-mode") && i + 1 < argc) {
            mode = argv[++i];
        } else if ((arg == "--aspect-ratio" || arg == "--set-aspect") && i + 1 < argc) {
            const std::string ar = argv[++i];
            if (ar == "16:9") {
                aspect_w = 16; aspect_h = 9; out_w = 1920; out_h = 1080;
            } else if (ar == "1:1") {
                aspect_w = 1; aspect_h = 1; out_w = 1080; out_h = 1080;
            } else if (ar == "9:16") {
                aspect_w = 9; aspect_h = 16; out_w = 608; out_h = 1080;
            } else {
                aspect_w = 4; aspect_h = 3; out_w = 1440; out_h = 1080;
            }
        }
    }

    SmartCamEngineHandle engine = smartcam_engine_create(0);
    if (!engine) {
        std::fprintf(stderr, "[SmartCam Linux] Failed to initialize SmartCam C++17 SIMD Engine.\n");
        return 1;
    }

    const char* simd_backend = smartcam_engine_get_simd_backend(engine);
    const uint32_t cam_count = smartcam_linux_get_camera_count();
    const int gpu_count      = smartcam_engine_get_compute_device_count(engine);

    std::printf("============================================================\n");
    std::printf("  SmartCam Linux v1.0.0 (PEFL v1.0 — Rubens Rainelli)\n");
    std::printf("============================================================\n");
    std::printf("  * Active SIMD Backend : %s\n", simd_backend ? simd_backend : "AVX2");
    std::printf("  * Compute Devices     : %d detected\n", gpu_count);
    for (int g = 0; g < gpu_count; ++g) {
        uint32_t b_id = 0;
        char b_name[64]{};
        char d_name[160]{};
        uint64_t vram_mb = 0;
        int avail = 0;
        int is_igpu = 0;
        if (smartcam_engine_get_compute_device_info(
                engine, g, &b_id, b_name, sizeof(b_name), d_name, sizeof(d_name), &vram_mb, &avail, &is_igpu) != 0) {
            std::printf("      [%d] %-8s : %s (%s)\n", g, b_name, d_name, avail ? "Available" : "Not present");
        }
    }

    std::printf("  * V4L2 Cameras        : %u detected\n", cam_count);
    uint32_t selected_cam = 0;
    bool found_rgb = false;
    for (uint32_t idx = 0; idx < cam_count; ++idx) {
        char name_buf[256]{};
        char sym_buf[256]{};
        int is_ir = 0;
        if (smartcam_linux_get_camera_info(idx, name_buf, sizeof(name_buf), sym_buf, sizeof(sym_buf), &is_ir) != 0) {
            std::printf("      [%u] %s (%s) %s\n", idx, name_buf, sym_buf, is_ir ? "[IR Sensor]" : "[RGB]");
            if (!found_rgb && !is_ir) {
                selected_cam = idx;
                found_rgb = true;
            }
        }
    }
    std::printf("============================================================\n");

    if (run_benchmark) {
        std::printf("[SmartCam Linux] Running real-time 1440x1080 pipeline benchmark (12 iterations)...\n");
        for (uint32_t sm = 1; sm <= 3; ++sm) {
            float avg_ms = 0.0f, cpu_ms = 0.0f;
            int supported = smartcam_engine_benchmark_profile(engine, 1, sm, 1, 1440, 1080, 12, &avg_ms, &cpu_ms);
            const char* sm_name = (sm == 1) ? "CPU AVX-512 + AI" : (sm == 2) ? "CPU AVX2 + AI   " : "CPU Scalar + AI ";
            if (supported && avg_ms > 0.01f) {
                const float est_fps = 1000.0f / avg_ms;
                const uint32_t score = static_cast<uint32_t>(est_fps * 35.0f);
                std::printf("  * %s : %6.2f ms/frame | %6.1f FPS | CPU %5.2f ms | Score: %u pt\n",
                            sm_name, avg_ms, est_fps, cpu_ms, score);
            } else {
                std::printf("  * %s : Non supportato su questa CPU\n", sm_name);
            }
        }
        smartcam_engine_destroy(engine);
        return 0;
    }

    if (status_only) {
        smartcam_engine_destroy(engine);
        return 0;
    }

    SmartCamLinuxCameraHandle cam = smartcam_linux_camera_create();
    bool cam_opened = false;
    if (cam && cam_count > 0) {
        cam_opened = (smartcam_linux_camera_open(cam, selected_cam, 1920, 1080, fps) != 0);
    }

    SmartCamLinuxSinkHandle sink = smartcam_linux_sink_create();
    bool sink_opened = false;
    if (sink) {
        sink_opened = (smartcam_linux_sink_init(sink, "SmartCam", out_w, out_h, fps) != 0);
    }

    std::printf("[SmartCam Linux] Pipeline active: %ux%u @ %u FPS (V4L2 Input=%s, v4l2loopback Sink=%s)\n",
                out_w, out_h, fps, cam_opened ? "OPEN" : "SYNTHETIC", sink_opened ? "OPEN" : "STANDBY");

    std::vector<uint8_t> raw_buf(1920 * 1080 * 4, 40);
    std::vector<uint8_t> framed_buf(out_w * out_h * 4, 0);
    std::vector<uint8_t> bg_buf(out_w * out_h * 4, 0);
    std::vector<uint8_t> mask_buf(out_w * out_h, 255);
    std::vector<uint8_t> out_buf(out_w * out_h * 4, 0);

    const auto frame_interval = std::chrono::milliseconds(1000 / fps);
    uint64_t frames = 0;

    while (true) {
        const auto t0 = std::chrono::steady_clock::now();
        uint32_t rw = 1920;
        uint32_t rh = 1080;

        if (!cam_opened || smartcam_linux_camera_read_frame(
                               cam, raw_buf.data(), static_cast<uint32_t>(raw_buf.size()), &rw, &rh) == 0) {
            rw = out_w;
            rh = out_h;
        }

        smartcam_engine_crop_scale_bgra(
            raw_buf.data(), rw, rh, rw * 4,
            framed_buf.data(), out_w, out_h, out_w * 4,
            aspect_w, aspect_h, 0);

        if (mode == "passthrough") {
            std::memcpy(out_buf.data(), framed_buf.data(), out_buf.size());
        } else {
            smartcam_engine_generate_blurred_bg(framed_buf.data(), bg_buf.data(), out_w, out_h, out_w * 4, 14);
            if (mode == "gs" || mode == "green_screen") {
                smartcam_engine_compute_greenscreen_mask(
                    engine, framed_buf.data(), out_w, out_h, out_w * 4, mask_buf.data(), out_w);
            } else {
                smartcam_engine_compute_ai_mask(
                    engine, framed_buf.data(), out_w, out_h, out_w * 4, mask_buf.data(), out_w);
            }
            smartcam_engine_blend_bgra(
                engine, framed_buf.data(), bg_buf.data(), mask_buf.data(), out_buf.data(),
                out_w, out_h, out_w * 4, out_w);
        }

        if (sink_opened) {
            smartcam_linux_sink_push_frame(sink, out_buf.data(), out_w, out_h, out_w * 4);
        }

        ++frames;
        if (max_frames > 0 && frames >= max_frames) {
            break;
        }

        const auto elapsed = std::chrono::steady_clock::now() - t0;
        if (max_frames == 0 && elapsed < frame_interval) {
            std::this_thread::sleep_for(frame_interval - elapsed);
        }
    }

    if (cam) {
        smartcam_linux_camera_close(cam);
        smartcam_linux_camera_destroy(cam);
    }
    if (sink) {
        smartcam_linux_sink_destroy(sink);
    }
    smartcam_engine_destroy(engine);
    std::printf("[SmartCam Linux] Processed %llu frames cleanly.\n", static_cast<unsigned long long>(frames));
    return 0;
}
