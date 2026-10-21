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

#include "mf_virtual_camera_sink.h"
#include <devpropdef.h>
#include <ks.h>
#include <ksmedia.h>
#include <string>

#if defined(_MSC_VER)
#pragma comment(lib, "mfplat.lib")
#pragma comment(lib, "mfuuid.lib")
#pragma comment(lib, "ole32.lib")
#endif

// Windows 11 21H2+ (Build 22000+) IMFVirtualCamera ABI definition
// Declared at global scope so both MSVC MIDL_INTERFACE and MinGW __CRT_UUID_DECL work seamlessly.
MIDL_INTERFACE("1C08A864-EF6C-4C75-AF59-5F2D68DA9563")
IMFVirtualCamera_SmartCamABI : public IMFAttributes {
public:
    virtual HRESULT STDMETHODCALLTYPE AddDeviceSourceInfo(LPCWSTR DeviceSourceInfo) = 0;
    virtual HRESULT STDMETHODCALLTYPE AddProperty(const DEVPROPKEY* pKey, DEVPROPTYPE Type, const BYTE* pbData, ULONG cbData) = 0;
    virtual HRESULT STDMETHODCALLTYPE AddRegistryEntry(LPCWSTR EntryName, LPCWSTR SubkeyPath, DWORD dwRegType, const BYTE* pbData, ULONG cbData) = 0;
    virtual HRESULT STDMETHODCALLTYPE Start(IMFAsyncCallback* pCallback) = 0;
    virtual HRESULT STDMETHODCALLTYPE Stop(void) = 0;
    virtual HRESULT STDMETHODCALLTYPE Remove(void) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetMediaSource(IMFMediaSource** ppMediaSource) = 0;
    virtual HRESULT STDMETHODCALLTYPE SendCameraProperty(REFGUID propertySet, ULONG propertyId, ULONG propertyFlags, void* propertyPayload, ULONG propertyPayloadLength, void* data, ULONG dataLength, ULONG* dataWritten) = 0;
    virtual HRESULT STDMETHODCALLTYPE CreateSyncEvent(REFGUID kseventSet, ULONG kseventId, ULONG kseventFlags, HANDLE eventHandle, IUnknown** cameraSyncObject) = 0;
    virtual HRESULT STDMETHODCALLTYPE CreateSyncSemaphore(REFGUID kseventSet, ULONG kseventId, ULONG kseventFlags, HANDLE semaphoreHandle, LONG semaphoreAdjustment, IUnknown** cameraSyncObject) = 0;
    virtual HRESULT STDMETHODCALLTYPE Shutdown(void) = 0;
};
#ifdef __CRT_UUID_DECL
__CRT_UUID_DECL(IMFVirtualCamera_SmartCamABI, 0x1c08a864, 0xef6c, 0x4c75, 0xaf, 0x59, 0x5f, 0x2d, 0x68, 0xda, 0x95, 0x63)
#endif

namespace smartcam {
namespace platforms {
namespace windows {

namespace {

// Windows 11 21H2+ (Build 22000+) MFCreateVirtualCamera ABI definitions
// Dynamically resolved from mfvirtualcamera.dll / mfplat.dll so compilation succeeds on any Windows 10/11 SDK
enum MFVirtualCameraType_ABI {
    MFVirtualCameraType_SoftwareCameraSource_ABI = 0
};

enum MFVirtualCameraLifetime_ABI {
    MFVirtualCameraLifetime_Session_ABI = 0,
    MFVirtualCameraLifetime_System_ABI  = 1
};

enum MFVirtualCameraAccess_ABI {
    MFVirtualCameraAccess_CurrentUser_ABI = 0,
    MFVirtualCameraAccess_AllUsers_ABI    = 1
};

using PFN_MFCreateVirtualCamera = HRESULT(STDAPICALLTYPE*)(
    MFVirtualCameraType_ABI type,
    MFVirtualCameraLifetime_ABI lifetime,
    MFVirtualCameraAccess_ABI access,
    LPCWSTR friendlyName,
    LPCWSTR sourceId,
    const GUID* categories,
    ULONG categoryCount,
    IMFVirtualCamera_SmartCamABI** virtualCamera);

std::wstring Utf8ToWide(const std::string& str) {
    if (str.empty()) return std::wstring(kSmartCamDefaultFriendlyName);
    const int needed = MultiByteToWideChar(CP_UTF8, 0, str.c_str(), static_cast<int>(str.size()), nullptr, 0);
    if (needed <= 0) return std::wstring(kSmartCamDefaultFriendlyName);
    std::wstring out(static_cast<size_t>(needed), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, str.c_str(), static_cast<int>(str.size()), &out[0], needed);
    return out;
}

}  // namespace

bool MfVirtualCameraSink::Initialize(const common::VirtualCameraConfig& config) {
    Shutdown();
    config_ = config;

    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(hr) && hr != RPC_E_CHANGED_MODE) {
        state_ = common::VirtualCameraState::ERROR_STATE;
        return false;
    }

    hr = MFStartup(MF_VERSION);
    if (FAILED(hr)) {
        state_ = common::VirtualCameraState::ERROR_STATE;
        return false;
    }
    mf_started_ = true;

    // 1. Initialize User-Mode Named Shared Memory IPC for frame transport
    if (!ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, true)) {
        Shutdown();
        state_ = common::VirtualCameraState::ERROR_STATE;
        return false;
    }

    // 2. Attempt to bind Windows 11 Native MFCreateVirtualCamera API
    vcam_dll_ = LoadLibraryW(L"mfvirtualcamera.dll");
    if (!vcam_dll_) {
        vcam_dll_ = LoadLibraryW(L"mfplat.dll");
    }

    if (vcam_dll_) {
        auto pfn_create_vcam = reinterpret_cast<PFN_MFCreateVirtualCamera>(
            reinterpret_cast<void*>(GetProcAddress(vcam_dll_, "MFCreateVirtualCamera")));

        if (pfn_create_vcam) {
            const std::wstring wide_name = Utf8ToWide(config_.friendly_name);
            // KSCATEGORY_VIDEO_CAMERA {E5323777-F976-4f5b-9B55-B94699C46E44}
            constexpr GUID kKsCategoryVideoCamera = {
                0xe5323777, 0xf976, 0x4f5b, {0x9b, 0x55, 0xb9, 0x46, 0x99, 0xc4, 0x6e, 0x44}
            };
            Microsoft::WRL::ComPtr<IMFVirtualCamera_SmartCamABI> vcam;

            hr = pfn_create_vcam(
                MFVirtualCameraType_SoftwareCameraSource_ABI,
                MFVirtualCameraLifetime_Session_ABI,
                MFVirtualCameraAccess_CurrentUser_ABI,
                wide_name.c_str(),
                kSmartCamClsidString,
                &kKsCategoryVideoCamera,
                1,
                &vcam);

            if (SUCCEEDED(hr) && vcam) {
                hr = vcam->Start(nullptr);
                if (SUCCEEDED(hr)) {
                    vcam_handle_ = vcam;
                }
            }
        }
    }

    state_ = common::VirtualCameraState::READY;
    return true;
}

bool MfVirtualCameraSink::PushFrame(const common::FrameView& frame) {
    if (!frame.IsValid() || !ipc_.IsOpen()) {
        return false;
    }
    if (frame.format != common::PixelFormat::BGRA32 && frame.format != common::PixelFormat::RGBA32) {
        return false;
    }

    // Convert nanosecond timestamp to Media Foundation 100-ns units
    uint64_t ts_100ns = frame.timestamp_ns / 100ull;
    if (ts_100ns == 0) {
        LARGE_INTEGER qpc{}, freq{};
        QueryPerformanceCounter(&qpc);
        QueryPerformanceFrequency(&freq);
        if (freq.QuadPart > 0) {
            ts_100ns = static_cast<uint64_t>((qpc.QuadPart * 10000000ull) / freq.QuadPart);
        }
    }

    const bool ok = ipc_.WriteFrame(
        frame.data,
        frame.width,
        frame.height,
        frame.stride,
        config_.fps_num,
        config_.fps_den,
        ts_100ns,
        ++frame_count_);

    if (ok) {
        state_ = common::VirtualCameraState::STREAMING;
    }
    return ok;
}

void MfVirtualCameraSink::Shutdown() {
    if (vcam_handle_) {
        Microsoft::WRL::ComPtr<IMFVirtualCamera_SmartCamABI> vcam;
        if (SUCCEEDED(vcam_handle_.As(&vcam)) && vcam) {
            vcam->Stop();
            vcam->Shutdown();
        }
        vcam_handle_.Reset();
    }

    if (vcam_dll_) {
        FreeLibrary(vcam_dll_);
        vcam_dll_ = nullptr;
    }

    ipc_.Close();

    if (mf_started_) {
        MFShutdown();
        mf_started_ = false;
    }

    state_ = common::VirtualCameraState::STOPPED;
}

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam
