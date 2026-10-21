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

#include "v4l2_camera_capture.h"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <cstdio>
#include <cstring>

#if defined(__linux__)
#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#endif

namespace smartcam {
namespace platforms {
namespace linux_os {

namespace {

std::string ToUpperAscii(std::string s) {
    for (char& c : s) {
        c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
    }
    return s;
}

bool HasIrKeyword(const std::string& name, const std::string& bus) {
    const std::string n_up = ToUpperAscii(name);
    const std::string b_up = ToUpperAscii(bus);

    if (n_up.find("INFRARED") != std::string::npos ||
        n_up.find("INFRAROSSI") != std::string::npos ||
        n_up.find("WINDOWS HELLO") != std::string::npos ||
        n_up.find("HOWDY") != std::string::npos ||
        n_up.find("DEPTH") != std::string::npos ||
        b_up.find("INFRARED") != std::string::npos) {
        return true;
    }

    auto has_ir_token = [](const std::string& str) -> bool {
        size_t pos = 0;
        while ((pos = str.find("IR", pos)) != std::string::npos) {
            const bool left_ok =
                (pos == 0) || !std::isalpha(static_cast<unsigned char>(str[pos - 1]));
            const bool right_ok =
                (pos + 2 >= str.size()) || !std::isalpha(static_cast<unsigned char>(str[pos + 2]));
            if (left_ok && right_ok) {
                return true;
            }
            pos += 2;
        }
        return false;
    };

    return has_ir_token(n_up) || has_ir_token(b_up);
}

inline uint8_t ClampByte(int32_t v) noexcept {
    return static_cast<uint8_t>(v < 0 ? 0 : (v > 255 ? 255 : v));
}

inline void YuvToBgraPixel(int32_t y, int32_t u, int32_t v, uint8_t* dst) noexcept {
    const int32_t c = y - 16;
    const int32_t d = u - 128;
    const int32_t e = v - 128;

    const int32_t r = (298 * c + 409 * e + 128) >> 8;
    const int32_t g = (298 * c - 100 * d - 208 * e + 128) >> 8;
    const int32_t b = (298 * c + 516 * d + 128) >> 8;

    dst[0] = ClampByte(b);
    dst[1] = ClampByte(g);
    dst[2] = ClampByte(r);
    dst[3] = 255;
}

#if defined(__linux__)
int Xioctl(int fd, unsigned long request, void* arg) noexcept {
    int r = 0;
    do {
        r = ::ioctl(fd, request, arg);
    } while (r == -1 && (errno == EINTR || errno == EAGAIN));
    return r;
}

bool IsGreyOnlyFormatSensor(int fd) noexcept {
    uint32_t total_fmts = 0;
    uint32_t grey_fmts  = 0;

    for (uint32_t i = 0; i < 32; ++i) {
        struct v4l2_fmtdesc fmtd {};
        fmtd.index = i;
        fmtd.type  = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        if (Xioctl(fd, VIDIOC_ENUM_FMT, &fmtd) < 0) {
            break;
        }
        ++total_fmts;
        if (fmtd.pixelformat == V4L2_PIX_FMT_GREY ||
            fmtd.pixelformat == V4L2_PIX_FMT_Y10 ||
            fmtd.pixelformat == V4L2_PIX_FMT_Y12 ||
            fmtd.pixelformat == V4L2_PIX_FMT_Y16 ||
            fmtd.pixelformat == V4L2_PIX_FMT_Y16_BE) {
            ++grey_fmts;
        }
    }

    return (total_fmts > 0 && total_fmts == grey_fmts);
}
#endif

}  // namespace

std::vector<V4l2CameraDeviceInfo> V4l2CameraCapture::EnumerateDevices() {
    std::vector<V4l2CameraDeviceInfo> list;
#if defined(__linux__)
    uint32_t logical_idx = 0;
    for (int i = 0; i < 64; ++i) {
        char dev_path[64];
        std::snprintf(dev_path, sizeof(dev_path), "/dev/video%d", i);
        int fd = ::open(dev_path, O_RDWR | O_NONBLOCK);
        if (fd < 0) {
            continue;
        }

        struct v4l2_capability cap {};
        if (Xioctl(fd, VIDIOC_QUERYCAP, &cap) == 0) {
            const uint32_t caps = (cap.capabilities & V4L2_CAP_DEVICE_CAPS)
                                      ? cap.device_caps
                                      : cap.capabilities;
            const bool is_capture = (caps & V4L2_CAP_VIDEO_CAPTURE) != 0;
            const bool is_streaming = (caps & V4L2_CAP_STREAMING) != 0;

            const std::string driver = reinterpret_cast<const char*>(cap.driver);
            const std::string card   = reinterpret_cast<const char*>(cap.card);
            const std::string bus    = reinterpret_cast<const char*>(cap.bus_info);

            const std::string drv_up  = ToUpperAscii(driver);
            const std::string card_up = ToUpperAscii(card);
            const bool is_loopback =
                (drv_up.find("V4L2 LOOPBACK") != std::string::npos) ||
                (drv_up.find("V4L2LOOPBACK") != std::string::npos) ||
                (card_up == "SMARTCAM") ||
                (card_up.find("SMARTCAM VIRTUAL") != std::string::npos);

            if (is_capture && is_streaming && !is_loopback) {
                V4l2CameraDeviceInfo info;
                info.index         = logical_idx++;
                info.dev_node      = dev_path;
                info.friendly_name = card.empty() ? std::string(dev_path) : card;
                info.symbolic_link = bus.empty()
                                         ? std::string(dev_path)
                                         : (std::string(dev_path) + " (" + bus + ")");
                info.is_infrared   = HasIrKeyword(info.friendly_name, info.symbolic_link) ||
                                     IsGreyOnlyFormatSensor(fd);
                list.push_back(std::move(info));
            }
        }
        ::close(fd);
    }
#endif
    return list;
}

bool V4l2CameraCapture::Open(const common::CameraCaptureConfig& config) {
    Close();
#if defined(__linux__)
    const auto devices = EnumerateDevices();
    std::string target_node;

    if (!config.device_id.empty()) {
        for (const auto& d : devices) {
            if (d.dev_node == config.device_id || d.symbolic_link == config.device_id) {
                target_node = d.dev_node;
                break;
            }
        }
        if (target_node.empty() && config.device_id.rfind("/dev/video", 0) == 0) {
            target_node = config.device_id;
        }
    }

    if (target_node.empty()) {
        const size_t idx = (config.device_index >= 0) ? static_cast<size_t>(config.device_index) : 0u;
        if (idx < devices.size()) {
            target_node = devices[idx].dev_node;
        } else if (!devices.empty()) {
            target_node = devices[0].dev_node;
        } else {
            return false;
        }
    }

    fd_ = ::open(target_node.c_str(), O_RDWR | O_NONBLOCK);
    if (fd_ < 0) {
        return false;
    }

    const uint32_t req_w = (config.width > 0) ? config.width : 1280;
    const uint32_t req_h = (config.height > 0) ? config.height : 720;

    // Negotiate uncompressed or fast YUV format supported by V4L2 camera
    const uint32_t preferred_pixfmts[] = {
        V4L2_PIX_FMT_YUYV,
        V4L2_PIX_FMT_NV12,
        V4L2_PIX_FMT_BGR32,
        V4L2_PIX_FMT_RGB24,
        V4L2_PIX_FMT_BGR24,
        V4L2_PIX_FMT_GREY
    };

    bool fmt_negotiated = false;
    for (uint32_t pix : preferred_pixfmts) {
        struct v4l2_format fmt {};
        fmt.type                = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        fmt.fmt.pix.width       = req_w;
        fmt.fmt.pix.height      = req_h;
        fmt.fmt.pix.pixelformat = pix;
        fmt.fmt.pix.field       = V4L2_FIELD_ANY;

        if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) == 0 && fmt.fmt.pix.pixelformat == pix) {
            v4l2_pixfmt_          = fmt.fmt.pix.pixelformat;
            bytes_per_line_       = fmt.fmt.pix.bytesperline;
            actual_config_        = config;
            actual_config_.width  = fmt.fmt.pix.width;
            actual_config_.height = fmt.fmt.pix.height;
            actual_config_.output_format = common::PixelFormat::BGRA32;
            fmt_negotiated        = true;
            break;
        }
    }

    if (!fmt_negotiated) {
        // Query current default format and accept if we can decode it
        struct v4l2_format fmt {};
        fmt.type = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        if (Xioctl(fd_, VIDIOC_G_FMT, &fmt) == 0) {
            v4l2_pixfmt_          = fmt.fmt.pix.pixelformat;
            bytes_per_line_       = fmt.fmt.pix.bytesperline;
            actual_config_        = config;
            actual_config_.width  = fmt.fmt.pix.width;
            actual_config_.height = fmt.fmt.pix.height;
            actual_config_.output_format = common::PixelFormat::BGRA32;
            fmt_negotiated        = true;
        }
    }

    if (!fmt_negotiated || actual_config_.width == 0 || actual_config_.height == 0) {
        Close();
        return false;
    }

    // Configure target FPS via VIDIOC_S_PARM
    if (config.target_fps > 0) {
        struct v4l2_streamparm parm {};
        parm.type = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        parm.parm.capture.timeperframe.numerator   = 1;
        parm.parm.capture.timeperframe.denominator = config.target_fps;
        if (Xioctl(fd_, VIDIOC_S_PARM, &parm) == 0 &&
            parm.parm.capture.timeperframe.numerator > 0) {
            actual_config_.target_fps =
                parm.parm.capture.timeperframe.denominator /
                parm.parm.capture.timeperframe.numerator;
        }
    }

    // Request 4 MMAP kernel buffers for low-latency streaming
    struct v4l2_requestbuffers req {};
    req.count  = 4;
    req.type   = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    req.memory = V4L2_MEMORY_MMAP;

    if (Xioctl(fd_, VIDIOC_REQBUFS, &req) < 0 || req.count < 2) {
        Close();
        return false;
    }

    buffers_.resize(req.count);
    for (uint32_t i = 0; i < req.count; ++i) {
        struct v4l2_buffer buf {};
        buf.type   = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buf.memory = V4L2_MEMORY_MMAP;
        buf.index  = i;

        if (Xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
            Close();
            return false;
        }

        buffers_[i].length = buf.length;
        buffers_[i].start  = ::mmap(
            nullptr,
            buf.length,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd_,
            buf.m.offset);

        if (buffers_[i].start == MAP_FAILED) {
            buffers_[i].start = nullptr;
            Close();
            return false;
        }

        if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
            Close();
            return false;
        }
    }

    enum v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    if (Xioctl(fd_, VIDIOC_STREAMON, &type) < 0) {
        Close();
        return false;
    }

    streaming_     = true;
    is_open_       = true;
    frame_counter_ = 0;
    return true;
#else
    (void)config;
    return false;
#endif
}

void V4l2CameraCapture::DecodeToBgra32(
    const uint8_t* src,
    size_t src_bytes,
    common::FrameBuffer& out_frame) const noexcept {
#if defined(__linux__)
    const uint32_t w          = actual_config_.width;
    const uint32_t h          = actual_config_.height;
    const uint32_t dst_stride = out_frame.Stride();
    uint8_t*       dst_base   = out_frame.Data();

    if (v4l2_pixfmt_ == V4L2_PIX_FMT_YUYV) {
        const uint32_t src_stride = (bytes_per_line_ >= w * 2) ? bytes_per_line_ : (w * 2);
        const uint32_t even_w     = w & ~1u;
        for (uint32_t y = 0; y < h; ++y) {
            const size_t row_off = static_cast<size_t>(y) * src_stride;
            if (row_off + even_w * 2 > src_bytes) break;
            const uint8_t* srow = src + row_off;
            uint8_t*       drow = dst_base + static_cast<size_t>(y) * dst_stride;
            for (uint32_t x = 0; x < even_w; x += 2) {
                const int32_t y0 = srow[0];
                const int32_t u  = srow[1];
                const int32_t y1 = srow[2];
                const int32_t v  = srow[3];
                YuvToBgraPixel(y0, u, v, drow);
                YuvToBgraPixel(y1, u, v, drow + 4);
                srow += 4;
                drow += 8;
            }
        }
    } else if (v4l2_pixfmt_ == V4L2_PIX_FMT_NV12) {
        const uint32_t y_stride  = (bytes_per_line_ >= w) ? bytes_per_line_ : w;
        const size_t   uv_offset = static_cast<size_t>(y_stride) * h;
        const uint32_t even_w    = w & ~1u;
        for (uint32_t y = 0; y < h; ++y) {
            const size_t y_off  = static_cast<size_t>(y) * y_stride;
            const size_t uv_off = uv_offset + static_cast<size_t>(y / 2) * y_stride;
            if (uv_off + even_w > src_bytes) break;
            const uint8_t* y_row  = src + y_off;
            const uint8_t* uv_row = src + uv_off;
            uint8_t*       drow   = dst_base + static_cast<size_t>(y) * dst_stride;
            for (uint32_t x = 0; x < even_w; x += 2) {
                const int32_t u = uv_row[x];
                const int32_t v = uv_row[x + 1];
                YuvToBgraPixel(y_row[x], u, v, drow);
                YuvToBgraPixel(y_row[x + 1], u, v, drow + 4);
                drow += 8;
            }
        }
    } else if (v4l2_pixfmt_ == V4L2_PIX_FMT_RGB24) {
        const uint32_t src_stride = (bytes_per_line_ >= w * 3) ? bytes_per_line_ : (w * 3);
        for (uint32_t y = 0; y < h; ++y) {
            const size_t row_off = static_cast<size_t>(y) * src_stride;
            if (row_off + w * 3 > src_bytes) break;
            const uint8_t* srow = src + row_off;
            uint8_t*       drow = dst_base + static_cast<size_t>(y) * dst_stride;
            for (uint32_t x = 0; x < w; ++x) {
                drow[0] = srow[2];  // B
                drow[1] = srow[1];  // G
                drow[2] = srow[0];  // R
                drow[3] = 255;
                srow += 3;
                drow += 4;
            }
        }
    } else if (v4l2_pixfmt_ == V4L2_PIX_FMT_BGR24) {
        const uint32_t src_stride = (bytes_per_line_ >= w * 3) ? bytes_per_line_ : (w * 3);
        for (uint32_t y = 0; y < h; ++y) {
            const size_t row_off = static_cast<size_t>(y) * src_stride;
            if (row_off + w * 3 > src_bytes) break;
            const uint8_t* srow = src + row_off;
            uint8_t*       drow = dst_base + static_cast<size_t>(y) * dst_stride;
            for (uint32_t x = 0; x < w; ++x) {
                drow[0] = srow[0];  // B
                drow[1] = srow[1];  // G
                drow[2] = srow[2];  // R
                drow[3] = 255;
                srow += 3;
                drow += 4;
            }
        }
    } else if (v4l2_pixfmt_ == V4L2_PIX_FMT_GREY) {
        const uint32_t src_stride = (bytes_per_line_ >= w) ? bytes_per_line_ : w;
        for (uint32_t y = 0; y < h; ++y) {
            const size_t row_off = static_cast<size_t>(y) * src_stride;
            if (row_off + w > src_bytes) break;
            const uint8_t* srow = src + row_off;
            uint8_t*       drow = dst_base + static_cast<size_t>(y) * dst_stride;
            for (uint32_t x = 0; x < w; ++x) {
                const uint8_t g = srow[x];
                drow[0] = g;
                drow[1] = g;
                drow[2] = g;
                drow[3] = 255;
                drow += 4;
            }
        }
    } else {
        // V4L2_PIX_FMT_BGR32 / BGRA32 direct copy
        const uint32_t src_stride = (bytes_per_line_ >= w * 4) ? bytes_per_line_ : (w * 4);
        for (uint32_t y = 0; y < h; ++y) {
            const size_t row_off = static_cast<size_t>(y) * src_stride;
            if (row_off + w * 4 > src_bytes) break;
            std::memcpy(
                dst_base + static_cast<size_t>(y) * dst_stride,
                src + row_off,
                static_cast<size_t>(w) * 4);
        }
    }
#else
    (void)src;
    (void)src_bytes;
    (void)out_frame;
#endif
}

bool V4l2CameraCapture::ReadFrame(common::FrameBuffer& out_frame, uint32_t timeout_ms) {
#if defined(__linux__)
    if (!is_open_ || fd_ < 0) {
        return false;
    }

    fd_set fds;
    FD_ZERO(&fds);
    FD_SET(fd_, &fds);

    struct timeval tv {};
    tv.tv_sec  = static_cast<time_t>(timeout_ms / 1000);
    tv.tv_usec = static_cast<suseconds_t>((timeout_ms % 1000) * 1000);

    const int r = ::select(fd_ + 1, &fds, nullptr, nullptr, &tv);
    if (r <= 0) {
        return false;
    }

    struct v4l2_buffer buf {};
    buf.type   = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    buf.memory = V4L2_MEMORY_MMAP;

    if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
        return false;
    }

    if (buf.index >= buffers_.size() || !buffers_[buf.index].start) {
        (void)Xioctl(fd_, VIDIOC_QBUF, &buf);
        return false;
    }

    if (!out_frame.Allocate(
            actual_config_.width,
            actual_config_.height,
            common::PixelFormat::BGRA32)) {
        (void)Xioctl(fd_, VIDIOC_QBUF, &buf);
        return false;
    }

    const size_t valid_bytes = (buf.bytesused > 0 && buf.bytesused <= buffers_[buf.index].length)
                                   ? static_cast<size_t>(buf.bytesused)
                                   : buffers_[buf.index].length;

    DecodeToBgra32(
        static_cast<const uint8_t*>(buffers_[buf.index].start),
        valid_bytes,
        out_frame);

    const auto now_ns = std::chrono::duration_cast<std::chrono::nanoseconds>(
                            std::chrono::steady_clock::now().time_since_epoch())
                            .count();
    out_frame.View().timestamp_ns = static_cast<uint64_t>(now_ns);
    out_frame.View().frame_index  = ++frame_counter_;

    (void)Xioctl(fd_, VIDIOC_QBUF, &buf);
    return true;
#else
    (void)out_frame;
    (void)timeout_ms;
    return false;
#endif
}

void V4l2CameraCapture::Close() {
#if defined(__linux__)
    if (streaming_ && fd_ >= 0) {
        enum v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        (void)Xioctl(fd_, VIDIOC_STREAMOFF, &type);
        streaming_ = false;
    }

    for (auto& b : buffers_) {
        if (b.start && b.length > 0) {
            ::munmap(b.start, b.length);
            b.start  = nullptr;
            b.length = 0;
        }
    }
    buffers_.clear();

    if (fd_ >= 0) {
        ::close(fd_);
        fd_ = -1;
    }
#endif
    is_open_ = false;
}

}  // namespace linux_os
}  // namespace platforms
}  // namespace smartcam
