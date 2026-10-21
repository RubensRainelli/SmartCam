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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_IPC_SHARED_FRAME_IPC_H_
#define SMARTCAM_PLATFORMS_WINDOWS_IPC_SHARED_FRAME_IPC_H_

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <cstdint>
#include <cstring>

namespace smartcam {
namespace platforms {
namespace windows {

/**
 * @brief Shared memory layout for zero-elevation user-mode frame transport between
 *        the SmartCam runtime pipeline and the Media Foundation COM Virtual Camera Source.
 */
#pragma pack(push, 8)
struct SharedFrameHeader {
    static constexpr uint32_t kMagic   = 0x534D434Du;  // "SMCM"
    static constexpr uint32_t kVersion = 1u;
    static constexpr size_t   kMaxFrameBytes = 3840ull * 2160ull * 4ull; // Up to 4K BGRA32 (33.1 MB)

    uint32_t          magic;            ///< Must be `kMagic` (0x534D434D)
    uint32_t          version;          ///< Protocol version (`1`)
    volatile LONG64   sequence_counter; ///< Odd = writer in progress, Even = consistent frame ready
    uint32_t          width;            ///< Active frame width in pixels
    uint32_t          height;           ///< Active frame height in pixels
    uint32_t          stride;           ///< Bytes per row (`width * 4` for BGRA32)
    uint32_t          fps_num;          ///< Target framerate numerator
    uint32_t          fps_den;          ///< Target framerate denominator
    uint32_t          pixel_format;     ///< `PixelFormat::BGRA32` (4)
    uint64_t          timestamp_100ns;            ///< Media Foundation 100-ns presentation timestamp
    uint64_t          frame_index;                ///< Monotonic frame number
    volatile LONG64   last_consumer_heartbeat_ms; ///< GetTickCount64() ms timestamp of last client RequestSample/StartStream
    volatile LONG64   consumer_requests;          ///< Monotonic counter of client frame requests
    uint8_t           reserved[48];               ///< Padding to 128-byte boundary before pixel payload
};
#pragma pack(pop)

/**
 * @brief Named Windows File Mapping wrapper used by both the User-Mode Producer (`MfVirtualCameraSink`)
 *        and the Media Foundation COM Consumer (`SmartCamMfMediaSource`).
 */
class SharedFrameIpc {
public:
    static constexpr const wchar_t* kDefaultMappingName = L"Local\\SmartCam_MF_SharedFrame_v1";
    static constexpr const wchar_t* kDefaultEventName   = L"Local\\SmartCam_MF_FrameEvent_v1";

    SharedFrameIpc() noexcept = default;
    ~SharedFrameIpc() { Close(); }

    SharedFrameIpc(const SharedFrameIpc&) = delete;
    SharedFrameIpc& operator=(const SharedFrameIpc&) = delete;

    bool OpenOrCreate(const wchar_t* mapping_name = kDefaultMappingName,
                      const wchar_t* event_name   = kDefaultEventName,
                      bool is_producer            = true) noexcept {
        Close();
        is_producer_ = is_producer;

        const size_t total_bytes = sizeof(SharedFrameHeader) + SharedFrameHeader::kMaxFrameBytes;
        const DWORD size_high    = static_cast<DWORD>((total_bytes >> 32) & 0xFFFFFFFFull);
        const DWORD size_low     = static_cast<DWORD>(total_bytes & 0xFFFFFFFFull);

        SECURITY_DESCRIPTOR sd{};
        SECURITY_ATTRIBUTES sa{};
        SECURITY_ATTRIBUTES* p_sa = nullptr;
        if (InitializeSecurityDescriptor(&sd, SECURITY_DESCRIPTOR_REVISION) &&
            SetSecurityDescriptorDacl(&sd, TRUE, nullptr, FALSE)) {
            sa.nLength              = sizeof(SECURITY_ATTRIBUTES);
            sa.lpSecurityDescriptor = &sd;
            sa.bInheritHandle       = FALSE;
            p_sa                    = &sa;
        }

        bool newly_created = false;
        if (!is_producer) {
            mapping_handle_ = OpenFileMappingW(FILE_MAP_ALL_ACCESS, FALSE, mapping_name);
            if (!mapping_handle_) {
                mapping_handle_ = OpenFileMappingW(FILE_MAP_READ, FALSE, mapping_name);
                if (!mapping_handle_) {
                    return false;
                }
                read_only_ = true;
            }
        } else {
            mapping_handle_ = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                p_sa,
                PAGE_READWRITE,
                size_high,
                size_low,
                mapping_name);

            if (!mapping_handle_) {
                mapping_handle_ = OpenFileMappingW(FILE_MAP_ALL_ACCESS, FALSE, mapping_name);
                if (!mapping_handle_) {
                    return false;
                }
            } else {
                newly_created = (GetLastError() != ERROR_ALREADY_EXISTS);
            }
        }

        mapped_view_ = static_cast<uint8_t*>(
            MapViewOfFile(mapping_handle_, read_only_ ? FILE_MAP_READ : FILE_MAP_ALL_ACCESS, 0, 0, total_bytes));
        if (!mapped_view_ && !read_only_) {
            mapped_view_ = static_cast<uint8_t*>(
                MapViewOfFile(mapping_handle_, FILE_MAP_READ, 0, 0, total_bytes));
            if (mapped_view_) {
                read_only_ = true;
            }
        }
        if (!mapped_view_) {
            Close();
            return false;
        }

        if (is_producer) {
            frame_event_ = CreateEventW(p_sa, FALSE, FALSE, event_name);
        }

        if (!read_only_) {
            SharedFrameHeader* hdr = Header();
            if (newly_created || hdr->magic != SharedFrameHeader::kMagic) {
                std::memset(hdr, 0, sizeof(SharedFrameHeader));
                hdr->magic            = SharedFrameHeader::kMagic;
                hdr->version          = SharedFrameHeader::kVersion;
                hdr->sequence_counter = 0;
                hdr->width            = 1440;
                hdr->height           = 1080;
                hdr->stride           = 1440 * 4;
                hdr->fps_num          = 30;
                hdr->fps_den          = 1;
                hdr->pixel_format     = 4; // BGRA32
            }
        }
        return true;
    }

    void Close() noexcept {
        if (mapped_view_) {
            UnmapViewOfFile(mapped_view_);
            mapped_view_ = nullptr;
        }
        if (mapping_handle_) {
            CloseHandle(mapping_handle_);
            mapping_handle_ = nullptr;
        }
        if (frame_event_) {
            CloseHandle(frame_event_);
            frame_event_ = nullptr;
        }
        read_only_ = false;
    }

    bool WriteFrame(const uint8_t* src_data,
                    uint32_t width,
                    uint32_t height,
                    uint32_t src_stride,
                    uint32_t fps_num,
                    uint32_t fps_den,
                    uint64_t timestamp_100ns,
                    uint64_t frame_index) noexcept {
        if (!mapped_view_ || !src_data) return false;
        const size_t dst_stride  = static_cast<size_t>(width) * 4;
        const size_t frame_bytes = dst_stride * height;
        if (frame_bytes == 0 || frame_bytes > SharedFrameHeader::kMaxFrameBytes) {
            return false;
        }

        SharedFrameHeader* hdr = Header();
        // Increment sequence counter to odd value (signals write in progress)
        InterlockedIncrement64(&hdr->sequence_counter);
        MemoryBarrier();

        hdr->width           = width;
        hdr->height          = height;
        hdr->stride          = static_cast<uint32_t>(dst_stride);
        hdr->fps_num         = fps_num;
        hdr->fps_den         = fps_den;
        hdr->timestamp_100ns = timestamp_100ns;
        hdr->frame_index     = frame_index;

        uint8_t* dst_payload = Payload();
        if (src_stride == dst_stride) {
            std::memcpy(dst_payload, src_data, frame_bytes);
        } else {
            for (uint32_t y = 0; y < height; ++y) {
                std::memcpy(dst_payload + y * dst_stride, src_data + static_cast<size_t>(y) * src_stride, dst_stride);
            }
        }

        MemoryBarrier();
        // Increment sequence counter to even value (signals consistent frame ready)
        InterlockedIncrement64(&hdr->sequence_counter);

        if (frame_event_) {
            SetEvent(frame_event_);
        }
        return true;
    }

    bool ReadFrame(uint8_t* dst_buffer,
                   size_t dst_capacity,
                   uint32_t& out_width,
                   uint32_t& out_height,
                   uint64_t& out_timestamp_100ns) noexcept {
        if (!mapped_view_ || !dst_buffer) return false;
        TouchConsumerHeartbeat();
        const SharedFrameHeader* hdr = Header();
        if (hdr->magic != SharedFrameHeader::kMagic || hdr->sequence_counter == 0) {
            return false;
        }

        for (int attempt = 0; attempt < 3; ++attempt) {
            const LONG64 seq_before = InterlockedCompareExchange64(
                const_cast<volatile LONG64*>(&hdr->sequence_counter), 0, 0);
            if ((seq_before & 1LL) != 0) {
                // Writer active, yield briefly
                SwitchToThread();
                continue;
            }

            const uint32_t w          = hdr->width;
            const uint32_t h          = hdr->height;
            const size_t   needed     = static_cast<size_t>(w) * h * 4;
            const uint64_t ts_100ns   = hdr->timestamp_100ns;

            if (needed == 0 || needed > dst_capacity || needed > SharedFrameHeader::kMaxFrameBytes) {
                return false;
            }

            std::memcpy(dst_buffer, Payload(), needed);
            MemoryBarrier();

            const LONG64 seq_after = InterlockedCompareExchange64(
                const_cast<volatile LONG64*>(&hdr->sequence_counter), 0, 0);
            if (seq_before == seq_after && seq_after > 0) {
                out_width           = w;
                out_height          = h;
                out_timestamp_100ns = ts_100ns;
                return true;
            }
        }
        return false;
    }

    /**
     * @brief Records a consumer heartbeat timestamp (`GetTickCount64()`) in shared memory.
     *        Called by `SmartCamMfMediaStream::RequestSample` and `StartStream`.
     */
    void TouchConsumerHeartbeat() noexcept {
        if (!mapped_view_ || read_only_) return;
        SharedFrameHeader* hdr = Header();
        if (hdr->magic == SharedFrameHeader::kMagic) {
            InterlockedExchange64(&hdr->last_consumer_heartbeat_ms, static_cast<LONG64>(GetTickCount64()));
            InterlockedIncrement64(&hdr->consumer_requests);
        }
    }

    /**
     * @brief Returns true if a Virtual Camera consumer (Zoom, Teams, Chrome, etc.) has requested
     *        a frame or started the stream within the last `timeout_ms` milliseconds.
     */
    bool IsConsumerActive(uint64_t timeout_ms = 2000) const noexcept {
        if (!mapped_view_) return false;
        const SharedFrameHeader* hdr = Header();
        if (hdr->magic != SharedFrameHeader::kMagic) return false;
        const LONG64 last_hb = InterlockedCompareExchange64(
            const_cast<volatile LONG64*>(&hdr->last_consumer_heartbeat_ms), 0, 0);
        if (last_hb <= 0) return false;
        const uint64_t now_ms = GetTickCount64();
        const uint64_t hb_ms  = static_cast<uint64_t>(last_hb);
        return (now_ms >= hb_ms) && ((now_ms - hb_ms) <= timeout_ms);
    }

    bool IsOpen() const noexcept { return mapped_view_ != nullptr; }
    SharedFrameHeader* Header() noexcept { return reinterpret_cast<SharedFrameHeader*>(mapped_view_); }
    const SharedFrameHeader* Header() const noexcept { return reinterpret_cast<const SharedFrameHeader*>(mapped_view_); }
    uint8_t* Payload() noexcept { return mapped_view_ + sizeof(SharedFrameHeader); }
    const uint8_t* Payload() const noexcept { return mapped_view_ + sizeof(SharedFrameHeader); }

private:
    HANDLE   mapping_handle_ = nullptr;
    HANDLE   frame_event_    = nullptr;
    uint8_t* mapped_view_    = nullptr;
    bool     is_producer_    = true;
    bool     read_only_      = false;
};

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_IPC_SHARED_FRAME_IPC_H_
