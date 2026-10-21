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

#include "mf_camera_capture.h"
#include <cstring>

#if defined(_MSC_VER)
#pragma comment(lib, "mf.lib")
#pragma comment(lib, "mfplat.lib")
#pragma comment(lib, "mfreadwrite.lib")
#pragma comment(lib, "mfuuid.lib")
#pragma comment(lib, "ole32.lib")
#endif

namespace smartcam {
namespace platforms {
namespace windows {

namespace {

std::string WideToUtf8(const wchar_t* wstr) {
    if (!wstr || !*wstr) return "";
    const int len = static_cast<int>(wcslen(wstr));
    const int needed = WideCharToMultiByte(CP_UTF8, 0, wstr, len, nullptr, 0, nullptr, nullptr);
    if (needed <= 0) return "";
    std::string out(static_cast<size_t>(needed), '\0');
    WideCharToMultiByte(CP_UTF8, 0, wstr, len, &out[0], needed, nullptr, nullptr);
    return out;
}

}  // namespace

std::vector<MfCameraDeviceInfo> MfCameraCapture::EnumerateDevices() {
    std::vector<MfCameraDeviceInfo> devices;
    CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(MFStartup(MF_VERSION))) {
        return devices;
    }

    Microsoft::WRL::ComPtr<IMFAttributes> attr;
    if (SUCCEEDED(MFCreateAttributes(&attr, 1))) {
        attr->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);
        IMFActivate** ppDevices = nullptr;
        UINT32 count = 0;
        if (SUCCEEDED(MFEnumDeviceSources(attr.Get(), &ppDevices, &count))) {
            for (UINT32 i = 0; i < count; ++i) {
                MfCameraDeviceInfo info;
                info.index = i;
                WCHAR* name = nullptr;
                UINT32 name_len = 0;
                if (SUCCEEDED(ppDevices[i]->GetAllocatedString(MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &name, &name_len))) {
                    info.friendly_name = WideToUtf8(name);
                    CoTaskMemFree(name);
                }
                WCHAR* sym = nullptr;
                UINT32 sym_len = 0;
                if (SUCCEEDED(ppDevices[i]->GetAllocatedString(
                        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, &sym, &sym_len))) {
                    info.symbolic_link = WideToUtf8(sym);
                    CoTaskMemFree(sym);
                }
                if (info.friendly_name != "SmartCam" &&
                    info.friendly_name.find("SmartCam Virtual Camera") == std::string::npos) {
                    info.index = static_cast<uint32_t>(devices.size());
                    devices.push_back(info);
                }
                ppDevices[i]->Release();
            }
            CoTaskMemFree(ppDevices);
        }
    }

    MFShutdown();
    return devices;
}

bool MfCameraCapture::Open(const common::CameraCaptureConfig& config) {
    Close();
    actual_config_ = config;

    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(hr) && hr != RPC_E_CHANGED_MODE) {
        return false;
    }

    hr = MFStartup(MF_VERSION);
    if (FAILED(hr)) {
        return false;
    }
    mf_started_ = true;

    Microsoft::WRL::ComPtr<IMFAttributes> enum_attr;
    hr = MFCreateAttributes(&enum_attr, 1);
    if (FAILED(hr)) {
        Close();
        return false;
    }
    enum_attr->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);

    IMFActivate** ppDevices = nullptr;
    UINT32 count = 0;
    hr = MFEnumDeviceSources(enum_attr.Get(), &ppDevices, &count);
    if (FAILED(hr) || count == 0) {
        Close();
        return false;
    }

    UINT32 selected_idx = 0;
    if (!config.device_id.empty()) {
        for (UINT32 i = 0; i < count; ++i) {
            WCHAR* sym = nullptr;
            UINT32 sym_len = 0;
            if (SUCCEEDED(ppDevices[i]->GetAllocatedString(
                    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, &sym, &sym_len))) {
                const std::string sym_utf8 = WideToUtf8(sym);
                CoTaskMemFree(sym);
                if (sym_utf8.find(config.device_id) != std::string::npos) {
                    selected_idx = i;
                    break;
                }
            }
        }
    } else if (config.device_index >= 0 && static_cast<UINT32>(config.device_index) < count) {
        selected_idx = static_cast<UINT32>(config.device_index);
    }

    Microsoft::WRL::ComPtr<IMFMediaSource> media_source;
    hr = ppDevices[selected_idx]->ActivateObject(IID_PPV_ARGS(&media_source));
    for (UINT32 i = 0; i < count; ++i) {
        ppDevices[i]->Release();
    }
    CoTaskMemFree(ppDevices);

    if (FAILED(hr) || !media_source) {
        Close();
        return false;
    }

    // Enable MF SourceReader Video Processing so NV12/YUY2/MJPEG hardware streams decode directly to RGB32 (BGRA32)
    Microsoft::WRL::ComPtr<IMFAttributes> reader_attr;
    MFCreateAttributes(&reader_attr, 2);
    if (reader_attr) {
        reader_attr->SetUINT32(MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, TRUE);
    }

    hr = MFCreateSourceReaderFromMediaSource(media_source.Get(), reader_attr.Get(), &reader_);
    if (FAILED(hr) || !reader_) {
        Close();
        return false;
    }

    Microsoft::WRL::ComPtr<IMFMediaType> out_type;
    MFCreateMediaType(&out_type);
    out_type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    out_type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_RGB32);
    MFSetAttributeSize(out_type.Get(), MF_MT_FRAME_SIZE, config.width, config.height);
    MFSetAttributeRatio(out_type.Get(), MF_MT_FRAME_RATE, config.target_fps, 1);

    hr = reader_->SetCurrentMediaType(static_cast<DWORD>(MF_SOURCE_READER_FIRST_VIDEO_STREAM), nullptr, out_type.Get());
    if (FAILED(hr)) {
        // Fallback: keep camera's native resolution/fps but request RGB32 decoding
        Microsoft::WRL::ComPtr<IMFMediaType> fallback_type;
        MFCreateMediaType(&fallback_type);
        fallback_type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
        fallback_type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_RGB32);
        hr = reader_->SetCurrentMediaType(static_cast<DWORD>(MF_SOURCE_READER_FIRST_VIDEO_STREAM), nullptr, fallback_type.Get());
        if (FAILED(hr)) {
            Close();
            return false;
        }
    }

    // Query actual negotiated resolution & framerate
    Microsoft::WRL::ComPtr<IMFMediaType> current_type;
    if (SUCCEEDED(reader_->GetCurrentMediaType(static_cast<DWORD>(MF_SOURCE_READER_FIRST_VIDEO_STREAM), &current_type))) {
        UINT32 w = 0, h = 0, fps_n = 0, fps_d = 1;
        if (SUCCEEDED(MFGetAttributeSize(current_type.Get(), MF_MT_FRAME_SIZE, &w, &h))) {
            actual_config_.width  = w;
            actual_config_.height = h;
        }
        if (SUCCEEDED(MFGetAttributeRatio(current_type.Get(), MF_MT_FRAME_RATE, &fps_n, &fps_d)) && fps_d > 0) {
            actual_config_.target_fps = fps_n / fps_d;
        }
    }

    is_open_ = true;
    return true;
}

bool MfCameraCapture::ReadFrame(common::FrameBuffer& out_frame, uint32_t /*timeout_ms*/) {
    if (!is_open_ || !reader_) {
        return false;
    }

    DWORD stream_index = 0;
    DWORD flags = 0;
    LONGLONG ll_timestamp = 0;
    Microsoft::WRL::ComPtr<IMFSample> sample;

    HRESULT hr = reader_->ReadSample(
        static_cast<DWORD>(MF_SOURCE_READER_FIRST_VIDEO_STREAM),
        0,
        &stream_index,
        &flags,
        &ll_timestamp,
        &sample);

    if (FAILED(hr) || (flags & MF_SOURCE_READERF_ERROR)) {
        return false;
    }
    if (!sample) {
        return false;
    }

    Microsoft::WRL::ComPtr<IMFMediaBuffer> buffer;
    hr = sample->ConvertToContiguousBuffer(&buffer);
    if (FAILED(hr) || !buffer) {
        return false;
    }

    BYTE* raw_data = nullptr;
    DWORD cur_len = 0;
    hr = buffer->Lock(&raw_data, nullptr, &cur_len);
    if (FAILED(hr) || !raw_data) {
        return false;
    }

    const uint32_t w = actual_config_.width;
    const uint32_t h = actual_config_.height;
    const size_t expected_bytes_32 = static_cast<size_t>(w) * h * 4;
    const size_t expected_bytes_64 = static_cast<size_t>(w) * h * 8;

    bool copied = false;
    if (actual_config_.output_format == common::PixelFormat::BGRA64) {
        if (out_frame.Allocate(w, h, common::PixelFormat::BGRA64)) {
            if (cur_len >= expected_bytes_64) {
                std::memcpy(out_frame.Data(), raw_data, expected_bytes_64);
                copied = true;
            } else if (cur_len >= expected_bytes_32) {
                common::ConvertBgra32ToBgra64(
                    raw_data,
                    w * 4,
                    reinterpret_cast<uint16_t*>(out_frame.Data()),
                    out_frame.Stride(),
                    w,
                    h);
                copied = true;
            }
            if (copied) {
                out_frame.View().timestamp_ns = static_cast<uint64_t>(ll_timestamp) * 100ull;
                out_frame.View().frame_index  = ++frame_counter_;
            }
        }
    } else if (cur_len >= expected_bytes_32 && out_frame.Allocate(w, h, common::PixelFormat::BGRA32)) {
        std::memcpy(out_frame.Data(), raw_data, expected_bytes_32);
        out_frame.View().timestamp_ns = static_cast<uint64_t>(ll_timestamp) * 100ull;
        out_frame.View().frame_index  = ++frame_counter_;
        copied = true;
    }

    buffer->Unlock();
    return copied;
}

void MfCameraCapture::Close() {
    is_open_ = false;
    reader_.Reset();
    if (mf_started_) {
        MFShutdown();
        mf_started_ = false;
    }
}

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam
