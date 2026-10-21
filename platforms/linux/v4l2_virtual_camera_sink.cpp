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

#include "v4l2_virtual_camera_sink.h"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>

#if defined(__linux__)
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#endif

namespace smartcam {
namespace platforms {
namespace linux_os {

namespace {

uint64_t GetMonotonicMs() noexcept {
    using namespace std::chrono;
    return static_cast<uint64_t>(
        duration_cast<milliseconds>(steady_clock::now().time_since_epoch()).count());
}

std::string ToLowerAscii(std::string s) {
    for (char& c : s) {
        c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    }
    return s;
}

inline uint8_t ClampU8(int32_t v) noexcept {
    return static_cast<uint8_t>(v < 0 ? 0 : (v > 255 ? 255 : v));
}

#if defined(__linux__)
int Xioctl(int fd, unsigned long request, void* arg) noexcept {
    int r = 0;
    do {
        r = ::ioctl(fd, request, arg);
    } while (r == -1 && (errno == EINTR || errno == EAGAIN));
    return r;
}
#endif

}  // namespace

std::string V4l2VirtualCameraSink::FindV4l2LoopbackDevice(const std::string& preferred_card_name) {
#if defined(__linux__)
    // 1. Check explicit environment override first (e.g., SMARTCAM_V4L2_DEVICE=/dev/video10)
    if (const char* env_dev = std::getenv("SMARTCAM_V4L2_DEVICE")) {
        if (*env_dev != '\0' && ::access(env_dev, R_OK | W_OK) == 0) {
            return std::string(env_dev);
        }
    }

    const std::string pref_low = ToLowerAscii(preferred_card_name);
    std::string fallback_loopback;

    // 2. Scan /dev/video0 .. /dev/video63 (and prioritize /dev/video10 if it matches)
    const int scan_order[] = {
        10, 11, 12, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
        13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31
    };

    for (int idx : scan_order) {
        char path[64];
        std::snprintf(path, sizeof(path), "/dev/video%d", idx);
        int fd = ::open(path, O_RDWR | O_NONBLOCK);
        if (fd < 0) {
            continue;
        }

        struct v4l2_capability cap {};
        if (Xioctl(fd, VIDIOC_QUERYCAP, &cap) == 0) {
            const std::string driver = ToLowerAscii(reinterpret_cast<const char*>(cap.driver));
            const std::string card   = ToLowerAscii(reinterpret_cast<const char*>(cap.card));

            const bool is_loopback_driver =
                (driver.find("v4l2 loopback") != std::string::npos) ||
                (driver.find("v4l2loopback") != std::string::npos);
            const bool is_smartcam_card =
                (!pref_low.empty() && card.find(pref_low) != std::string::npos) ||
                (card.find("smartcam") != std::string::npos);

            if (is_loopback_driver && is_smartcam_card) {
                ::close(fd);
                return std::string(path);
            }
            if ((is_loopback_driver || is_smartcam_card) && fallback_loopback.empty()) {
                fallback_loopback = path;
            }
        }
        ::close(fd);
    }

    return fallback_loopback;
#else
    (void)preferred_card_name;
    return "";
#endif
}

bool V4l2VirtualCameraSink::Initialize(const common::VirtualCameraConfig& config) {
    Shutdown();
    config_ = config;
    if (config_.width == 0)  config_.width  = 1440;
    if (config_.height == 0) config_.height = 1080;
    if (config_.fps_num == 0) config_.fps_num = 30;
    if (config_.fps_den == 0) config_.fps_den = 1;

#if defined(__linux__)
    device_path_ = FindV4l2LoopbackDevice(config_.friendly_name);
    if (device_path_.empty()) {
        // Fallback probe of default /dev/video10 created by setup_v4l2loopback.sh
        if (::access("/dev/video10", R_OK | W_OK) == 0) {
            device_path_ = "/dev/video10";
        } else {
            state_ = common::VirtualCameraState::ERROR_STATE;
            return false;
        }
    }

    fd_ = ::open(device_path_.c_str(), O_RDWR);
    if (fd_ < 0) {
        state_ = common::VirtualCameraState::ERROR_STATE;
        return false;
    }

    struct v4l2_capability cap {};
    if (Xioctl(fd_, VIDIOC_QUERYCAP, &cap) < 0) {
        Shutdown();
        state_ = common::VirtualCameraState::ERROR_STATE;
        return false;
    }

    const uint32_t candidate_formats[] = {
        V4L2_PIX_FMT_YUYV,
        V4L2_PIX_FMT_RGB24,
        V4L2_PIX_FMT_BGR32
    };

    bool fmt_ok = false;
    for (uint32_t pix_fmt : candidate_formats) {
        struct v4l2_format fmt {};
        fmt.type                = V4L2_BUF_TYPE_VIDEO_OUTPUT;
        fmt.fmt.pix.width       = config_.width;
        fmt.fmt.pix.height      = config_.height;
        fmt.fmt.pix.pixelformat = pix_fmt;
        fmt.fmt.pix.field       = V4L2_FIELD_NONE;
        fmt.fmt.pix.colorspace  = V4L2_COLORSPACE_SRGB;

        if (pix_fmt == V4L2_PIX_FMT_YUYV) {
            fmt.fmt.pix.bytesperline = config_.width * 2;
            fmt.fmt.pix.sizeimage    = config_.width * config_.height * 2;
        } else if (pix_fmt == V4L2_PIX_FMT_RGB24) {
            fmt.fmt.pix.bytesperline = config_.width * 3;
            fmt.fmt.pix.sizeimage    = config_.width * config_.height * 3;
        } else {
            fmt.fmt.pix.bytesperline = config_.width * 4;
            fmt.fmt.pix.sizeimage    = config_.width * config_.height * 4;
        }

        if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) == 0) {
            negotiated_pix_ = fmt.fmt.pix.pixelformat;
            negotiated_w_   = fmt.fmt.pix.width;
            negotiated_h_   = fmt.fmt.pix.height;
            tx_buffer_.resize(fmt.fmt.pix.sizeimage);
            fmt_ok = true;
            break;
        }
    }

    if (!fmt_ok) {
        Shutdown();
        state_ = common::VirtualCameraState::ERROR_STATE;
        return false;
    }

    // Best-effort framerate parameter setup on v4l2loopback output stream
    struct v4l2_streamparm parm {};
    parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT;
    parm.parm.output.timeperframe.numerator   = config_.fps_den;
    parm.parm.output.timeperframe.denominator = config_.fps_num;
    (void)Xioctl(fd_, VIDIOC_S_PARM, &parm);

    frame_count_ = 0;
    state_       = common::VirtualCameraState::READY;
    return true;
#else
    state_ = common::VirtualCameraState::ERROR_STATE;
    return false;
#endif
}

void V4l2VirtualCameraSink::ConvertBgraToYuyv(
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t src_stride,
    uint8_t* dst_yuyv) const noexcept {
    // BT.601 Studio-range integer conversion: 2 BGRA pixels -> 4 bytes (Y0, U, Y1, V)
    const uint32_t even_w = width & ~1u;
    for (uint32_t y = 0; y < height; ++y) {
        const uint8_t* srow = src_bgra + static_cast<size_t>(y) * src_stride;
        uint8_t*       drow = dst_yuyv + static_cast<size_t>(y) * (width * 2);

        for (uint32_t x = 0; x < even_w; x += 2) {
            const int32_t b0 = srow[0];
            const int32_t g0 = srow[1];
            const int32_t r0 = srow[2];

            const int32_t b1 = srow[4];
            const int32_t g1 = srow[5];
            const int32_t r1 = srow[6];

            const int32_t y0 = ((66 * r0 + 129 * g0 + 25 * b0 + 128) >> 8) + 16;
            const int32_t y1 = ((66 * r1 + 129 * g1 + 25 * b1 + 128) >> 8) + 16;

            const int32_t r_avg = (r0 + r1) >> 1;
            const int32_t g_avg = (g0 + g1) >> 1;
            const int32_t b_avg = (b0 + b1) >> 1;

            const int32_t u = ((-38 * r_avg - 74 * g_avg + 112 * b_avg + 128) >> 8) + 128;
            const int32_t v = ((112 * r_avg - 94 * g_avg - 18 * b_avg + 128) >> 8) + 128;

            drow[0] = ClampU8(y0);
            drow[1] = ClampU8(u);
            drow[2] = ClampU8(y1);
            drow[3] = ClampU8(v);

            srow += 8;
            drow += 4;
        }
    }
}

void V4l2VirtualCameraSink::ConvertBgraToRgb24(
    const uint8_t* src_bgra,
    uint32_t width,
    uint32_t height,
    uint32_t src_stride,
    uint8_t* dst_rgb24) const noexcept {
    for (uint32_t y = 0; y < height; ++y) {
        const uint8_t* srow = src_bgra + static_cast<size_t>(y) * src_stride;
        uint8_t*       drow = dst_rgb24 + static_cast<size_t>(y) * (width * 3);
        for (uint32_t x = 0; x < width; ++x) {
            drow[0] = srow[2];  // R
            drow[1] = srow[1];  // G
            drow[2] = srow[0];  // B
            srow += 4;
            drow += 3;
        }
    }
}

bool V4l2VirtualCameraSink::PushFrame(const common::FrameView& frame) {
#if defined(__linux__)
    if (fd_ < 0 || !frame.IsValid() || frame.format != common::PixelFormat::BGRA32) {
        return false;
    }

    // Re-negotiate if incoming resolution changed dynamically (e.g. user changed preset 1080p -> 720p)
    if (frame.width != negotiated_w_ || frame.height != negotiated_h_) {
        common::VirtualCameraConfig new_cfg = config_;
        new_cfg.width  = frame.width;
        new_cfg.height = frame.height;
        if (!Initialize(new_cfg)) {
            return false;
        }
    }

    const uint8_t* write_ptr = nullptr;
    size_t         write_len = 0;

    if (negotiated_pix_ == V4L2_PIX_FMT_YUYV) {
        const size_t needed = static_cast<size_t>(negotiated_w_) * negotiated_h_ * 2;
        if (tx_buffer_.size() < needed) {
            tx_buffer_.resize(needed);
        }
        ConvertBgraToYuyv(frame.data, negotiated_w_, negotiated_h_, frame.stride, tx_buffer_.data());
        write_ptr = tx_buffer_.data();
        write_len = needed;
    } else if (negotiated_pix_ == V4L2_PIX_FMT_RGB24) {
        const size_t needed = static_cast<size_t>(negotiated_w_) * negotiated_h_ * 3;
        if (tx_buffer_.size() < needed) {
            tx_buffer_.resize(needed);
        }
        ConvertBgraToRgb24(frame.data, negotiated_w_, negotiated_h_, frame.stride, tx_buffer_.data());
        write_ptr = tx_buffer_.data();
        write_len = needed;
    } else {
        // V4L2_PIX_FMT_BGR32 (BGRA32 memory layout)
        const size_t needed = static_cast<size_t>(negotiated_w_) * negotiated_h_ * 4;
        if (frame.stride == negotiated_w_ * 4) {
            write_ptr = frame.data;
            write_len = needed;
        } else {
            if (tx_buffer_.size() < needed) {
                tx_buffer_.resize(needed);
            }
            for (uint32_t y = 0; y < negotiated_h_; ++y) {
                std::memcpy(
                    tx_buffer_.data() + static_cast<size_t>(y) * negotiated_w_ * 4,
                    frame.data + static_cast<size_t>(y) * frame.stride,
                    static_cast<size_t>(negotiated_w_) * 4);
            }
            write_ptr = tx_buffer_.data();
            write_len = needed;
        }
    }

    const ssize_t written = ::write(fd_, write_ptr, write_len);
    if (written < 0) {
        return false;
    }

    ++frame_count_;
    state_ = common::VirtualCameraState::STREAMING;
    return static_cast<size_t>(written) == write_len;
#else
    (void)frame;
    return false;
#endif
}

bool V4l2VirtualCameraSink::IsConsumerActive(uint64_t timeout_ms) const noexcept {
#if defined(__linux__)
    if (fd_ < 0 || device_path_.empty()) {
        return false;
    }

    const uint64_t now_ms = GetMonotonicMs();
    if (last_active_ms_ != 0 && (now_ms - last_active_ms_) < timeout_ms) {
        return true;
    }

    const pid_t self_pid = ::getpid();

    // Scan /proc/<pid>/fd/* for any other process holding our /dev/videoN device open
    DIR* proc_dir = ::opendir("/proc");
    if (!proc_dir) {
        return true;  // Fail-open if /proc is restricted
    }

    bool found_reader = false;
    struct dirent* p_entry = nullptr;
    while (!found_reader && (p_entry = ::readdir(proc_dir)) != nullptr) {
        if (!std::isdigit(static_cast<unsigned char>(p_entry->d_name[0]))) {
            continue;
        }
        const pid_t pid = static_cast<pid_t>(std::atoi(p_entry->d_name));
        if (pid <= 0 || pid == self_pid) {
            continue;
        }

        char fd_dir_path[128];
        std::snprintf(fd_dir_path, sizeof(fd_dir_path), "/proc/%d/fd", static_cast<int>(pid));
        DIR* fd_dir = ::opendir(fd_dir_path);
        if (!fd_dir) {
            continue;
        }

        struct dirent* fd_entry = nullptr;
        while ((fd_entry = ::readdir(fd_dir)) != nullptr) {
            if (fd_entry->d_name[0] == '.') {
                continue;
            }
            char link_path[256];
            char target_path[256];
            std::snprintf(link_path, sizeof(link_path), "%s/%s", fd_dir_path, fd_entry->d_name);
            const ssize_t len = ::readlink(link_path, target_path, sizeof(target_path) - 1);
            if (len > 0) {
                target_path[len] = '\0';
                if (device_path_ == target_path) {
                    found_reader = true;
                    break;
                }
            }
        }
        ::closedir(fd_dir);
    }
    ::closedir(proc_dir);

    if (found_reader) {
        last_active_ms_ = now_ms;
        return true;
    }
    return false;
#else
    (void)timeout_ms;
    return false;
#endif
}

void V4l2VirtualCameraSink::Shutdown() {
#if defined(__linux__)
    if (fd_ >= 0) {
        ::close(fd_);
        fd_ = -1;
    }
#endif
    tx_buffer_.clear();
    negotiated_pix_ = 0;
    negotiated_w_   = 0;
    negotiated_h_   = 0;
    state_          = common::VirtualCameraState::STOPPED;
}

}  // namespace linux_os
}  // namespace platforms
}  // namespace smartcam
