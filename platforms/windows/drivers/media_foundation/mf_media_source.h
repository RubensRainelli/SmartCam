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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_MEDIA_SOURCE_H_
#define SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_MEDIA_SOURCE_H_

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
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mferror.h>
#include <mfobjects.h>
#include <wrl/client.h>
#include <mutex>

#include "../../ipc/shared_frame_ipc.h"
#include "smartcam_mf_guids.h"

namespace smartcam {
namespace platforms {
namespace windows {

class SmartCamMfMediaStream;

/**
 * @brief Windows 11 Media Foundation COM Virtual Camera MediaSource (`IMFMediaSourceEx`).
 *
 * Instantiated by the Windows Camera Frame Server (`svchost.exe -k Camera`) or client browsers
 * (Chrome, Edge, Teams, WebRTC) when opening `"SmartCam Virtual Camera"`.
 * Reads processed frames from the User-Mode Shared Memory channel (`SharedFrameIpc`).
 */
class SmartCamMfMediaSource final : public IMFMediaSourceEx, public IMFGetService {
public:
    SmartCamMfMediaSource();
    virtual ~SmartCamMfMediaSource();

    HRESULT Initialize();

    // IUnknown
    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override;
    STDMETHODIMP_(ULONG) AddRef() override;
    STDMETHODIMP_(ULONG) Release() override;

    // IMFMediaEventGenerator
    STDMETHODIMP GetEvent(DWORD dwFlags, IMFMediaEvent** ppEvent) override;
    STDMETHODIMP BeginGetEvent(IMFAsyncCallback* pCallback, IUnknown* punkState) override;
    STDMETHODIMP EndGetEvent(IMFAsyncResult* pResult, IMFMediaEvent** ppEvent) override;
    STDMETHODIMP QueueEvent(MediaEventType met, REFGUID guidExtendedType, HRESULT hrStatus, const PROPVARIANT* pvValue) override;

    // IMFMediaSource
    STDMETHODIMP GetCharacteristics(DWORD* pdwCharacteristics) override;
    STDMETHODIMP CreatePresentationDescriptor(IMFPresentationDescriptor** ppPresentationDescriptor) override;
    STDMETHODIMP Start(IMFPresentationDescriptor* pPresentationDescriptor, const GUID* pguidTimeFormat, const PROPVARIANT* pvarStartPosition) override;
    STDMETHODIMP Stop() override;
    STDMETHODIMP Pause() override;
    STDMETHODIMP Shutdown() override;

    // IMFMediaSourceEx
    STDMETHODIMP GetSourceAttributes(IMFAttributes** ppAttributes) override;
    STDMETHODIMP GetStreamAttributes(DWORD dwStreamIdentifier, IMFAttributes** ppAttributes) override;
    STDMETHODIMP SetD3DManager(IUnknown* pManager) override;

    // IMFGetService
    STDMETHODIMP GetService(REFGUID guidService, REFIID riid, LPVOID* ppvObject) override;

private:
    volatile LONG                                      ref_count_ = 1;
    mutable std::mutex                                 mutex_;
    Microsoft::WRL::ComPtr<IMFMediaEventQueue>         event_queue_;
    Microsoft::WRL::ComPtr<IMFPresentationDescriptor>  pres_descriptor_;
    Microsoft::WRL::ComPtr<IMFAttributes>              source_attributes_;
    Microsoft::WRL::ComPtr<SmartCamMfMediaStream>      video_stream_;
    bool                                               is_shutdown_ = false;
};

/**
 * @brief Media Foundation video stream (`IMFMediaStream`) delivering `MFVideoFormat_RGB32` (`BGRA32`) samples.
 */
class SmartCamMfMediaStream final : public IMFMediaStream {
public:
    SmartCamMfMediaStream(SmartCamMfMediaSource* parent, IMFStreamDescriptor* stream_desc);
    virtual ~SmartCamMfMediaStream();

    HRESULT Initialize();
    HRESULT StartStream(const PROPVARIANT* start_pos);
    HRESULT StopStream();
    HRESULT PauseStream();
    HRESULT ShutdownStream();
    IMFAttributes* GetAttributesRaw() const noexcept { return stream_attributes_.Get(); }

    // IUnknown
    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override;
    STDMETHODIMP_(ULONG) AddRef() override;
    STDMETHODIMP_(ULONG) Release() override;

    // IMFMediaEventGenerator
    STDMETHODIMP GetEvent(DWORD dwFlags, IMFMediaEvent** ppEvent) override;
    STDMETHODIMP BeginGetEvent(IMFAsyncCallback* pCallback, IUnknown* punkState) override;
    STDMETHODIMP EndGetEvent(IMFAsyncResult* pResult, IMFMediaEvent** ppEvent) override;
    STDMETHODIMP QueueEvent(MediaEventType met, REFGUID guidExtendedType, HRESULT hrStatus, const PROPVARIANT* pvValue) override;

    // IMFMediaStream
    STDMETHODIMP GetMediaSource(IMFMediaSource** ppMediaSource) override;
    STDMETHODIMP GetStreamDescriptor(IMFStreamDescriptor** ppStreamDescriptor) override;
    STDMETHODIMP RequestSample(IUnknown* pToken) override;

private:
    void RenderStandbyPattern(uint8_t* dst, uint32_t width, uint32_t height) noexcept;

    volatile LONG                                     ref_count_ = 1;
    mutable std::mutex                                mutex_;
    SmartCamMfMediaSource*                            parent_source_ = nullptr;
    Microsoft::WRL::ComPtr<IMFMediaEventQueue>        event_queue_;
    Microsoft::WRL::ComPtr<IMFStreamDescriptor>       stream_descriptor_;
    Microsoft::WRL::ComPtr<IMFAttributes>             stream_attributes_;
    SharedFrameIpc                                    ipc_;
    uint32_t                                          width_        = 1440;
    uint32_t                                          height_       = 1080;
    uint64_t                                          sample_count_ = 0;
    bool                                              is_shutdown_  = false;
};

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_MEDIA_SOURCE_H_
