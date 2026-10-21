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

#ifndef SMARTCAM_COMMON_ENGINE_GPU_ACCELERATOR_H_
#define SMARTCAM_COMMON_ENGINE_GPU_ACCELERATOR_H_

#include <cstdint>
#include <memory>
#include <mutex>
#include <vector>
#include "../../include/frame_buffer.h"
#include "../../include/igpu_accelerator.h"
#include "../compositor/compositor.h"

namespace smartcam {
namespace common {
namespace engine {

/**
 * @brief Multi-Vendor Runtime GPU & CPU Compute Engine (`Vulkan`, `OpenCL`, `CUDA`, `ROCm`, `CPU SIMD`).
 *
 * Zero compile-time SDK dependencies: dynamically loads installed vendor runtime drivers
 * (`vulkan-1.dll`/`libvulkan.so.1`, `OpenCL.dll`/`libOpenCL.so.1`, `nvcuda.dll`/`libcuda.so.1`,
 * `amdhip64.dll`/`libamdhip64.so`) at runtime via OS dynamic linker APIs.
 *
 * Supports:
 *   - Hardware device discovery (model name, driver version, VRAM, compute units, iGPU detection)
 *   - Runtime-compiled OpenCL C GPU kernels for Alpha Blending, Separable Box Blur, and AI Mask Upsampling
 *   - Vulkan compute device/queue probing & staging memory transfer pipelines
 *   - NVIDIA CUDA Driver API context, device memory & PTX execution / high-speed DMA pipeline
 *   - AMD ROCm / HIP Runtime API device management & memory transfer pipeline
 *   - Multi-tier CPU SIMD fallback (`AVX-512`, `AVX2`, `Scalar`)
 *   - Real-time hardware micro-benchmarking (`RunMicroBenchmark`) with synthetic scoring `[0..10000]`
 */
class GpuAccelerator final : public IGpuAccelerator {
public:
    using IGpuAccelerator::AccelerateBlurBgra;
    using IGpuAccelerator::AccelerateBlendBgra;
    using IGpuAccelerator::AccelerateAiPostProcess;

    explicit GpuAccelerator(ComputeBackend initial_backend = ComputeBackend::AUTO,
                            SimdTier initial_simd = SimdTier::AUTO);
    ~GpuAccelerator() override;

    GpuAccelerator(const GpuAccelerator&) = delete;
    GpuAccelerator& operator=(const GpuAccelerator&) = delete;

    /**
     * @brief Probes all 5 compute backends (CPU, Vulkan, OpenCL, CUDA, ROCm) on the machine.
     */
    std::vector<ComputeDeviceInfo> ProbeAvailableBackends() override;

    /**
     * @brief Returns cached list of probed compute backend descriptors.
     */
    const std::vector<ComputeDeviceInfo>& GetCachedDevices() const noexcept;

    /**
     * @brief Returns number of probed backend entries (typically 5: CPU, Vulkan, OpenCL, CUDA, ROCm).
     */
    uint32_t GetDeviceCount() const noexcept;

    /**
     * @brief Copies the device descriptor at `index` into `out_info`.
     */
    bool GetDeviceInfoByIndex(uint32_t index, ComputeDeviceInfo* out_info) const noexcept;

    /**
     * @brief Copies the device descriptor for a specific `backend` into `out_info`.
     */
    bool GetDeviceInfoByBackend(ComputeBackend backend, ComputeDeviceInfo* out_info) const noexcept;

    /**
     * @brief Returns true if `backend` was detected as available on this host.
     */
    bool IsBackendAvailable(ComputeBackend backend) const noexcept;

    /**
     * @brief Sets the active compute backend (`AUTO` selects best available hardware backend).
     */
    bool SetActiveBackend(ComputeBackend backend) override;

    /**
     * @brief Returns the resolved active compute backend (`CPU`, `VULKAN`, `OPENCL`, `CUDA`, or `ROCM`).
     */
    ComputeBackend GetActiveBackend() const noexcept override;

    /**
     * @brief Returns the user-requested backend (which may be `ComputeBackend::AUTO`).
     */
    ComputeBackend GetRequestedBackend() const noexcept;

    /**
     * @brief Returns canonical name of the active compute backend ("CPU", "Vulkan", "OpenCL", "CUDA", "ROCm").
     */
    const char* GetActiveBackendName() const noexcept override;

    /**
     * @brief Returns hardware device model string of the active compute backend.
     */
    const char* GetActiveDeviceName() const noexcept override;

    /**
     * @brief Overrides the CPU SIMD tier used by the CPU backend and hybrid fallbacks.
     */
    void SetSimdTier(SimdTier tier) noexcept;

    /**
     * @brief Returns the currently active CPU SIMD tier.
     */
    SimdTier GetSimdTier() const noexcept;

    /**
     * @brief Returns human-readable name of active CPU SIMD tier ("AVX-512", "AVX2", or "Scalar").
     */
    const char* GetActiveSimdName() const noexcept;

    /**
     * @brief Accelerated separable Bokeh box blur on `BGRA32`/`RGBA32` frames.
     */
    bool AccelerateBlurBgra(const FrameView& src,
                            FrameView& dst_bg,
                            uint32_t radius) override;

    /**
     * @brief Accelerated per-pixel alpha blending of `foreground` and `background` using `alpha_mask` (`GRAY8`).
     */
    bool AccelerateBlendBgra(const FrameView& foreground,
                             const FrameView& background,
                             const FrameView& alpha_mask,
                             FrameView& output) override;

    /**
     * @brief Accelerated AI probability map post-processing (EMA temporal smoothing + bilinear upsampling).
     */
    bool AccelerateAiPostProcess(const float* prob_map,
                                 uint32_t map_width,
                                 uint32_t map_height,
                                 FrameView& out_mask,
                                 float temporal_smooth = 0.25f) override;

    /**
     * @brief Runs real hardware micro-benchmarks across all backends & SIMD tiers (with and without AI).
     */
    std::vector<BackendBenchmarkResult> RunMicroBenchmark(uint32_t width = 1280,
                                                          uint32_t height = 720,
                                                          uint32_t iterations = 10) override;

    /**
     * @brief C-compatible wrapper for `RunMicroBenchmark` writing up to `max_results` entries.
     * @return Number of benchmark entries written to `out_results`.
     */
    uint32_t RunMicroBenchmarkC(uint32_t width,
                                uint32_t height,
                                uint32_t iterations,
                                BackendBenchmarkResult* out_results,
                                uint32_t max_results);

    /**
     * @brief Selects the highest-scoring benchmark profile for the specified workload.
     */
    BackendBenchmarkResult GetRecommendedProfile(uint32_t width = 1280,
                                                 uint32_t height = 720,
                                                 bool ai_enabled = true);

private:
    struct RuntimeImpl;
    std::unique_ptr<RuntimeImpl> impl_;
};

}  // namespace engine

using GpuAccelerator = ::smartcam::common::engine::GpuAccelerator;

}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_ENGINE_GPU_ACCELERATOR_H_
