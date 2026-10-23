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

#ifndef SMARTCAM_COMMON_FRAME_BUFFER_H_
#define SMARTCAM_COMMON_FRAME_BUFFER_H_

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>

namespace smartcam {
namespace common {

/**
 * @brief Supported pixel formats across the platform-agnostic SmartCam pipeline.
 * All SIMD compositors operate preferentially on RGBA32 or BGRA32 64-byte aligned buffers.
 */
enum class PixelFormat : uint32_t {
    UNKNOWN = 0,
    RGB24   = 1,  ///< 3 bytes per pixel (24-bit): R, G, B
    BGR24   = 2,  ///< 3 bytes per pixel (24-bit): B, G, R
    RGBA32  = 3,  ///< 4 bytes per pixel (32-bit, 8-bit/ch): R, G, B, A
    BGRA32  = 4,  ///< 4 bytes per pixel (32-bit, 8-bit/ch): B, G, R, A (Media Foundation RGB32/ARGB32)
    NV12    = 5,  ///< Y plane followed by interleaved UV plane (4:2:0)
    YUY2    = 6,  ///< Packed YUYV 4:2:2
    GRAY8   = 7,  ///< Single-channel 8-bit alpha/segmentation mask [0..255]
    RGBA64  = 8,  ///< 8 bytes per pixel (64-bit Deep Color, 16-bit/ch): R16, G16, B16, A16
    BGRA64  = 9   ///< 8 bytes per pixel (64-bit Deep Color, 16-bit/ch): B16, G16, R16, A16 (Media Foundation ARGB64)
};

/**
 * @brief Target aspect ratio mode for output framing.
 */
enum class AspectRatioMode : uint32_t {
    NATIVE      = 0,  ///< Keep original input dimensions
    RATIO_16_9  = 1,  ///< Force 16:9 output aspect ratio
    RATIO_4_3   = 2,  ///< Force 4:3 output aspect ratio
    RATIO_1_1   = 3,  ///< Force 1:1 square aspect ratio
    RATIO_9_16  = 4,  ///< Force 9:16 vertical aspect ratio
    CUSTOM      = 5   ///< Custom user-specified width:height ratio
};

/**
 * @brief Scaling / framing strategy when transforming between different aspect ratios.
 */
enum class CropStrategy : uint32_t {
    CENTER_CROP = 0,  ///< Distortion-free center crop + bilinear/bicubic resize
    LETTERBOX   = 1   ///< Preserve full frame with black bars (letterbox / pillarbox)
};

/**
 * @brief Returns the bytes per pixel for packed formats (or 0 for planar formats like NV12).
 */
inline constexpr uint32_t GetBytesPerPixel(PixelFormat fmt) noexcept {
    switch (fmt) {
        case PixelFormat::RGBA64:
        case PixelFormat::BGRA64:
            return 8;
        case PixelFormat::RGB24:
        case PixelFormat::BGR24:
            return 3;
        case PixelFormat::RGBA32:
        case PixelFormat::BGRA32:
            return 4;
        case PixelFormat::YUY2:
            return 2;
        case PixelFormat::GRAY8:
            return 1;
        default:
            return 0;
    }
}

/**
 * @brief Converts a 32-bit `BGRA32` (8-bit per channel) buffer to 64-bit `BGRA64` (16-bit per channel Deep Color).
 * Expands each 8-bit sample `v` to 16-bit `(v << 8) | v` (`0..65535`) for zero-quantization-loss studio processing.
 */
inline void ConvertBgra32ToBgra64(
    const uint8_t* src_bgra32,
    uint32_t       src_stride,
    uint16_t*      dst_bgra64,
    uint32_t       dst_stride_bytes,
    uint32_t       width,
    uint32_t       height) noexcept {
    if (!src_bgra32 || !dst_bgra64 || width == 0 || height == 0) {
        return;
    }
    for (uint32_t y = 0; y < height; ++y) {
        const uint8_t* srow = src_bgra32 + static_cast<size_t>(y) * src_stride;
        uint16_t* drow = reinterpret_cast<uint16_t*>(
            reinterpret_cast<uint8_t*>(dst_bgra64) + static_cast<size_t>(y) * dst_stride_bytes);
        const uint32_t total_channels = width * 4;
        for (uint32_t c = 0; c < total_channels; ++c) {
            const uint16_t v = srow[c];
            drow[c] = static_cast<uint16_t>((v << 8) | v);
        }
    }
}

/**
 * @brief Converts a 64-bit `BGRA64` (16-bit per channel Deep Color) buffer to 32-bit `BGRA32` (8-bit per channel)
 * with rounding (`(v + 128) >> 8`).
 */
inline void ConvertBgra64ToBgra32(
    const uint16_t* src_bgra64,
    uint32_t        src_stride_bytes,
    uint8_t*        dst_bgra32,
    uint32_t        dst_stride,
    uint32_t        width,
    uint32_t        height) noexcept {
    if (!src_bgra64 || !dst_bgra32 || width == 0 || height == 0) {
        return;
    }
    for (uint32_t y = 0; y < height; ++y) {
        const uint16_t* srow = reinterpret_cast<const uint16_t*>(
            reinterpret_cast<const uint8_t*>(src_bgra64) + static_cast<size_t>(y) * src_stride_bytes);
        uint8_t* drow = dst_bgra32 + static_cast<size_t>(y) * dst_stride;
        const uint32_t total_channels = width * 4;
        for (uint32_t c = 0; c < total_channels; ++c) {
            drow[c] = static_cast<uint8_t>((static_cast<uint32_t>(srow[c]) * 255u + 32768u) >> 16);
        }
    }
}

/**
 * @brief Computes total byte size required for a frame of given dimensions and pixel format.
 */
inline constexpr size_t ComputeFrameByteSize(uint32_t width, uint32_t height, PixelFormat fmt, uint32_t stride = 0) noexcept {
    if (width == 0 || height == 0) {
        return 0;
    }
    if (fmt == PixelFormat::NV12) {
        const uint32_t eff_stride = (stride >= width) ? stride : width;
        return static_cast<size_t>(eff_stride) * height * 3 / 2;
    }
    const uint32_t bpp = GetBytesPerPixel(fmt);
    const uint32_t min_stride = width * bpp;
    const uint32_t eff_stride = (stride >= min_stride) ? stride : min_stride;
    return static_cast<size_t>(eff_stride) * height;
}

/**
 * @brief Non-owning POD view over a raw video frame or alpha mask.
 * Safe across C/C++ ABI boundaries and SIMD-aligned memory regions.
 */
struct FrameView {
    uint8_t*    data         = nullptr;               ///< Pointer to first scanline (64-byte alignment recommended for AVX-512)
    uint32_t    width        = 0;                     ///< Frame width in pixels
    uint32_t    height       = 0;                     ///< Frame height in pixels
    uint32_t    stride       = 0;                     ///< Bytes per row (including row padding)
    PixelFormat format       = PixelFormat::UNKNOWN;  ///< Pixel memory layout
    uint64_t    timestamp_ns = 0;                     ///< Monotonic capture timestamp in nanoseconds
    uint64_t    frame_index  = 0;                     ///< Sequential frame counter

    constexpr bool IsValid() const noexcept {
        return data != nullptr && width > 0 && height > 0 &&
               stride >= (width * GetBytesPerPixel(format)) &&
               format != PixelFormat::UNKNOWN;
    }

    constexpr size_t ByteSize() const noexcept {
        return ComputeFrameByteSize(width, height, format, stride);
    }
};

/**
 * @brief Owning, 64-byte SIMD-aligned frame buffer container.
 * Guarantees safe unaligned/aligned vector loads for AVX2 (32-byte) and AVX-512 (64-byte).
 */
class FrameBuffer {
public:
    static constexpr size_t kSimdAlignment = 64;

    FrameBuffer() noexcept = default;

    FrameBuffer(uint32_t width, uint32_t height, PixelFormat format) {
        Allocate(width, height, format);
    }

    ~FrameBuffer() {
        Release();
    }

    FrameBuffer(const FrameBuffer& other) {
        if (other.IsValid()) {
            Allocate(other.view_.width, other.view_.height, other.view_.format);
            std::memcpy(view_.data, other.view_.data, SizeBytes());
            view_.timestamp_ns = other.view_.timestamp_ns;
            view_.frame_index  = other.view_.frame_index;
        }
    }

    FrameBuffer& operator=(const FrameBuffer& other) {
        if (this != &other) {
            if (other.IsValid()) {
                Allocate(other.view_.width, other.view_.height, other.view_.format);
                std::memcpy(view_.data, other.view_.data, SizeBytes());
                view_.timestamp_ns = other.view_.timestamp_ns;
                view_.frame_index  = other.view_.frame_index;
            } else {
                Release();
            }
        }
        return *this;
    }

    FrameBuffer(FrameBuffer&& other) noexcept
        : raw_alloc_(other.raw_alloc_),
          capacity_(other.capacity_),
          view_(other.view_) {
        other.raw_alloc_ = nullptr;
        other.capacity_  = 0;
        other.view_      = FrameView{};
    }

    FrameBuffer& operator=(FrameBuffer&& other) noexcept {
        if (this != &other) {
            Release();
            raw_alloc_       = other.raw_alloc_;
            capacity_        = other.capacity_;
            view_            = other.view_;
            other.raw_alloc_ = nullptr;
            other.capacity_  = 0;
            other.view_      = FrameView{};
        }
        return *this;
    }

    bool Allocate(uint32_t width, uint32_t height, PixelFormat format) {
        const uint32_t bpp = GetBytesPerPixel(format);
        const uint32_t stride = (format == PixelFormat::NV12) ? width : (width * bpp);
        const size_t needed = ComputeFrameByteSize(width, height, format, stride);
        if (needed == 0) {
            Release();
            return false;
        }

        if (needed > capacity_ || view_.data == nullptr) {
            Release();
            // Allocate extra padding for 64-byte alignment + 64-byte tail safety for AVX-512 loops
            const size_t total_alloc = needed + kSimdAlignment * 2;
            raw_alloc_ = new (std::nothrow) uint8_t[total_alloc];
            if (!raw_alloc_) {
                return false;
            }
            const uintptr_t addr = reinterpret_cast<uintptr_t>(raw_alloc_);
            const uintptr_t aligned_addr = (addr + (kSimdAlignment - 1)) & ~(kSimdAlignment - 1);
            view_.data = reinterpret_cast<uint8_t*>(aligned_addr);
            capacity_  = needed;
            std::memset(view_.data, 0, capacity_);
        }

        view_.width  = width;
        view_.height = height;
        view_.stride = stride;
        view_.format = format;
        return true;
    }

    void Release() noexcept {
        delete[] raw_alloc_;
        raw_alloc_ = nullptr;
        capacity_  = 0;
        view_      = FrameView{};
    }

    bool IsValid() const noexcept { return view_.IsValid(); }
    FrameView& View() noexcept { return view_; }
    const FrameView& View() const noexcept { return view_; }
    uint8_t* Data() noexcept { return view_.data; }
    const uint8_t* Data() const noexcept { return view_.data; }
    uint32_t Width() const noexcept { return view_.width; }
    uint32_t Height() const noexcept { return view_.height; }
    uint32_t Stride() const noexcept { return view_.stride; }
    PixelFormat Format() const noexcept { return view_.format; }
    size_t SizeBytes() const noexcept { return view_.ByteSize(); }

private:
    uint8_t*  raw_alloc_ = nullptr;
    size_t    capacity_  = 0;
    FrameView view_{};
};

}  // namespace common
}  // namespace smartcam

#endif  // SMARTCAM_COMMON_FRAME_BUFFER_H_
