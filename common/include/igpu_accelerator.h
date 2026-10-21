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

#ifndef SMARTCAM_COMMON_IGPU_ACCELERATOR_H_
#define SMARTCAM_COMMON_IGPU_ACCELERATOR_H_

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>
#include "frame_buffer.h"

namespace smartcam {
namespace common {

/**
 * @brief Multi-vendor compute backend selector for video pipeline & AI acceleration.
 */
enum class ComputeBackend : uint32_t {
    AUTO   = 0,  ///< Automatically select highest-scoring hardware backend
    CPU    = 1,  ///< Multi-threaded CPU + SIMD (AVX-512 / AVX2 / Scalar)
    VULKAN = 2,  ///< Cross-vendor Vulkan 1.x Compute API (Intel, AMD, NVIDIA)
    OPENCL = 3,  ///< Cross-vendor OpenCL 1.2/2.0/3.0 GPU/iGPU Compute
    CUDA   = 4,  ///< NVIDIA CUDA Driver API (GeForce / RTX / Quadro)
    ROCM   = 5   ///< AMD ROCm / HIP Runtime API (Radeon RX / Instinct / APU)
};

/**
 * @brief Returns canonical short name for a ComputeBackend value.
 */
inline constexpr const char* ComputeBackendToString(ComputeBackend backend) noexcept {
    switch (backend) {
        case ComputeBackend::AUTO:   return "Auto";
        case ComputeBackend::CPU:    return "CPU";
        case ComputeBackend::VULKAN: return "Vulkan";
        case ComputeBackend::OPENCL: return "OpenCL";
        case ComputeBackend::CUDA:   return "CUDA";
        case ComputeBackend::ROCM:   return "ROCm";
        default:                     return "Unknown";
    }
}

/**
 * @brief Hardware capabilities and driver diagnostics for a discovered compute backend.
 * POD-compatible structure suitable for C/Rust FFI boundaries.
 */
struct ComputeDeviceInfo {
    ComputeBackend backend;             ///< Backend identifier enum
    char           backend_name[32];    ///< "CPU", "Vulkan", "OpenCL", "CUDA", "ROCm"
    char           device_name[128];    ///< Hardware model string (e.g., "Intel(R) Iris(R) Xe Graphics")
    char           driver_version[64];  ///< Driver or runtime API version string
    uint64_t       vram_bytes;          ///< Dedicated or shared video memory in bytes
    uint32_t       compute_units;       ///< Execution units / SMs / CUs / logical cores
    bool           available;           ///< True if runtime driver loaded and a valid device responded
    bool           is_integrated_gpu;   ///< True if iGPU / unified host memory architecture
};

/**
 * @brief Real-time micro-benchmark result for a given compute backend and workload profile.
 * POD-compatible structure suitable for C/Rust FFI boundaries.
 */
struct BackendBenchmarkResult {
    ComputeBackend backend;                     ///< Tested compute backend
    uint32_t       simd_mode;                   ///< 0=Auto, 1=AVX512, 2=AVX2, 3=Scalar
    bool           ai_enabled;                  ///< True if benchmarked with AI segmentation workload included
    float          avg_frame_ms;                ///< Measured average frame latency in milliseconds
    float          estimated_fps;               ///< Estimated sustainable frame rate (FPS)
    float          estimated_cpu_load_percent;  ///< Estimated host CPU utilization percentage [0..100]
    uint32_t       score;                       ///< Synthetic performance & efficiency score [0..10000]
    bool           supported;                   ///< True if this backend + SIMD mode is supported on current machine
};

/**
 * @brief Platform-agnostic interface for Multi-Vendor GPU & CPU Compute Acceleration.
 *
 * Strictly free of OS-specific headers. Implementations dynamically probe installed
 * GPU drivers (Vulkan, OpenCL, NVIDIA CUDA, AMD ROCm/HIP) at runtime without requiring
 * compile-time proprietary SDK libraries.
 */
class IGpuAccelerator {
public:
    virtual ~IGpuAccelerator() = default;

    /**
     * @brief Dynamically probes all compute backends (CPU, Vulkan, OpenCL, CUDA, ROCm)
     *        present on the host system and returns their hardware descriptors.
     */
    virtual std::vector<ComputeDeviceInfo> ProbeAvailableBackends() = 0;

    /**
     * @brief Selects the active compute backend for subsequent frame operations.
     *        If `ComputeBackend::AUTO` is passed, selects the optimal available backend.
     * @return true if the backend is available and activated (or fell back cleanly on AUTO).
     */
    virtual bool SetActiveBackend(ComputeBackend backend) = 0;

    /**
     * @brief Returns the currently active compute backend.
     */
    virtual ComputeBackend GetActiveBackend() const noexcept = 0;

    /**
     * @brief Returns human-readable name of the active compute backend ("CPU", "Vulkan", "OpenCL", "CUDA", "ROCm").
     */
    virtual const char* GetActiveBackendName() const noexcept = 0;

    /**
     * @brief Returns human-readable hardware device name of the active backend.
     */
    virtual const char* GetActiveDeviceName() const noexcept = 0;

    /**
     * @brief Accelerated separable Bokeh box blur on a `BGRA32`/`RGBA32` frame.
     */
    virtual bool AccelerateBlurBgra(const FrameView& src,
                                    FrameView& dst_bg,
                                    uint32_t radius) = 0;

    /**
     * @brief Convenience raw-pointer overload for `AccelerateBlurBgra`.
     */
    bool AccelerateBlurBgra(const uint8_t* src_bgra,
                            uint8_t* dst_bgra,
                            uint32_t width,
                            uint32_t height,
                            uint32_t stride,
                            uint32_t radius) {
        if (!src_bgra || !dst_bgra) return false;
        FrameView src{const_cast<uint8_t*>(src_bgra), width, height, stride, PixelFormat::BGRA32, 0, 0};
        FrameView dst{dst_bgra, width, height, stride, PixelFormat::BGRA32, 0, 0};
        return AccelerateBlurBgra(src, dst, radius);
    }

    /**
     * @brief Accelerated per-pixel alpha blending of `foreground` and `background` using `alpha_mask` (`GRAY8`).
     */
    virtual bool AccelerateBlendBgra(const FrameView& foreground,
                                     const FrameView& background,
                                     const FrameView& alpha_mask,
                                     FrameView& output) = 0;

    /**
     * @brief Convenience raw-pointer overload for `AccelerateBlendBgra`.
     */
    bool AccelerateBlendBgra(const uint8_t* fg_bgra,
                             const uint8_t* bg_bgra,
                             const uint8_t* mask_gray8,
                             uint8_t* out_bgra,
                             uint32_t width,
                             uint32_t height,
                             uint32_t rgba_stride,
                             uint32_t mask_stride) {
        if (!fg_bgra || !bg_bgra || !mask_gray8 || !out_bgra) return false;
        FrameView fg{const_cast<uint8_t*>(fg_bgra), width, height, rgba_stride, PixelFormat::BGRA32, 0, 0};
        FrameView bg{const_cast<uint8_t*>(bg_bgra), width, height, rgba_stride, PixelFormat::BGRA32, 0, 0};
        FrameView mask{const_cast<uint8_t*>(mask_gray8), width, height, mask_stride, PixelFormat::GRAY8, 0, 0};
        FrameView out{out_bgra, width, height, rgba_stride, PixelFormat::BGRA32, 0, 0};
        return AccelerateBlendBgra(fg, bg, mask, out);
    }

    /**
     * @brief Accelerated AI probability map post-processing (EMA temporal smoothing + bilinear upsample to `GRAY8`).
     */
    virtual bool AccelerateAiPostProcess(const float* prob_map,
                                         uint32_t map_width,
                                         uint32_t map_height,
                                         FrameView& out_mask,
                                         float temporal_smooth = 0.25f) = 0;

    /**
     * @brief Convenience raw-pointer overload for `AccelerateAiPostProcess`.
     */
    bool AccelerateAiPostProcess(const float* prob_map,
                                 uint32_t map_width,
                                 uint32_t map_height,
                                 uint8_t* out_mask_gray8,
                                 uint32_t out_width,
                                 uint32_t out_height,
                                 uint32_t out_stride,
                                 float temporal_smooth = 0.25f) {
        if (!prob_map || !out_mask_gray8) return false;
        FrameView mask{out_mask_gray8, out_width, out_height, out_stride, PixelFormat::GRAY8, 0, 0};
        return AccelerateAiPostProcess(prob_map, map_width, map_height, mask, temporal_smooth);
    }

    /**
     * @brief Executes a real hardware micro-benchmark across all probed backends and SIMD tiers
     *        (both with and without AI workload), returning measured latency, FPS, CPU load, and score.
     */
    virtual std::vector<BackendBenchmarkResult> RunMicroBenchmark(uint32_t width = 1280,
                                                                  uint32_t height = 720,
                                                                  uint32_t iterations = 10) = 0;
};

namespace engine {
using ComputeBackend         = ::smartcam::common::ComputeBackend;
using ComputeDeviceInfo      = ::smartcam::common::ComputeDeviceInfo;
using BackendBenchmarkResult = ::smartcam::common::BackendBenchmarkResult;
using IGpuAccelerator        = ::smartcam::common::IGpuAccelerator;
using ::smartcam::common::ComputeBackendToString;
}  // namespace engine

}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_IGPU_ACCELERATOR_H_
