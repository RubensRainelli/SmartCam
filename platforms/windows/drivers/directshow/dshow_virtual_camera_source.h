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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_DSHOW_VIRTUAL_CAMERA_SOURCE_H_
#define SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_DSHOW_VIRTUAL_CAMERA_SOURCE_H_

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <objbase.h>
#include <strmif.h>
#include <amvideo.h>
#include <dvdmedia.h>
#include <uuids.h>
#include <vfwmsgs.h>
#include <wrl/client.h>

#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include "../../ipc/shared_frame_ipc.h"
#include "../media_foundation/mf_media_source.h"
#include "../media_foundation/smartcam_mf_guids.h"

namespace smartcam {
namespace platforms {
namespace windows {

class SmartCamDShowFilter;
class SmartCamDShowPin;

struct SmartCamFormatSpec {
    uint32_t width;
    uint32_t height;
    uint32_t fps;
    GUID     subtype; // MEDIASUBTYPE_RGB24, MEDIASUBTYPE_YUY2, MEDIASUBTYPE_RGB32
    uint16_t bit_count;
    DWORD    compression;
};

/**
 * @brief DirectShow Output Capture Pin (`IPin`, `IAMStreamConfig`, `IKsPropertySet`, `IQualityControl`)
 *        compatible with Chromium (`VideoCaptureDeviceFactoryWin` / `VideoCaptureDeviceWin`),
 *        Chrome, Edge, Brave, Firefox, OBS, Zoom, and Microsoft Teams.
 */
class SmartCamDShowPin final : public IPin,
                               public IAMStreamConfig,
                               public IKsPropertySet,
                               public IQualityControl {
public:
    explicit SmartCamDShowPin(SmartCamDShowFilter* parent_filter);
    ~SmartCamDShowPin();

    // IUnknown
    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override;
    STDMETHODIMP_(ULONG) AddRef() override;
    STDMETHODIMP_(ULONG) Release() override;

    // IPin
    STDMETHODIMP Connect(IPin* pReceivePin, const AM_MEDIA_TYPE* pmt) override;
    STDMETHODIMP ReceiveConnection(IPin* pConnector, const AM_MEDIA_TYPE* pmt) override;
    STDMETHODIMP Disconnect() override;
    STDMETHODIMP ConnectedTo(IPin** pPin) override;
    STDMETHODIMP ConnectionMediaType(AM_MEDIA_TYPE* pmt) override;
    STDMETHODIMP QueryPinInfo(PIN_INFO* pInfo) override;
    STDMETHODIMP QueryDirection(PIN_DIRECTION* pPinDir) override;
    STDMETHODIMP QueryId(LPWSTR* Id) override;
    STDMETHODIMP QueryAccept(const AM_MEDIA_TYPE* pmt) override;
    STDMETHODIMP EnumMediaTypes(IEnumMediaTypes** ppEnum) override;
    STDMETHODIMP QueryInternalConnections(IPin** apPin, ULONG* nPin) override;
    STDMETHODIMP EndOfStream() override;
    STDMETHODIMP BeginFlush() override;
    STDMETHODIMP EndFlush() override;
    STDMETHODIMP NewSegment(REFERENCE_TIME tStart, REFERENCE_TIME tStop, double dRate) override;

    // IAMStreamConfig
    STDMETHODIMP SetFormat(AM_MEDIA_TYPE* pmt) override;
    STDMETHODIMP GetFormat(AM_MEDIA_TYPE** ppmt) override;
    STDMETHODIMP GetNumberOfCapabilities(int* piCount, int* piSize) override;
    STDMETHODIMP GetStreamCaps(int iIndex, AM_MEDIA_TYPE** ppmt, BYTE* pSCC) override;

    // IKsPropertySet
    STDMETHODIMP Set(REFGUID guidPropSet, DWORD dwPropID, LPVOID pInstanceData, DWORD cbInstanceData,
                     LPVOID pPropData, DWORD cbPropData) override;
    STDMETHODIMP Get(REFGUID guidPropSet, DWORD dwPropID, LPVOID pInstanceData, DWORD cbInstanceData,
                     LPVOID pPropData, DWORD cbPropData, DWORD* pcbReturned) override;
    STDMETHODIMP QuerySupported(REFGUID guidPropSet, DWORD dwPropID, DWORD* pTypeSupport) override;

    // IQualityControl
    STDMETHODIMP Notify(IBaseFilter* pSelf, Quality q) override;
    STDMETHODIMP SetSink(IQualityControl* piqc) override;

    // Stream control invoked by SmartCamDShowFilter
    HRESULT OnFilterPause();
    HRESULT OnFilterRun(REFERENCE_TIME tStart);
    HRESULT OnFilterStop();

    std::vector<SmartCamFormatSpec> BuildFormatList() const;
    static bool FillMediaTypeFromSpec(const SmartCamFormatSpec& spec, AM_MEDIA_TYPE* mt);
    static void FreeMediaTypeFields(AM_MEDIA_TYPE& mt);
    static void DeleteMediaType(AM_MEDIA_TYPE* pmt);
    static HRESULT CopyMediaType(AM_MEDIA_TYPE* dst, const AM_MEDIA_TYPE* src);

private:
    void StreamingThreadLoop();
    void RenderFrameToSampleBuffer(uint8_t* dst, size_t dst_cap, uint32_t out_w, uint32_t out_h,
                                   bool bottom_up, const GUID& subtype);

    SmartCamDShowFilter*                   parent_filter_ = nullptr;
    mutable std::mutex                     mutex_;
    AM_MEDIA_TYPE                          current_mt_{};
    bool                                   has_custom_mt_ = false;
    Microsoft::WRL::ComPtr<IPin>           connected_pin_;
    Microsoft::WRL::ComPtr<IMemInputPin>   mem_input_pin_;
    Microsoft::WRL::ComPtr<IMemAllocator>  allocator_;
    SharedFrameIpc                         ipc_;
    std::vector<uint8_t>                   ipc_frame_scratch_;
    std::atomic<bool>                      streaming_{false};
    std::thread                            worker_thread_;
    REFERENCE_TIME                         run_start_time_ = 0;
    uint64_t                               sample_counter_ = 0;
};

/**
 * @brief Dual-Stack Virtual Camera COM Class implementing both DirectShow (`IBaseFilter`,
 *        `IAMFilterMiscFlags`) and Media Foundation (`IMFMediaSourceEx` delegation).
 */
class SmartCamDShowFilter final : public IBaseFilter,
                                  public IAMFilterMiscFlags {
public:
    SmartCamDShowFilter();
    ~SmartCamDShowFilter();

    HRESULT Initialize();

    // IUnknown
    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override;
    STDMETHODIMP_(ULONG) AddRef() override;
    STDMETHODIMP_(ULONG) Release() override;

    // IPersist
    STDMETHODIMP GetClassID(CLSID* pClassID) override;

    // IMediaFilter
    STDMETHODIMP Stop() override;
    STDMETHODIMP Pause() override;
    STDMETHODIMP Run(REFERENCE_TIME tStart) override;
    STDMETHODIMP GetState(DWORD dwMilliSecsTimeout, FILTER_STATE* State) override;
    STDMETHODIMP SetSyncSource(IReferenceClock* pClock) override;
    STDMETHODIMP GetSyncSource(IReferenceClock** pClock) override;

    // IBaseFilter
    STDMETHODIMP EnumPins(IEnumPins** ppEnum) override;
    STDMETHODIMP FindPin(LPCWSTR Id, IPin** ppPin) override;
    STDMETHODIMP QueryFilterInfo(FILTER_INFO* pInfo) override;
    STDMETHODIMP JoinFilterGraph(IFilterGraph* pGraph, LPCWSTR pName) override;
    STDMETHODIMP QueryVendorInfo(LPWSTR* pVendorInfo) override;

    // IAMFilterMiscFlags
    STDMETHODIMP_(ULONG) GetMiscFlags() override;

    SmartCamDShowPin* GetCapturePin() const noexcept { return capture_pin_; }
    HRESULT GetReferenceTime(REFERENCE_TIME* out_time);

private:
    volatile LONG                            ref_count_ = 1;
    mutable std::mutex                       mutex_;
    FILTER_STATE                             state_ = State_Stopped;
    IFilterGraph*                            graph_ = nullptr; // Weak reference per DirectShow spec
    std::wstring                             filter_name_ = L"SmartCam";
    Microsoft::WRL::ComPtr<IReferenceClock>  clock_;
    SmartCamDShowPin*                        capture_pin_ = nullptr;
    Microsoft::WRL::ComPtr<SmartCamMfMediaSource> mf_source_;
};

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_DSHOW_VIRTUAL_CAMERA_SOURCE_H_
