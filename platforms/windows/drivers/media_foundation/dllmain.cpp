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

#if !defined(_WIN32_WINNT) || (_WIN32_WINNT < 0x0A00)
#undef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#endif
#if !defined(WINVER) || (WINVER < 0x0A00)
#undef WINVER
#define WINVER 0x0A00
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <unknwn.h>
#include <new>
#include <string>

#include "../directshow/dshow_virtual_camera_source.h"
#include "mf_media_source.h"
#include "smartcam_mf_guids.h"

namespace {

HMODULE g_module_handle = nullptr;
volatile LONG g_server_locks = 0;

constexpr const wchar_t* kVideoInputCategoryClsid = L"{860BB310-5D01-11d0-BD3B-00A0C911CE86}";

// Standard DirectShow REGFILTER2 binary blob (1 Output Video Capture Pin, Merit 0x00200000)
constexpr BYTE kSmartCamFilterData[] = {
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x70, 0x69, 0x33, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x74, 0x79, 0x33, 0x00, 0x00, 0x00, 0x00,
    0x38, 0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00,
    0x76, 0x69, 0x64, 0x73, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
};

bool RegisterInRootHive(HKEY root_hive, const wchar_t* module_path) {
    const std::wstring clsid_key =
        std::wstring(L"SOFTWARE\\Classes\\CLSID\\") + smartcam::platforms::windows::kSmartCamClsidString;
    const std::wstring inproc_key = clsid_key + L"\\InprocServer32";
    const std::wstring dshow_inst_key =
        std::wstring(L"SOFTWARE\\Classes\\CLSID\\") + kVideoInputCategoryClsid +
        L"\\Instance\\" + smartcam::platforms::windows::kSmartCamClsidString;

    HKEY hKey = nullptr;
    LONG status = RegCreateKeyExW(
        root_hive, clsid_key.c_str(), 0, nullptr,
        REG_OPTION_NON_VOLATILE, KEY_WRITE | KEY_WOW64_64KEY, nullptr, &hKey, nullptr);
    if (status != ERROR_SUCCESS) {
        return false;
    }

    const wchar_t* desc = smartcam::platforms::windows::kSmartCamDefaultFriendlyName;
    RegSetValueExW(hKey, nullptr, 0, REG_SZ,
                   reinterpret_cast<const BYTE*>(desc),
                   static_cast<DWORD>((wcslen(desc) + 1) * sizeof(wchar_t)));
    RegCloseKey(hKey);

    status = RegCreateKeyExW(
        root_hive, inproc_key.c_str(), 0, nullptr,
        REG_OPTION_NON_VOLATILE, KEY_WRITE | KEY_WOW64_64KEY, nullptr, &hKey, nullptr);
    if (status != ERROR_SUCCESS) {
        return false;
    }

    RegSetValueExW(hKey, nullptr, 0, REG_SZ,
                   reinterpret_cast<const BYTE*>(module_path),
                   static_cast<DWORD>((wcslen(module_path) + 1) * sizeof(wchar_t)));

    const wchar_t* threading = L"Both";
    RegSetValueExW(hKey, L"ThreadingModel", 0, REG_SZ,
                   reinterpret_cast<const BYTE*>(threading),
                   static_cast<DWORD>((wcslen(threading) + 1) * sizeof(wchar_t)));
    RegCloseKey(hKey);

    // Register in DirectShow VideoInputDeviceCategory so Chromium/Edge/Firefox/OBS/Zoom/Teams enumerate "SmartCam"
    status = RegCreateKeyExW(
        root_hive, dshow_inst_key.c_str(), 0, nullptr,
        REG_OPTION_NON_VOLATILE, KEY_WRITE | KEY_WOW64_64KEY, nullptr, &hKey, nullptr);
    if (status == ERROR_SUCCESS) {
        RegSetValueExW(hKey, L"FriendlyName", 0, REG_SZ,
                       reinterpret_cast<const BYTE*>(desc),
                       static_cast<DWORD>((wcslen(desc) + 1) * sizeof(wchar_t)));
        const wchar_t* clsid_str = smartcam::platforms::windows::kSmartCamClsidString;
        RegSetValueExW(hKey, L"CLSID", 0, REG_SZ,
                       reinterpret_cast<const BYTE*>(clsid_str),
                       static_cast<DWORD>((wcslen(clsid_str) + 1) * sizeof(wchar_t)));
        const wchar_t* dev_path = L"smartcam://virtual-camera-0";
        RegSetValueExW(hKey, L"DevicePath", 0, REG_SZ,
                       reinterpret_cast<const BYTE*>(dev_path),
                       static_cast<DWORD>((wcslen(dev_path) + 1) * sizeof(wchar_t)));
        RegSetValueExW(hKey, L"FilterData", 0, REG_BINARY,
                       kSmartCamFilterData, static_cast<DWORD>(sizeof(kSmartCamFilterData)));
        RegCloseKey(hKey);
    }

    return true;
}

void UnregisterFromRootHive(HKEY root_hive) {
    const std::wstring clsid_key =
        std::wstring(L"SOFTWARE\\Classes\\CLSID\\") + smartcam::platforms::windows::kSmartCamClsidString;
    const std::wstring inproc_key = clsid_key + L"\\InprocServer32";
    const std::wstring dshow_inst_key =
        std::wstring(L"SOFTWARE\\Classes\\CLSID\\") + kVideoInputCategoryClsid +
        L"\\Instance\\" + smartcam::platforms::windows::kSmartCamClsidString;

    RegDeleteKeyW(root_hive, dshow_inst_key.c_str());
    RegDeleteKeyW(root_hive, inproc_key.c_str());
    RegDeleteKeyW(root_hive, clsid_key.c_str());
}

class SmartCamClassFactory final : public IClassFactory {
public:
    SmartCamClassFactory() = default;
    ~SmartCamClassFactory() = default;

    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IClassFactory) {
            *ppv = static_cast<IClassFactory*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }

    STDMETHODIMP_(ULONG) AddRef() override {
        return InterlockedIncrement(&ref_count_);
    }

    STDMETHODIMP_(ULONG) Release() override {
        const ULONG count = InterlockedDecrement(&ref_count_);
        if (count == 0) {
            delete this;
        }
        return count;
    }

    STDMETHODIMP CreateInstance(IUnknown* pUnkOuter, REFIID riid, void** ppv) override {
        if (pUnkOuter != nullptr) return CLASS_E_NOAGGREGATION;
        if (!ppv) return E_POINTER;
        *ppv = nullptr;

        auto* filter = new (std::nothrow) smartcam::platforms::windows::SmartCamDShowFilter();
        if (!filter) return E_OUTOFMEMORY;

        HRESULT hr = filter->Initialize();
        if (SUCCEEDED(hr)) {
            hr = filter->QueryInterface(riid, ppv);
        }
        filter->Release();
        return hr;
    }

    STDMETHODIMP LockServer(BOOL fLock) override {
        if (fLock) {
            InterlockedIncrement(&g_server_locks);
        } else {
            InterlockedDecrement(&g_server_locks);
        }
        return S_OK;
    }

private:
    volatile LONG ref_count_ = 1;
};

}  // namespace

BOOL APIENTRY DllMain(HMODULE hModule, DWORD ul_reason_for_call, LPVOID /*lpReserved*/) {
    if (ul_reason_for_call == DLL_PROCESS_ATTACH) {
        g_module_handle = hModule;
        DisableThreadLibraryCalls(hModule);
    }
    return TRUE;
}

extern "C" HRESULT STDAPICALLTYPE DllGetClassObject(REFCLSID rclsid, REFIID riid, LPVOID* ppv) {
    if (!ppv) return E_POINTER;
    *ppv = nullptr;

    if (rclsid != CLSID_SmartCamMfVirtualCameraSource) {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    auto* factory = new (std::nothrow) SmartCamClassFactory();
    if (!factory) return E_OUTOFMEMORY;

    const HRESULT hr = factory->QueryInterface(riid, ppv);
    factory->Release();
    return hr;
}

extern "C" HRESULT STDAPICALLTYPE DllCanUnloadNow() {
    return (g_server_locks == 0) ? S_OK : S_FALSE;
}

extern "C" HRESULT STDAPICALLTYPE DllRegisterServer() {
    wchar_t module_path[MAX_PATH] = {0};
    if (!GetModuleFileNameW(g_module_handle, module_path, MAX_PATH)) {
        return HRESULT_FROM_WIN32(GetLastError());
    }

    const bool ok_hkcu = RegisterInRootHive(HKEY_CURRENT_USER, module_path);
    const bool ok_hklm = RegisterInRootHive(HKEY_LOCAL_MACHINE, module_path);

    return (ok_hkcu || ok_hklm) ? S_OK : E_ACCESSDENIED;
}

extern "C" HRESULT STDAPICALLTYPE DllUnregisterServer() {
    UnregisterFromRootHive(HKEY_CURRENT_USER);
    UnregisterFromRootHive(HKEY_LOCAL_MACHINE);
    return S_OK;
}
