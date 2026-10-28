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

#include "dshow_virtual_camera_source.h"

#include <algorithm>
#include <chrono>
#include <cstring>
#include <new>

namespace smartcam {
namespace platforms {
namespace windows {

namespace {

// {FB6C4281-0353-11D1-905F-0000C0CC16BA} PIN_CATEGORY_CAPTURE
constexpr GUID kPinCategoryCapture = {
    0xfb6c4281, 0x0353, 0x11d1, {0x90, 0x5f, 0x00, 0x00, 0xc0, 0xcc, 0x16, 0xba}
};

// {9B00F101-1567-11D1-B3F1-00AA003761C5} AMPROPSETID_Pin
constexpr GUID kAmPropSetIdPin = {
    0x9b00f101, 0x1567, 0x11d1, {0xb3, 0xf1, 0x00, 0xaa, 0x00, 0x37, 0x61, 0xc5}
};

// {32595559-0000-0010-8000-00AA00389B71} MEDIASUBTYPE_YUY2
constexpr GUID kMediaSubtypeYUY2 = {
    0x32595559, 0x0000, 0x0010, {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}
};

// {E436EB7D-524F-11CE-9F53-0020AF0BA770} MEDIASUBTYPE_RGB24
constexpr GUID kMediaSubtypeRGB24 = {
    0xe436eb7d, 0x524f, 0x11ce, {0x9f, 0x53, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70}
};

// {E436EB7E-524F-11CE-9F53-0020AF0BA770} MEDIASUBTYPE_RGB32
constexpr GUID kMediaSubtypeRGB32 = {
    0xe436eb7e, 0x524f, 0x11ce, {0x9f, 0x53, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70}
};

// {73646976-0000-0010-8000-00AA00389B71} MEDIATYPE_Video
constexpr GUID kMediaTypeVideo = {
    0x73646976, 0x0000, 0x0010, {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}
};

// {05589F80-C356-11CE-BF01-00AA0055595A} FORMAT_VideoInfo
constexpr GUID kFormatVideoInfo = {
    0x05589f80, 0xc356, 0x11ce, {0xbf, 0x01, 0x00, 0xaa, 0x00, 0x55, 0x59, 0x5a}
};

// {1E651CC0-B199-11D0-8212-00C04FC32C45} CLSID_MemoryAllocator
constexpr GUID kClsidMemoryAllocator = {
    0x1e651cc0, 0xb199, 0x11d0, {0x82, 0x12, 0x00, 0xc0, 0x4f, 0xc3, 0x2c, 0x45}
};

constexpr DWORD kFourCC_YUY2 = 0x32595559u; // 'YUY2'

LPWSTR AllocCoTaskWideString(const wchar_t* src) {
    if (!src) return nullptr;
    const size_t bytes = (wcslen(src) + 1) * sizeof(wchar_t);
    auto* dst = static_cast<LPWSTR>(CoTaskMemAlloc(bytes));
    if (dst) {
        std::memcpy(dst, src, bytes);
    }
    return dst;
}

// ============================================================================
// Self-Contained Fallback IMediaSample & IMemAllocator
// ============================================================================

class SmartCamMediaSample final : public IMediaSample {
public:
    explicit SmartCamMediaSample(size_t capacity)
        : buffer_(capacity, 0), actual_len_(static_cast<LONG>(capacity)) {}
    ~SmartCamMediaSample() = default;

    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IMediaSample) {
            *ppv = static_cast<IMediaSample*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }

    STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&ref_count_); }
    STDMETHODIMP_(ULONG) Release() override {
        const ULONG c = InterlockedDecrement(&ref_count_);
        if (c == 0) delete this;
        return c;
    }

    STDMETHODIMP GetPointer(BYTE** ppBuffer) override {
        if (!ppBuffer) return E_POINTER;
        *ppBuffer = buffer_.data();
        return S_OK;
    }

    STDMETHODIMP_(LONG) GetSize() override { return static_cast<LONG>(buffer_.size()); }

    STDMETHODIMP GetTime(REFERENCE_TIME* pTimeStart, REFERENCE_TIME* pTimeEnd) override {
        if (!pTimeStart || !pTimeEnd) return E_POINTER;
        if (!has_time_) return VFW_E_SAMPLE_TIME_NOT_SET;
        *pTimeStart = time_start_;
        *pTimeEnd   = time_end_;
        return S_OK;
    }

    STDMETHODIMP SetTime(REFERENCE_TIME* pTimeStart, REFERENCE_TIME* pTimeEnd) override {
        if (!pTimeStart) {
            has_time_ = false;
            return S_OK;
        }
        time_start_ = *pTimeStart;
        time_end_   = pTimeEnd ? *pTimeEnd : (*pTimeStart + 333333LL);
        has_time_   = true;
        return S_OK;
    }

    STDMETHODIMP IsSyncPoint() override { return sync_point_ ? S_OK : S_FALSE; }
    STDMETHODIMP SetSyncPoint(BOOL bIsSyncPoint) override {
        sync_point_ = (bIsSyncPoint != FALSE);
        return S_OK;
    }

    STDMETHODIMP IsPreroll() override { return preroll_ ? S_OK : S_FALSE; }
    STDMETHODIMP SetPreroll(BOOL bIsPreroll) override {
        preroll_ = (bIsPreroll != FALSE);
        return S_OK;
    }

    STDMETHODIMP_(LONG) GetActualDataLength() override { return actual_len_; }
    STDMETHODIMP SetActualDataLength(LONG lActual) override {
        if (lActual < 0 || static_cast<size_t>(lActual) > buffer_.size()) {
            return VFW_E_BUFFER_OVERFLOW;
        }
        actual_len_ = lActual;
        return S_OK;
    }

    STDMETHODIMP GetMediaType(AM_MEDIA_TYPE** ppMediaType) override {
        if (!ppMediaType) return E_POINTER;
        *ppMediaType = nullptr;
        return S_FALSE;
    }

    STDMETHODIMP SetMediaType(AM_MEDIA_TYPE* /*pMediaType*/) override { return S_OK; }

    STDMETHODIMP IsDiscontinuity() override { return discontinuity_ ? S_OK : S_FALSE; }
    STDMETHODIMP SetDiscontinuity(BOOL bDiscontinuity) override {
        discontinuity_ = (bDiscontinuity != FALSE);
        return S_OK;
    }

    STDMETHODIMP GetMediaTime(LONGLONG* pTimeStart, LONGLONG* pTimeEnd) override {
        if (!pTimeStart || !pTimeEnd) return E_POINTER;
        if (!has_media_time_) return VFW_E_MEDIA_TIME_NOT_SET;
        *pTimeStart = media_start_;
        *pTimeEnd   = media_end_;
        return S_OK;
    }

    STDMETHODIMP SetMediaTime(LONGLONG* pTimeStart, LONGLONG* pTimeEnd) override {
        if (!pTimeStart) {
            has_media_time_ = false;
            return S_OK;
        }
        media_start_    = *pTimeStart;
        media_end_      = pTimeEnd ? *pTimeEnd : (*pTimeStart + 1);
        has_media_time_ = true;
        return S_OK;
    }

    ULONG GetRefCount() const noexcept {
        return static_cast<ULONG>(InterlockedCompareExchange(&ref_count_, 0, 0));
    }

    void EnsureCapacityAndReset(size_t capacity) {
        if (buffer_.size() != capacity) {
            buffer_.assign(capacity, 0);
        }
        actual_len_     = static_cast<LONG>(capacity);
        has_time_       = false;
        has_media_time_ = false;
        sync_point_     = true;
        preroll_        = false;
        discontinuity_  = false;
    }

private:
    mutable volatile LONG ref_count_ = 1;
    std::vector<uint8_t> buffer_;
    LONG                 actual_len_ = 0;
    REFERENCE_TIME       time_start_ = 0;
    REFERENCE_TIME       time_end_ = 0;
    LONGLONG             media_start_ = 0;
    LONGLONG             media_end_ = 0;
    bool                 has_time_ = false;
    bool                 has_media_time_ = false;
    bool                 sync_point_ = true;
    bool                 preroll_ = false;
    bool                 discontinuity_ = false;
};

class SmartCamMemAllocator final : public IMemAllocator {
public:
    SmartCamMemAllocator() = default;
    ~SmartCamMemAllocator() {
        ClearPoolLocked();
    }

    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IMemAllocator) {
            *ppv = static_cast<IMemAllocator*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }

    STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&ref_count_); }
    STDMETHODIMP_(ULONG) Release() override {
        const ULONG c = InterlockedDecrement(&ref_count_);
        if (c == 0) delete this;
        return c;
    }

    STDMETHODIMP SetProperties(ALLOCATOR_PROPERTIES* pRequest, ALLOCATOR_PROPERTIES* pActual) override {
        if (!pRequest || !pActual) return E_POINTER;
        std::lock_guard<std::mutex> lock(mutex_);
        props_.cBuffers = (pRequest->cBuffers > 0) ? std::clamp<long>(pRequest->cBuffers, 1, 3) : 3;
        props_.cbBuffer = (pRequest->cbBuffer > 0) ? pRequest->cbBuffer : (1920 * 1080 * 4);
        props_.cbAlign  = (pRequest->cbAlign > 0) ? pRequest->cbAlign : 4;
        props_.cbPrefix = (pRequest->cbPrefix >= 0) ? pRequest->cbPrefix : 0;
        *pActual        = props_;
        return S_OK;
    }

    STDMETHODIMP GetProperties(ALLOCATOR_PROPERTIES* pProps) override {
        if (!pProps) return E_POINTER;
        std::lock_guard<std::mutex> lock(mutex_);
        *pProps = props_;
        return S_OK;
    }

    STDMETHODIMP Commit() override {
        std::lock_guard<std::mutex> lock(mutex_);
        committed_ = true;
        const size_t buf_size = static_cast<size_t>(props_.cbBuffer > 0 ? props_.cbBuffer : (1920 * 1080 * 4));
        const size_t max_pool = static_cast<size_t>(std::clamp<long>(props_.cBuffers, 1, 3));
        while (pool_.size() < max_pool) {
            auto* s = new (std::nothrow) SmartCamMediaSample(buf_size);
            if (!s) break;
            pool_.push_back(s);
        }
        return S_OK;
    }

    STDMETHODIMP Decommit() override {
        std::lock_guard<std::mutex> lock(mutex_);
        committed_ = false;
        ClearPoolLocked();
        return S_OK;
    }

    STDMETHODIMP GetBuffer(IMediaSample** ppBuffer, REFERENCE_TIME* /*pStartTime*/,
                           REFERENCE_TIME* /*pEndTime*/, DWORD /*dwFlags*/) override {
        if (!ppBuffer) return E_POINTER;
        *ppBuffer = nullptr;
        std::lock_guard<std::mutex> lock(mutex_);
        if (!committed_) return VFW_E_NOT_COMMITTED;
        const size_t buf_size = static_cast<size_t>(props_.cbBuffer > 0 ? props_.cbBuffer : (1920 * 1080 * 4));
        const size_t max_pool = static_cast<size_t>(std::clamp<long>(props_.cBuffers, 1, 3));

        // 1. Reuse an existing idle sample from the fixed-size pool (ref_count == 1 means only pool holds it)
        for (SmartCamMediaSample* sample : pool_) {
            if (sample && sample->GetRefCount() == 1) {
                sample->EnsureCapacityAndReset(buf_size);
                sample->AddRef();
                *ppBuffer = sample;
                return S_OK;
            }
        }

        // 2. Grow pool up to strict max_pool (max 3 frames) if not yet full
        if (pool_.size() < max_pool) {
            auto* sample = new (std::nothrow) SmartCamMediaSample(buf_size);
            if (!sample) return E_OUTOFMEMORY;
            sample->AddRef(); // 1 ref for pool_, 1 ref for caller
            pool_.push_back(sample);
            *ppBuffer = sample;
            return S_OK;
        }

        // 3. Backpressure: all 3 pooled buffers are currently held downstream; drop frame rather than leaking VirtualAlloc
        return VFW_E_TIMEOUT;
    }

    STDMETHODIMP ReleaseBuffer(IMediaSample* /*pBuffer*/) override { return S_OK; }

private:
    void ClearPoolLocked() noexcept {
        for (SmartCamMediaSample* s : pool_) {
            if (s) s->Release();
        }
        pool_.clear();
    }

    volatile LONG                     ref_count_ = 1;
    std::mutex                        mutex_;
    ALLOCATOR_PROPERTIES              props_{3, 1920 * 1080 * 4, 4, 0};
    std::vector<SmartCamMediaSample*> pool_;
    bool                              committed_ = false;
};

// ============================================================================
// IEnumPins Implementation
// ============================================================================

class SmartCamDShowEnumPins final : public IEnumPins {
public:
    SmartCamDShowEnumPins(SmartCamDShowPin* pin, ULONG index = 0)
        : pin_(pin), index_(index) {
        if (pin_) pin_->AddRef();
    }

    ~SmartCamDShowEnumPins() {
        if (pin_) pin_->Release();
    }

    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IEnumPins) {
            *ppv = static_cast<IEnumPins*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }

    STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&ref_count_); }
    STDMETHODIMP_(ULONG) Release() override {
        const ULONG c = InterlockedDecrement(&ref_count_);
        if (c == 0) delete this;
        return c;
    }

    STDMETHODIMP Next(ULONG cPins, IPin** ppPins, ULONG* pcFetched) override {
        if (!ppPins) return E_POINTER;
        if (cPins > 1 && !pcFetched) return E_INVALIDARG;

        ULONG fetched = 0;
        while (fetched < cPins && index_ < 1 && pin_) {
            ppPins[fetched] = static_cast<IPin*>(pin_);
            ppPins[fetched]->AddRef();
            ++fetched;
            ++index_;
        }
        if (pcFetched) *pcFetched = fetched;
        return (fetched == cPins) ? S_OK : S_FALSE;
    }

    STDMETHODIMP Skip(ULONG cPins) override {
        index_ += cPins;
        return (index_ <= 1) ? S_OK : S_FALSE;
    }

    STDMETHODIMP Reset() override {
        index_ = 0;
        return S_OK;
    }

    STDMETHODIMP Clone(IEnumPins** ppEnum) override {
        if (!ppEnum) return E_POINTER;
        auto* copy = new (std::nothrow) SmartCamDShowEnumPins(pin_, index_);
        if (!copy) return E_OUTOFMEMORY;
        *ppEnum = copy;
        return S_OK;
    }

private:
    volatile LONG     ref_count_ = 1;
    SmartCamDShowPin* pin_ = nullptr;
    ULONG             index_ = 0;
};

// ============================================================================
// IEnumMediaTypes Implementation
// ============================================================================

class SmartCamDShowEnumMediaTypes final : public IEnumMediaTypes {
public:
    SmartCamDShowEnumMediaTypes(const SmartCamDShowPin* pin, size_t index = 0)
        : specs_(pin ? pin->BuildFormatList() : std::vector<SmartCamFormatSpec>{}),
          index_(index) {}

    SmartCamDShowEnumMediaTypes(const std::vector<SmartCamFormatSpec>& specs, size_t index)
        : specs_(specs), index_(index) {}

    ~SmartCamDShowEnumMediaTypes() = default;

    STDMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IEnumMediaTypes) {
            *ppv = static_cast<IEnumMediaTypes*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }

    STDMETHODIMP_(ULONG) AddRef() override { return InterlockedIncrement(&ref_count_); }
    STDMETHODIMP_(ULONG) Release() override {
        const ULONG c = InterlockedDecrement(&ref_count_);
        if (c == 0) delete this;
        return c;
    }

    STDMETHODIMP Next(ULONG cMediaTypes, AM_MEDIA_TYPE** ppMediaTypes, ULONG* pcFetched) override {
        if (!ppMediaTypes) return E_POINTER;
        if (cMediaTypes > 1 && !pcFetched) return E_INVALIDARG;

        ULONG fetched = 0;
        while (fetched < cMediaTypes && index_ < specs_.size()) {
            auto* mt = static_cast<AM_MEDIA_TYPE*>(CoTaskMemAlloc(sizeof(AM_MEDIA_TYPE)));
            if (!mt) break;
            std::memset(mt, 0, sizeof(AM_MEDIA_TYPE));
            if (!SmartCamDShowPin::FillMediaTypeFromSpec(specs_[index_], mt)) {
                CoTaskMemFree(mt);
                break;
            }
            ppMediaTypes[fetched] = mt;
            ++fetched;
            ++index_;
        }
        if (pcFetched) *pcFetched = fetched;
        return (fetched == cMediaTypes) ? S_OK : S_FALSE;
    }

    STDMETHODIMP Skip(ULONG cMediaTypes) override {
        index_ += cMediaTypes;
        return (index_ <= specs_.size()) ? S_OK : S_FALSE;
    }

    STDMETHODIMP Reset() override {
        index_ = 0;
        return S_OK;
    }

    STDMETHODIMP Clone(IEnumMediaTypes** ppEnum) override {
        if (!ppEnum) return E_POINTER;
        auto* copy = new (std::nothrow) SmartCamDShowEnumMediaTypes(specs_, index_);
        if (!copy) return E_OUTOFMEMORY;
        *ppEnum = copy;
        return S_OK;
    }

private:
    volatile LONG                   ref_count_ = 1;
    std::vector<SmartCamFormatSpec> specs_;
    size_t                          index_ = 0;
};

}  // namespace

// ============================================================================
// SmartCamDShowPin Implementation
// ============================================================================

SmartCamDShowPin::SmartCamDShowPin(SmartCamDShowFilter* parent_filter)
    : parent_filter_(parent_filter) {
    std::memset(&current_mt_, 0, sizeof(current_mt_));
    ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
    const auto specs = BuildFormatList();
    if (!specs.empty()) {
        FillMediaTypeFromSpec(specs[0], &current_mt_);
    }
}

SmartCamDShowPin::~SmartCamDShowPin() {
    OnFilterStop();
    FreeMediaTypeFields(current_mt_);
    ipc_.Close();
}

STDMETHODIMP SmartCamDShowPin::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) return E_POINTER;
    if (riid == IID_IUnknown || riid == IID_IPin) {
        *ppv = static_cast<IPin*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IAMStreamConfig) {
        *ppv = static_cast<IAMStreamConfig*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IKsPropertySet) {
        *ppv = static_cast<IKsPropertySet*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IQualityControl) {
        *ppv = static_cast<IQualityControl*>(this);
        AddRef();
        return S_OK;
    }
    *ppv = nullptr;
    return E_NOINTERFACE;
}

STDMETHODIMP_(ULONG) SmartCamDShowPin::AddRef() {
    return parent_filter_ ? parent_filter_->AddRef() : 1;
}

STDMETHODIMP_(ULONG) SmartCamDShowPin::Release() {
    return parent_filter_ ? parent_filter_->Release() : 1;
}

std::vector<SmartCamFormatSpec> SmartCamDShowPin::BuildFormatList() const {
    uint32_t active_w = 1440;
    uint32_t active_h = 1080;
    uint32_t active_fps = 30;

    if (ipc_.IsOpen()) {
        const SharedFrameHeader* hdr = ipc_.Header();
        if (hdr && hdr->magic == SharedFrameHeader::kMagic &&
            hdr->width >= 120 && hdr->width <= 3840 &&
            hdr->height >= 120 && hdr->height <= 2160) {
            active_w   = hdr->width & ~1u;
            active_h   = hdr->height & ~1u;
            active_fps = (hdr->fps_num >= 5 && hdr->fps_num <= 120) ? hdr->fps_num : 30;
        }
    }

    struct ResPair { uint32_t w; uint32_t h; uint32_t fps; };
    std::vector<ResPair> resolutions = {
        {active_w, active_h, active_fps},
        {1440, 1080, 30}, // 4:3 1080p
        {1920, 1080, 30}, // 16:9 1080p
        {1280, 720,  30}, // 16:9 720p
        {960,  720,  30}, // 4:3 720p
        {1080, 1080, 30}, // 1:1 1080p
        {720,  720,  30}, // 1:1 720p
        {640,  480,  30}, // 4:3 480p
        {854,  480,  30}, // 16:9 480p
        {640,  360,  30}  // 16:9 360p
    };

    std::vector<SmartCamFormatSpec> list;
    list.reserve(resolutions.size() * 3);

    auto add_unique = [&](uint32_t w, uint32_t h, uint32_t fps, const GUID& sub, uint16_t bpp, DWORD comp) {
        for (const auto& existing : list) {
            if (existing.width == w && existing.height == h && existing.subtype == sub) {
                return;
            }
        }
        list.push_back(SmartCamFormatSpec{w, h, fps, sub, bpp, comp});
    };

    for (const auto& r : resolutions) {
        // 1. RGB24 (Natively preferred by Chromium VideoCaptureDeviceFactoryWin)
        add_unique(r.w, r.h, r.fps, kMediaSubtypeRGB24, 24, BI_RGB);
        // 2. YUY2 (Natively preferred by Chromium, WebRTC, Zoom, Teams)
        add_unique(r.w, r.h, r.fps, kMediaSubtypeYUY2, 16, kFourCC_YUY2);
        // 3. RGB32 (Supported by OBS, DirectShow tools, and Media Foundation bridges)
        add_unique(r.w, r.h, r.fps, kMediaSubtypeRGB32, 32, BI_RGB);
    }

    return list;
}

bool SmartCamDShowPin::FillMediaTypeFromSpec(const SmartCamFormatSpec& spec, AM_MEDIA_TYPE* mt) {
    if (!mt) return false;
    FreeMediaTypeFields(*mt);
    std::memset(mt, 0, sizeof(AM_MEDIA_TYPE));

    auto* vih = static_cast<VIDEOINFOHEADER*>(CoTaskMemAlloc(sizeof(VIDEOINFOHEADER)));
    if (!vih) return false;
    std::memset(vih, 0, sizeof(VIDEOINFOHEADER));

    const uint32_t w   = (spec.width >= 16) ? (spec.width & ~1u) : 1440;
    const uint32_t h   = (spec.height >= 16) ? (spec.height & ~1u) : 1080;
    const uint32_t fps = (spec.fps >= 5 && spec.fps <= 120) ? spec.fps : 30;
    const DWORD bytes_per_pixel = spec.bit_count / 8;
    const DWORD image_size      = w * h * bytes_per_pixel;

    SetRect(&vih->rcSource, 0, 0, static_cast<int>(w), static_cast<int>(h));
    SetRect(&vih->rcTarget, 0, 0, static_cast<int>(w), static_cast<int>(h));
    vih->AvgTimePerFrame         = static_cast<REFERENCE_TIME>(10000000ULL / fps);
    vih->dwBitRate               = image_size * 8 * fps;
    vih->bmiHeader.biSize        = sizeof(BITMAPINFOHEADER);
    vih->bmiHeader.biWidth       = static_cast<LONG>(w);
    vih->bmiHeader.biHeight      = static_cast<LONG>(h); // Positive height (standard DIB for RGB24/RGB32, top-down for YUY2)
    vih->bmiHeader.biPlanes      = 1;
    vih->bmiHeader.biBitCount    = spec.bit_count;
    vih->bmiHeader.biCompression = spec.compression;
    vih->bmiHeader.biSizeImage   = image_size;

    mt->majortype            = kMediaTypeVideo;
    mt->subtype              = spec.subtype;
    mt->bFixedSizeSamples    = TRUE;
    mt->bTemporalCompression = FALSE;
    mt->lSampleSize          = image_size;
    mt->formattype           = kFormatVideoInfo;
    mt->pUnk                 = nullptr;
    mt->cbFormat             = sizeof(VIDEOINFOHEADER);
    mt->pbFormat             = reinterpret_cast<BYTE*>(vih);
    return true;
}

void SmartCamDShowPin::FreeMediaTypeFields(AM_MEDIA_TYPE& mt) {
    if (mt.cbFormat != 0 && mt.pbFormat != nullptr) {
        CoTaskMemFree(mt.pbFormat);
        mt.pbFormat = nullptr;
        mt.cbFormat = 0;
    }
    if (mt.pUnk != nullptr) {
        mt.pUnk->Release();
        mt.pUnk = nullptr;
    }
}

void SmartCamDShowPin::DeleteMediaType(AM_MEDIA_TYPE* pmt) {
    if (!pmt) return;
    FreeMediaTypeFields(*pmt);
    CoTaskMemFree(pmt);
}

HRESULT SmartCamDShowPin::CopyMediaType(AM_MEDIA_TYPE* dst, const AM_MEDIA_TYPE* src) {
    if (!dst || !src) return E_POINTER;
    if (dst == src) return S_OK;
    FreeMediaTypeFields(*dst);
    *dst = *src;
    dst->pUnk = nullptr;
    if (src->cbFormat > 0 && src->pbFormat != nullptr) {
        dst->pbFormat = static_cast<BYTE*>(CoTaskMemAlloc(src->cbFormat));
        if (!dst->pbFormat) {
            dst->cbFormat = 0;
            return E_OUTOFMEMORY;
        }
        std::memcpy(dst->pbFormat, src->pbFormat, src->cbFormat);
    } else {
        dst->cbFormat = 0;
        dst->pbFormat = nullptr;
    }
    if (src->pUnk != nullptr) {
        dst->pUnk = src->pUnk;
        dst->pUnk->AddRef();
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::Connect(IPin* pReceivePin, const AM_MEDIA_TYPE* pmt) {
    if (!pReceivePin) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (connected_pin_) return VFW_E_ALREADY_CONNECTED;

    if (!ipc_.IsOpen()) {
        ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
    }

    if (pmt && pmt->majortype != GUID_NULL) {
        if (QueryAccept(pmt) != S_OK) {
            return VFW_E_TYPE_NOT_ACCEPTED;
        }
        CopyMediaType(&current_mt_, pmt);
        has_custom_mt_ = true;
    } else if (!has_custom_mt_) {
        const auto specs = BuildFormatList();
        if (!specs.empty()) {
            FillMediaTypeFromSpec(specs[0], &current_mt_);
        }
    }

    HRESULT hr = pReceivePin->ReceiveConnection(static_cast<IPin*>(this), &current_mt_);
    if (FAILED(hr)) {
        // Try fallback media types if the receiver rejected our initial format
        const auto specs = BuildFormatList();
        bool connected = false;
        for (const auto& s : specs) {
            AM_MEDIA_TYPE candidate{};
            if (FillMediaTypeFromSpec(s, &candidate)) {
                if (SUCCEEDED(pReceivePin->ReceiveConnection(static_cast<IPin*>(this), &candidate))) {
                    CopyMediaType(&current_mt_, &candidate);
                    FreeMediaTypeFields(candidate);
                    connected = true;
                    break;
                }
                FreeMediaTypeFields(candidate);
            }
        }
        if (!connected) {
            return VFW_E_NO_ACCEPTABLE_TYPES;
        }
    }

    mem_input_pin_.Reset();
    hr = pReceivePin->QueryInterface(IID_IMemInputPin, reinterpret_cast<void**>(mem_input_pin_.GetAddressOf()));
    if (FAILED(hr) || !mem_input_pin_) {
        pReceivePin->Disconnect();
        return VFW_E_NO_TRANSPORT;
    }

    allocator_.Reset();
    hr = mem_input_pin_->GetAllocator(allocator_.GetAddressOf());
    if (FAILED(hr) || !allocator_) {
        hr = CoCreateInstance(kClsidMemoryAllocator, nullptr, CLSCTX_INPROC_SERVER,
                              IID_IMemAllocator, reinterpret_cast<void**>(allocator_.GetAddressOf()));
        if (FAILED(hr) || !allocator_) {
            allocator_.Attach(new (std::nothrow) SmartCamMemAllocator());
        }
    }
    if (!allocator_) {
        mem_input_pin_.Reset();
        pReceivePin->Disconnect();
        return E_OUTOFMEMORY;
    }

    const LONG sample_bytes = (current_mt_.lSampleSize > 0)
                                  ? static_cast<LONG>(current_mt_.lSampleSize)
                                  : (1920 * 1080 * 4);
    ALLOCATOR_PROPERTIES req{};
    req.cBuffers = 4;
    req.cbBuffer = sample_bytes;
    req.cbAlign  = 4;
    req.cbPrefix = 0;

    ALLOCATOR_PROPERTIES actual{};
    hr = allocator_->SetProperties(&req, &actual);
    if (FAILED(hr)) {
        allocator_.Reset();
        allocator_.Attach(new (std::nothrow) SmartCamMemAllocator());
        allocator_->SetProperties(&req, &actual);
    }

    mem_input_pin_->NotifyAllocator(allocator_.Get(), FALSE);
    connected_pin_ = pReceivePin;
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::ReceiveConnection(IPin* /*pConnector*/, const AM_MEDIA_TYPE* /*pmt*/) {
    return E_UNEXPECTED; // Output pin does not receive incoming connections
}

STDMETHODIMP SmartCamDShowPin::Disconnect() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!connected_pin_) return S_FALSE;
    if (allocator_) {
        allocator_->Decommit();
    }
    allocator_.Reset();
    mem_input_pin_.Reset();
    connected_pin_.Reset();
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::ConnectedTo(IPin** pPin) {
    if (!pPin) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!connected_pin_) {
        *pPin = nullptr;
        return VFW_E_NOT_CONNECTED;
    }
    *pPin = connected_pin_.Get();
    (*pPin)->AddRef();
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::ConnectionMediaType(AM_MEDIA_TYPE* pmt) {
    if (!pmt) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!connected_pin_) {
        std::memset(pmt, 0, sizeof(AM_MEDIA_TYPE));
        return VFW_E_NOT_CONNECTED;
    }
    std::memset(pmt, 0, sizeof(AM_MEDIA_TYPE));
    return CopyMediaType(pmt, &current_mt_);
}

STDMETHODIMP SmartCamDShowPin::QueryPinInfo(PIN_INFO* pInfo) {
    if (!pInfo) return E_POINTER;
    pInfo->pFilter = static_cast<IBaseFilter*>(parent_filter_);
    if (pInfo->pFilter) {
        pInfo->pFilter->AddRef();
    }
    pInfo->dir = PINDIR_OUTPUT;
    wcscpy_s(pInfo->achName, L"Capture");
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::QueryDirection(PIN_DIRECTION* pPinDir) {
    if (!pPinDir) return E_POINTER;
    *pPinDir = PINDIR_OUTPUT;
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::QueryId(LPWSTR* Id) {
    if (!Id) return E_POINTER;
    *Id = AllocCoTaskWideString(L"Capture");
    return (*Id) ? S_OK : E_OUTOFMEMORY;
}

STDMETHODIMP SmartCamDShowPin::QueryAccept(const AM_MEDIA_TYPE* pmt) {
    if (!pmt) return E_POINTER;
    if (pmt->majortype != kMediaTypeVideo || pmt->formattype != kFormatVideoInfo) {
        return S_FALSE;
    }
    if (!pmt->pbFormat || pmt->cbFormat < sizeof(VIDEOINFOHEADER)) {
        return S_FALSE;
    }
    if (pmt->subtype != kMediaSubtypeRGB24 &&
        pmt->subtype != kMediaSubtypeYUY2 &&
        pmt->subtype != kMediaSubtypeRGB32) {
        return S_FALSE;
    }
    const auto* vih = reinterpret_cast<const VIDEOINFOHEADER*>(pmt->pbFormat);
    const LONG w = vih->bmiHeader.biWidth;
    const LONG h = std::abs(vih->bmiHeader.biHeight);
    if (w < 64 || w > 3840 || h < 64 || h > 2160) {
        return S_FALSE;
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::EnumMediaTypes(IEnumMediaTypes** ppEnum) {
    if (!ppEnum) return E_POINTER;
    auto* enumerator = new (std::nothrow) SmartCamDShowEnumMediaTypes(this, 0);
    if (!enumerator) return E_OUTOFMEMORY;
    *ppEnum = enumerator;
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::QueryInternalConnections(IPin** /*apPin*/, ULONG* /*nPin*/) {
    return E_NOTIMPL;
}

STDMETHODIMP SmartCamDShowPin::EndOfStream() { return S_OK; }
STDMETHODIMP SmartCamDShowPin::BeginFlush() { return S_OK; }
STDMETHODIMP SmartCamDShowPin::EndFlush() { return S_OK; }
STDMETHODIMP SmartCamDShowPin::NewSegment(REFERENCE_TIME /*tStart*/, REFERENCE_TIME /*tStop*/, double /*dRate*/) {
    return S_OK;
}

// IAMStreamConfig
STDMETHODIMP SmartCamDShowPin::SetFormat(AM_MEDIA_TYPE* pmt) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!pmt) {
        has_custom_mt_ = false;
        const auto specs = BuildFormatList();
        if (!specs.empty()) {
            FillMediaTypeFromSpec(specs[0], &current_mt_);
        }
        return S_OK;
    }
    if (QueryAccept(pmt) != S_OK) {
        return VFW_E_INVALIDMEDIATYPE;
    }
    HRESULT hr = CopyMediaType(&current_mt_, pmt);
    if (SUCCEEDED(hr)) {
        has_custom_mt_ = true;
        // Fix up lSampleSize & biSizeImage in case caller left them zero
        if (current_mt_.pbFormat && current_mt_.cbFormat >= sizeof(VIDEOINFOHEADER)) {
            auto* vih = reinterpret_cast<VIDEOINFOHEADER*>(current_mt_.pbFormat);
            const uint32_t w = static_cast<uint32_t>(std::max<LONG>(64, vih->bmiHeader.biWidth));
            const uint32_t h = static_cast<uint32_t>(std::max<LONG>(64, std::abs(vih->bmiHeader.biHeight)));
            uint32_t bpp = 3;
            if (current_mt_.subtype == kMediaSubtypeYUY2) {
                bpp = 2;
                vih->bmiHeader.biBitCount    = 16;
                vih->bmiHeader.biCompression = kFourCC_YUY2;
            } else if (current_mt_.subtype == kMediaSubtypeRGB32) {
                bpp = 4;
                vih->bmiHeader.biBitCount    = 32;
                vih->bmiHeader.biCompression = BI_RGB;
            } else {
                bpp = 3;
                vih->bmiHeader.biBitCount    = 24;
                vih->bmiHeader.biCompression = BI_RGB;
            }
            const DWORD img_bytes = w * h * bpp;
            vih->bmiHeader.biSizeImage = img_bytes;
            current_mt_.lSampleSize    = img_bytes;
            if (vih->AvgTimePerFrame <= 0) {
                vih->AvgTimePerFrame = 333333LL;
            }
        }
    }
    return hr;
}

STDMETHODIMP SmartCamDShowPin::GetFormat(AM_MEDIA_TYPE** ppmt) {
    if (!ppmt) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!has_custom_mt_) {
        const auto specs = BuildFormatList();
        if (!specs.empty()) {
            FillMediaTypeFromSpec(specs[0], const_cast<AM_MEDIA_TYPE*>(&current_mt_));
        }
    }
    auto* out = static_cast<AM_MEDIA_TYPE*>(CoTaskMemAlloc(sizeof(AM_MEDIA_TYPE)));
    if (!out) return E_OUTOFMEMORY;
    std::memset(out, 0, sizeof(AM_MEDIA_TYPE));
    HRESULT hr = CopyMediaType(out, &current_mt_);
    if (FAILED(hr)) {
        CoTaskMemFree(out);
        *ppmt = nullptr;
        return hr;
    }
    *ppmt = out;
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::GetNumberOfCapabilities(int* piCount, int* piSize) {
    if (!piCount || !piSize) return E_POINTER;
    const auto specs = BuildFormatList();
    *piCount = static_cast<int>(specs.size());
    *piSize  = static_cast<int>(sizeof(VIDEO_STREAM_CONFIG_CAPS));
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::GetStreamCaps(int iIndex, AM_MEDIA_TYPE** ppmt, BYTE* pSCC) {
    if (!ppmt || !pSCC) return E_POINTER;
    if (iIndex < 0) return E_INVALIDARG;

    const auto specs = BuildFormatList();
    if (static_cast<size_t>(iIndex) >= specs.size()) {
        return S_FALSE;
    }

    auto* mt = static_cast<AM_MEDIA_TYPE*>(CoTaskMemAlloc(sizeof(AM_MEDIA_TYPE)));
    if (!mt) return E_OUTOFMEMORY;
    std::memset(mt, 0, sizeof(AM_MEDIA_TYPE));

    const auto& spec = specs[static_cast<size_t>(iIndex)];
    if (!FillMediaTypeFromSpec(spec, mt)) {
        CoTaskMemFree(mt);
        return E_OUTOFMEMORY;
    }
    *ppmt = mt;

    auto* scc = reinterpret_cast<VIDEO_STREAM_CONFIG_CAPS*>(pSCC);
    std::memset(scc, 0, sizeof(VIDEO_STREAM_CONFIG_CAPS));
    scc->guid                 = kFormatVideoInfo;
    scc->VideoStandard        = AnalogVideo_None;
    scc->InputSize.cx         = static_cast<LONG>(spec.width);
    scc->InputSize.cy         = static_cast<LONG>(spec.height);
    scc->MinCroppingSize.cx   = static_cast<LONG>(spec.width);
    scc->MinCroppingSize.cy   = static_cast<LONG>(spec.height);
    scc->MaxCroppingSize.cx   = static_cast<LONG>(spec.width);
    scc->MaxCroppingSize.cy   = static_cast<LONG>(spec.height);
    scc->CropGranularityX     = 2;
    scc->CropGranularityY     = 2;
    scc->CropAlignX           = 0;
    scc->CropAlignY           = 0;
    scc->MinOutputSize.cx     = static_cast<LONG>(spec.width);
    scc->MinOutputSize.cy     = static_cast<LONG>(spec.height);
    scc->MaxOutputSize.cx     = static_cast<LONG>(spec.width);
    scc->MaxOutputSize.cy     = static_cast<LONG>(spec.height);
    scc->OutputGranularityX   = 2;
    scc->OutputGranularityY   = 2;
    scc->StretchTapsX         = 1;
    scc->StretchTapsY         = 1;
    scc->ShrinkTapsX          = 1;
    scc->ShrinkTapsY          = 1;
    scc->MinFrameInterval     = 166666LL;  // Up to 60 FPS
    scc->MaxFrameInterval     = 1000000LL; // Down to 10 FPS
    const LONG frame_bits     = static_cast<LONG>(spec.width * spec.height * spec.bit_count);
    scc->MinBitsPerSecond     = frame_bits * 10;
    scc->MaxBitsPerSecond     = frame_bits * 60;

    return S_OK;
}

// IKsPropertySet
STDMETHODIMP SmartCamDShowPin::Set(REFGUID /*guidPropSet*/, DWORD /*dwPropID*/,
                                   LPVOID /*pInstanceData*/, DWORD /*cbInstanceData*/,
                                   LPVOID /*pPropData*/, DWORD /*cbPropData*/) {
    return E_NOTIMPL;
}

STDMETHODIMP SmartCamDShowPin::Get(REFGUID guidPropSet, DWORD dwPropID,
                                   LPVOID /*pInstanceData*/, DWORD /*cbInstanceData*/,
                                   LPVOID pPropData, DWORD cbPropData, DWORD* pcbReturned) {
    if (guidPropSet != kAmPropSetIdPin) {
        return E_PROP_SET_UNSUPPORTED;
    }
    if (dwPropID != AMPROPERTY_PIN_CATEGORY) {
        return E_PROP_ID_UNSUPPORTED;
    }
    if (pcbReturned) {
        *pcbReturned = sizeof(GUID);
    }
    if (!pPropData) {
        return S_OK;
    }
    if (cbPropData < sizeof(GUID)) {
        return E_UNEXPECTED;
    }
    *reinterpret_cast<GUID*>(pPropData) = kPinCategoryCapture;
    return S_OK;
}

STDMETHODIMP SmartCamDShowPin::QuerySupported(REFGUID guidPropSet, DWORD dwPropID, DWORD* pTypeSupport) {
    if (guidPropSet != kAmPropSetIdPin) {
        return E_PROP_SET_UNSUPPORTED;
    }
    if (dwPropID != AMPROPERTY_PIN_CATEGORY) {
        return E_PROP_ID_UNSUPPORTED;
    }
    if (pTypeSupport) {
        *pTypeSupport = KSPROPERTY_SUPPORT_GET;
    }
    return S_OK;
}

// IQualityControl
STDMETHODIMP SmartCamDShowPin::Notify(IBaseFilter* /*pSelf*/, Quality /*q*/) { return S_OK; }
STDMETHODIMP SmartCamDShowPin::SetSink(IQualityControl* /*piqc*/) { return S_OK; }

HRESULT SmartCamDShowPin::OnFilterPause() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!ipc_.IsOpen()) {
        ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
    }
    if (ipc_.IsOpen()) {
        ipc_.TouchConsumerHeartbeat();
    }
    if (allocator_) {
        allocator_->Commit();
    }
    return S_OK;
}

HRESULT SmartCamDShowPin::OnFilterRun(REFERENCE_TIME tStart) {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        run_start_time_ = tStart;
        if (!ipc_.IsOpen()) {
            ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
        }
        if (ipc_.IsOpen()) {
            ipc_.TouchConsumerHeartbeat();
        }
        if (allocator_) {
            allocator_->Commit();
        }
        if (streaming_.load()) {
            return S_OK;
        }
        streaming_.store(true);
    }
    worker_thread_ = std::thread(&SmartCamDShowPin::StreamingThreadLoop, this);
    return S_OK;
}

HRESULT SmartCamDShowPin::OnFilterStop() {
    streaming_.store(false);
    {
        std::lock_guard<std::mutex> lock(mutex_);
        if (allocator_) {
            allocator_->Decommit();
        }
    }
    if (worker_thread_.joinable()) {
        worker_thread_.join();
    }
    return S_OK;
}

void SmartCamDShowPin::RenderFrameToSampleBuffer(uint8_t* dst, size_t dst_cap,
                                                 uint32_t out_w, uint32_t out_h,
                                                 bool bottom_up, const GUID& subtype) {
    if (!dst || out_w == 0 || out_h == 0) return;

    if (!ipc_.IsOpen()) {
        ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
    }

    uint32_t ipc_w = 0;
    uint32_t ipc_h = 0;
    uint64_t ts_100ns = 0;
    bool got_ipc = false;

    if (ipc_.IsOpen()) {
        const SharedFrameHeader* hdr = ipc_.Header();
        if (hdr && hdr->magic == SharedFrameHeader::kMagic &&
            hdr->width > 0 && hdr->height > 0 &&
            static_cast<size_t>(hdr->width) * hdr->height * 4 <= SharedFrameHeader::kMaxFrameBytes) {
            const size_t needed = static_cast<size_t>(hdr->width) * hdr->height * 4;
            if (ipc_frame_scratch_.size() < needed) {
                ipc_frame_scratch_.resize(needed);
            }
            got_ipc = ipc_.ReadFrame(ipc_frame_scratch_.data(), ipc_frame_scratch_.size(),
                                     ipc_w, ipc_h, ts_100ns);
        } else {
            ipc_.TouchConsumerHeartbeat();
        }
    }

    // If SmartCam.exe is not actively pushing frames yet, synthesize a clean studio standby pattern
    if (!got_ipc || ipc_w == 0 || ipc_h == 0) {
        ipc_w = out_w;
        ipc_h = out_h;
        const size_t needed = static_cast<size_t>(ipc_w) * ipc_h * 4;
        if (ipc_frame_scratch_.size() < needed) {
            ipc_frame_scratch_.resize(needed);
        }
        const uint32_t bar_offset = static_cast<uint32_t>((sample_counter_ * 4) % ipc_w);
        for (uint32_t y = 0; y < ipc_h; ++y) {
            uint8_t* row = ipc_frame_scratch_.data() + static_cast<size_t>(y) * ipc_w * 4;
            const uint8_t grad_y = static_cast<uint8_t>((y * 140) / ipc_h + 28);
            for (uint32_t x = 0; x < ipc_w; ++x) {
                const bool is_pulse = ((x + bar_offset) % 140) < 6;
                row[x * 4 + 0] = is_pulse ? 235 : 58;      // B
                row[x * 4 + 1] = is_pulse ? 190 : grad_y;  // G
                row[x * 4 + 2] = is_pulse ? 70  : 22;      // R
                row[x * 4 + 3] = 255;                      // A
            }
        }
    }

    const uint8_t* src_bgra = ipc_frame_scratch_.data();
    const bool same_dims    = (ipc_w == out_w && ipc_h == out_h);

    if (subtype == kMediaSubtypeRGB24) {
        const size_t dst_stride = static_cast<size_t>(out_w) * 3;
        if (dst_stride * out_h > dst_cap) return;

        for (uint32_t dy = 0; dy < out_h; ++dy) {
            const uint32_t src_y = same_dims ? dy : ((dy * ipc_h) / out_h);
            const uint32_t dst_row_idx = bottom_up ? (out_h - 1 - dy) : dy;
            uint8_t* dst_row = dst + static_cast<size_t>(dst_row_idx) * dst_stride;
            const uint8_t* src_row = src_bgra + static_cast<size_t>(src_y) * ipc_w * 4;

            if (same_dims) {
                for (uint32_t dx = 0; dx < out_w; ++dx) {
                    dst_row[dx * 3 + 0] = src_row[dx * 4 + 0];
                    dst_row[dx * 3 + 1] = src_row[dx * 4 + 1];
                    dst_row[dx * 3 + 2] = src_row[dx * 4 + 2];
                }
            } else {
                for (uint32_t dx = 0; dx < out_w; ++dx) {
                    const uint32_t sx = (dx * ipc_w) / out_w;
                    dst_row[dx * 3 + 0] = src_row[sx * 4 + 0];
                    dst_row[dx * 3 + 1] = src_row[sx * 4 + 1];
                    dst_row[dx * 3 + 2] = src_row[sx * 4 + 2];
                }
            }
        }
    } else if (subtype == kMediaSubtypeYUY2) {
        const size_t dst_stride = static_cast<size_t>(out_w) * 2;
        if (dst_stride * out_h > dst_cap) return;

        for (uint32_t dy = 0; dy < out_h; ++dy) {
            const uint32_t src_y = same_dims ? dy : ((dy * ipc_h) / out_h);
            uint8_t* dst_row = dst + static_cast<size_t>(dy) * dst_stride;
            const uint8_t* src_row = src_bgra + static_cast<size_t>(src_y) * ipc_w * 4;

            for (uint32_t dx = 0; dx + 1 < out_w; dx += 2) {
                const uint32_t sx0 = same_dims ? dx : ((dx * ipc_w) / out_w);
                const uint32_t sx1 = same_dims ? (dx + 1) : (((dx + 1) * ipc_w) / out_w);

                const int b0 = src_row[sx0 * 4 + 0];
                const int g0 = src_row[sx0 * 4 + 1];
                const int r0 = src_row[sx0 * 4 + 2];

                const int b1 = src_row[sx1 * 4 + 0];
                const int g1 = src_row[sx1 * 4 + 1];
                const int r1 = src_row[sx1 * 4 + 2];

                const int y0 = ((66 * r0 + 129 * g0 + 25 * b0 + 128) >> 8) + 16;
                const int y1 = ((66 * r1 + 129 * g1 + 25 * b1 + 128) >> 8) + 16;
                const int r_avg = (r0 + r1) >> 1;
                const int g_avg = (g0 + g1) >> 1;
                const int b_avg = (b0 + b1) >> 1;
                const int u = ((-38 * r_avg - 74 * g_avg + 112 * b_avg + 128) >> 8) + 128;
                const int v = ((112 * r_avg - 94 * g_avg - 18 * b_avg + 128) >> 8) + 128;

                dst_row[dx * 2 + 0] = static_cast<uint8_t>(std::clamp(y0, 16, 235));
                dst_row[dx * 2 + 1] = static_cast<uint8_t>(std::clamp(u,  16, 240));
                dst_row[dx * 2 + 2] = static_cast<uint8_t>(std::clamp(y1, 16, 235));
                dst_row[dx * 2 + 3] = static_cast<uint8_t>(std::clamp(v,  16, 240));
            }
        }
    } else {
        // MEDIASUBTYPE_RGB32
        const size_t dst_stride = static_cast<size_t>(out_w) * 4;
        if (dst_stride * out_h > dst_cap) return;

        for (uint32_t dy = 0; dy < out_h; ++dy) {
            const uint32_t src_y = same_dims ? dy : ((dy * ipc_h) / out_h);
            const uint32_t dst_row_idx = bottom_up ? (out_h - 1 - dy) : dy;
            uint8_t* dst_row = dst + static_cast<size_t>(dst_row_idx) * dst_stride;
            const uint8_t* src_row = src_bgra + static_cast<size_t>(src_y) * ipc_w * 4;

            if (same_dims) {
                std::memcpy(dst_row, src_row, dst_stride);
            } else {
                for (uint32_t dx = 0; dx < out_w; ++dx) {
                    const uint32_t sx = (dx * ipc_w) / out_w;
                    dst_row[dx * 4 + 0] = src_row[sx * 4 + 0];
                    dst_row[dx * 4 + 1] = src_row[sx * 4 + 1];
                    dst_row[dx * 4 + 2] = src_row[sx * 4 + 2];
                    dst_row[dx * 4 + 3] = 255;
                }
            }
        }
    }
}

void SmartCamDShowPin::StreamingThreadLoop() {
    uint64_t local_frame_idx = 0;
    auto next_tick = std::chrono::steady_clock::now();

    while (streaming_.load()) {
        Microsoft::WRL::ComPtr<IMemAllocator> alloc;
        Microsoft::WRL::ComPtr<IMemInputPin>  sink_pin;
        uint32_t out_w = 1440;
        uint32_t out_h = 1080;
        bool bottom_up = true;
        GUID subtype = kMediaSubtypeRGB24;
        REFERENCE_TIME avg_frame_time = 333333LL; // 30 FPS default
        LONG sample_bytes = 1440 * 1080 * 3;

        {
            std::lock_guard<std::mutex> lock(mutex_);
            alloc    = allocator_;
            sink_pin = mem_input_pin_;
            subtype  = current_mt_.subtype;
            if (current_mt_.pbFormat && current_mt_.cbFormat >= sizeof(VIDEOINFOHEADER)) {
                const auto* vih = reinterpret_cast<const VIDEOINFOHEADER*>(current_mt_.pbFormat);
                out_w          = static_cast<uint32_t>(std::max<LONG>(64, vih->bmiHeader.biWidth));
                out_h          = static_cast<uint32_t>(std::max<LONG>(64, std::abs(vih->bmiHeader.biHeight)));
                bottom_up      = (vih->bmiHeader.biHeight > 0);
                if (vih->AvgTimePerFrame >= 100000LL && vih->AvgTimePerFrame <= 2000000LL) {
                    avg_frame_time = vih->AvgTimePerFrame;
                }
            }
            if (ipc_.IsOpen()) {
                const SharedFrameHeader* hdr = ipc_.Header();
                if (hdr && hdr->magic == SharedFrameHeader::kMagic &&
                    hdr->fps_num >= 5 && hdr->fps_num <= 120) {
                    avg_frame_time = static_cast<REFERENCE_TIME>(10000000ULL / hdr->fps_num);
                }
            }
            uint32_t bpp = 3;
            if (subtype == kMediaSubtypeYUY2) bpp = 2;
            else if (subtype == kMediaSubtypeRGB32) bpp = 4;
            sample_bytes = static_cast<LONG>(out_w * out_h * bpp);
        }

        if (!alloc || !sink_pin) {
            std::this_thread::sleep_for(std::chrono::milliseconds(15));
            next_tick = std::chrono::steady_clock::now();
            continue;
        }

        Microsoft::WRL::ComPtr<IMediaSample> sample;
        HRESULT hr = alloc->GetBuffer(sample.GetAddressOf(), nullptr, nullptr, 0);
        if (SUCCEEDED(hr) && sample) {
            BYTE* ptr = nullptr;
            if (SUCCEEDED(sample->GetPointer(&ptr)) && ptr) {
                const LONG cap = sample->GetSize();
                RenderFrameToSampleBuffer(ptr, static_cast<size_t>(std::max<LONG>(0, cap)),
                                          out_w, out_h, bottom_up, subtype);
                sample->SetActualDataLength(std::min<LONG>(cap, sample_bytes));
                sample->SetSyncPoint(TRUE);
                sample->SetPreroll(FALSE);
                sample->SetDiscontinuity(local_frame_idx == 0 ? TRUE : FALSE);

                REFERENCE_TIME rt_start = static_cast<REFERENCE_TIME>(local_frame_idx) * avg_frame_time;
                if (parent_filter_) {
                    REFERENCE_TIME graph_now = 0;
                    if (SUCCEEDED(parent_filter_->GetReferenceTime(&graph_now)) && graph_now >= run_start_time_) {
                        rt_start = graph_now - run_start_time_;
                    }
                }
                REFERENCE_TIME rt_end = rt_start + avg_frame_time;
                sample->SetTime(&rt_start, &rt_end);

                LONGLONG mt_start = static_cast<LONGLONG>(local_frame_idx);
                LONGLONG mt_end   = mt_start + 1;
                sample->SetMediaTime(&mt_start, &mt_end);

                ++local_frame_idx;
                ++sample_counter_;
                hr = sink_pin->Receive(sample.Get());
                if (hr != S_OK && hr != S_FALSE) {
                    // Downstream stopped or rejected sample
                }
            }
        }

        const auto frame_dur = std::chrono::microseconds(avg_frame_time / 10);
        next_tick += frame_dur;
        const auto now = std::chrono::steady_clock::now();
        if (next_tick > now) {
            std::this_thread::sleep_until(next_tick);
        } else {
            next_tick = now;
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        }
    }
}

// ============================================================================
// SmartCamDShowFilter Implementation
// ============================================================================

SmartCamDShowFilter::SmartCamDShowFilter() {
    capture_pin_ = new (std::nothrow) SmartCamDShowPin(this);
}

SmartCamDShowFilter::~SmartCamDShowFilter() {
    Stop();
    delete capture_pin_;
    capture_pin_ = nullptr;
    if (mf_source_) {
        mf_source_->Shutdown();
        mf_source_.Reset();
    }
}

HRESULT SmartCamDShowFilter::Initialize() {
    if (!capture_pin_) return E_OUTOFMEMORY;
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) return E_POINTER;
    if (riid == IID_IUnknown || riid == IID_IPersist ||
        riid == IID_IMediaFilter || riid == IID_IBaseFilter) {
        *ppv = static_cast<IBaseFilter*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IAMFilterMiscFlags) {
        *ppv = static_cast<IAMFilterMiscFlags*>(this);
        AddRef();
        return S_OK;
    }
    // Also delegate Media Foundation interfaces (`IMFMediaSourceEx`, `IMFMediaSource`,
    // `IMFMediaEventGenerator`, `IMFGetService`) so this single COM class works seamlessly
    // for both DirectShow consumers (Chromium/Firefox/OBS) and Media Foundation FrameServer!
    if (riid == IID_IMFMediaEventGenerator || riid == IID_IMFMediaSource ||
        riid == IID_IMFMediaSourceEx || riid == IID_IMFGetService) {
        std::lock_guard<std::mutex> lock(mutex_);
        if (!mf_source_) {
            mf_source_.Attach(new (std::nothrow) SmartCamMfMediaSource());
            if (!mf_source_ || FAILED(mf_source_->Initialize())) {
                mf_source_.Reset();
                *ppv = nullptr;
                return E_NOINTERFACE;
            }
        }
        return mf_source_->QueryInterface(riid, ppv);
    }
    *ppv = nullptr;
    return E_NOINTERFACE;
}

STDMETHODIMP_(ULONG) SmartCamDShowFilter::AddRef() {
    return InterlockedIncrement(&ref_count_);
}

STDMETHODIMP_(ULONG) SmartCamDShowFilter::Release() {
    const ULONG count = InterlockedDecrement(&ref_count_);
    if (count == 0) {
        delete this;
    }
    return count;
}

STDMETHODIMP SmartCamDShowFilter::GetClassID(CLSID* pClassID) {
    if (!pClassID) return E_POINTER;
    *pClassID = CLSID_SmartCamMfVirtualCameraSource;
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::Stop() {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        state_ = State_Stopped;
    }
    if (capture_pin_) {
        capture_pin_->OnFilterStop();
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::Pause() {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        state_ = State_Paused;
    }
    if (capture_pin_) {
        capture_pin_->OnFilterPause();
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::Run(REFERENCE_TIME tStart) {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        state_ = State_Running;
    }
    if (capture_pin_) {
        capture_pin_->OnFilterRun(tStart);
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::GetState(DWORD /*dwMilliSecsTimeout*/, FILTER_STATE* State) {
    if (!State) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    *State = state_;
    if (state_ == State_Paused) {
        return VFW_S_CANT_CUE; // Live capture source does not cue pre-roll frames while paused
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::SetSyncSource(IReferenceClock* pClock) {
    std::lock_guard<std::mutex> lock(mutex_);
    clock_ = pClock;
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::GetSyncSource(IReferenceClock** pClock) {
    if (!pClock) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    *pClock = clock_.Get();
    if (*pClock) {
        (*pClock)->AddRef();
    }
    return S_OK;
}

HRESULT SmartCamDShowFilter::GetReferenceTime(REFERENCE_TIME* out_time) {
    if (!out_time) return E_POINTER;
    Microsoft::WRL::ComPtr<IReferenceClock> clk;
    {
        std::lock_guard<std::mutex> lock(mutex_);
        clk = clock_;
    }
    if (!clk) return E_FAIL;
    return clk->GetTime(out_time);
}

STDMETHODIMP SmartCamDShowFilter::EnumPins(IEnumPins** ppEnum) {
    if (!ppEnum) return E_POINTER;
    auto* enumerator = new (std::nothrow) SmartCamDShowEnumPins(capture_pin_, 0);
    if (!enumerator) return E_OUTOFMEMORY;
    *ppEnum = enumerator;
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::FindPin(LPCWSTR Id, IPin** ppPin) {
    if (!ppPin) return E_POINTER;
    if (!Id) return E_INVALIDARG;
    if (_wcsicmp(Id, L"Capture") == 0 || _wcsicmp(Id, L"Out") == 0 || wcscmp(Id, L"0") == 0) {
        *ppPin = static_cast<IPin*>(capture_pin_);
        if (*ppPin) (*ppPin)->AddRef();
        return S_OK;
    }
    *ppPin = nullptr;
    return VFW_E_NOT_FOUND;
}

STDMETHODIMP SmartCamDShowFilter::QueryFilterInfo(FILTER_INFO* pInfo) {
    if (!pInfo) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    wcsncpy_s(pInfo->achName, filter_name_.c_str(), _TRUNCATE);
    pInfo->pGraph = graph_;
    if (pInfo->pGraph) {
        pInfo->pGraph->AddRef();
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::JoinFilterGraph(IFilterGraph* pGraph, LPCWSTR pName) {
    std::lock_guard<std::mutex> lock(mutex_);
    graph_ = pGraph; // Weak reference per DirectShow specification (must NOT AddRef)
    if (pName && *pName) {
        filter_name_ = pName;
    } else {
        filter_name_ = L"SmartCam";
    }
    return S_OK;
}

STDMETHODIMP SmartCamDShowFilter::QueryVendorInfo(LPWSTR* pVendorInfo) {
    if (!pVendorInfo) return E_POINTER;
    *pVendorInfo = AllocCoTaskWideString(L"SmartCam Suite (Rubens Rainelli - PEFL v1.0)");
    return (*pVendorInfo) ? S_OK : E_OUTOFMEMORY;
}

STDMETHODIMP_(ULONG) SmartCamDShowFilter::GetMiscFlags() {
    return AM_FILTER_MISC_FLAGS_IS_SOURCE;
}

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam
