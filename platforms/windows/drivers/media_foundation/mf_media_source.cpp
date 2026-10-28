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

#include "mf_media_source.h"
#include <ks.h>
#include <ksmedia.h>
#include <propvarutil.h>
#include <new>
#include <vector>

#if defined(_MSC_VER)
#pragma comment(lib, "mfplat.lib")
#pragma comment(lib, "mfuuid.lib")
#pragma comment(lib, "propsys.lib")
#endif

namespace smartcam {
namespace platforms {
namespace windows {

namespace {
// PINNAME_VIDEO_CAPTURE {FB6C4281-0353-11D1-905F-0000C0CC16BA}
constexpr GUID kPinNameVideoCapture = {
    0xfb6c4281, 0x0353, 0x11d1, {0x90, 0x5f, 0x00, 0x00, 0xc0, 0xcc, 0x16, 0xba}
};
}  // namespace

// ============================================================================
// SmartCamMfMediaStream Implementation
// ============================================================================

SmartCamMfMediaStream::SmartCamMfMediaStream(SmartCamMfMediaSource* parent, IMFStreamDescriptor* stream_desc)
    : parent_source_(parent), stream_descriptor_(stream_desc) {
    if (parent_source_) {
        parent_source_->AddRef();
    }
}

SmartCamMfMediaStream::~SmartCamMfMediaStream() {
    ShutdownStream();
    if (parent_source_) {
        parent_source_->Release();
        parent_source_ = nullptr;
    }
}

HRESULT SmartCamMfMediaStream::Initialize() {
    HRESULT hr = MFCreateEventQueue(&event_queue_);
    if (FAILED(hr)) return hr;

    hr = MFCreateAttributes(&stream_attributes_, 4);
    if (FAILED(hr)) return hr;

    stream_attributes_->SetGUID(MF_DEVICESTREAM_STREAM_CATEGORY, kPinNameVideoCapture);
    stream_attributes_->SetUINT32(MF_DEVICESTREAM_STREAM_ID, 0);
    stream_attributes_->SetUINT32(MF_DEVICESTREAM_FRAMESERVER_SHARED, 1);

    ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaStream::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) return E_POINTER;
    if (riid == IID_IUnknown || riid == IID_IMFMediaEventGenerator || riid == IID_IMFMediaStream) {
        *ppv = static_cast<IMFMediaStream*>(this);
        AddRef();
        return S_OK;
    }
    *ppv = nullptr;
    return E_NOINTERFACE;
}

STDMETHODIMP_(ULONG) SmartCamMfMediaStream::AddRef() {
    return InterlockedIncrement(&ref_count_);
}

STDMETHODIMP_(ULONG) SmartCamMfMediaStream::Release() {
    const ULONG count = InterlockedDecrement(&ref_count_);
    if (count == 0) {
        delete this;
    }
    return count;
}

STDMETHODIMP SmartCamMfMediaStream::GetEvent(DWORD dwFlags, IMFMediaEvent** ppEvent) {
    Microsoft::WRL::ComPtr<IMFMediaEventQueue> queue;
    {
        std::lock_guard<std::mutex> lock(mutex_);
        if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
        queue = event_queue_;
    }
    return queue->GetEvent(dwFlags, ppEvent);
}

STDMETHODIMP SmartCamMfMediaStream::BeginGetEvent(IMFAsyncCallback* pCallback, IUnknown* punkState) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->BeginGetEvent(pCallback, punkState);
}

STDMETHODIMP SmartCamMfMediaStream::EndGetEvent(IMFAsyncResult* pResult, IMFMediaEvent** ppEvent) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->EndGetEvent(pResult, ppEvent);
}

STDMETHODIMP SmartCamMfMediaStream::QueueEvent(MediaEventType met, REFGUID guidExtendedType, HRESULT hrStatus, const PROPVARIANT* pvValue) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->QueueEventParamVar(met, guidExtendedType, hrStatus, pvValue);
}

STDMETHODIMP SmartCamMfMediaStream::GetMediaSource(IMFMediaSource** ppMediaSource) {
    if (!ppMediaSource) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !parent_source_) return MF_E_SHUTDOWN;
    *ppMediaSource = parent_source_;
    (*ppMediaSource)->AddRef();
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaStream::GetStreamDescriptor(IMFStreamDescriptor** ppStreamDescriptor) {
    if (!ppStreamDescriptor) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !stream_descriptor_) return MF_E_SHUTDOWN;
    *ppStreamDescriptor = stream_descriptor_.Get();
    (*ppStreamDescriptor)->AddRef();
    return S_OK;
}

void SmartCamMfMediaStream::RenderStandbyPattern(uint8_t* dst, uint32_t width, uint32_t height) noexcept {
    const uint32_t bar_offset = static_cast<uint32_t>((sample_count_ * 4) % width);
    for (uint32_t y = 0; y < height; ++y) {
        uint8_t* row = dst + static_cast<size_t>(y) * width * 4;
        const uint8_t grad_y = static_cast<uint8_t>((y * 180) / height + 30);
        for (uint32_t x = 0; x < width; ++x) {
            const bool is_pulse = ((x + bar_offset) % 120) < 6;
            row[x * 4 + 0] = is_pulse ? 240 : 56;      // B
            row[x * 4 + 1] = is_pulse ? 200 : grad_y;  // G
            row[x * 4 + 2] = is_pulse ? 80  : 24;      // R
            row[x * 4 + 3] = 255;                      // A
        }
    }
}

STDMETHODIMP SmartCamMfMediaStream::RequestSample(IUnknown* pToken) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;

    const DWORD buffer_size = width_ * height_ * 4;
    Microsoft::WRL::ComPtr<IMFMediaBuffer> media_buffer;

    // Select or reuse a buffer from the fixed 3-slot pool when not held downstream
    for (size_t i = 0; i < 3; ++i) {
        const size_t slot = (pool_cursor_ + i) % 3;
        if (!buffer_pool_[slot]) {
            if (SUCCEEDED(MFCreateMemoryBuffer(buffer_size, &buffer_pool_[slot]))) {
                media_buffer = buffer_pool_[slot];
                pool_cursor_ = (slot + 1) % 3;
                break;
            }
        } else {
            // Test if buffer_pool_[slot] has ref_count == 1 (only held by buffer_pool_[slot])
            buffer_pool_[slot].Get()->AddRef();
            const ULONG rc_after_add = buffer_pool_[slot].Get()->Release();
            DWORD max_len = 0;
            if (rc_after_add == 1 && SUCCEEDED(buffer_pool_[slot]->GetMaxLength(&max_len)) && max_len >= buffer_size) {
                media_buffer = buffer_pool_[slot];
                pool_cursor_ = (slot + 1) % 3;
                break;
            }
        }
    }

    if (!media_buffer) {
        HRESULT hr_alloc = MFCreateMemoryBuffer(buffer_size, &media_buffer);
        if (FAILED(hr_alloc)) return hr_alloc;
    }

    BYTE* raw_ptr = nullptr;
    HRESULT hr = media_buffer->Lock(&raw_ptr, nullptr, nullptr);
    if (FAILED(hr)) return hr;

    uint32_t ipc_w = 0, ipc_h = 0;
    uint64_t ts_100ns = 0;
    bool got_ipc_frame = false;

    if (!ipc_.IsOpen()) {
        ipc_.OpenOrCreate(SharedFrameIpc::kDefaultMappingName, SharedFrameIpc::kDefaultEventName, false);
    }

    if (ipc_.IsOpen()) {
        ipc_.TouchConsumerHeartbeat();
        const SharedFrameHeader* hdr = ipc_.Header();
        if (hdr && hdr->magic == SharedFrameHeader::kMagic &&
            hdr->width > 0 && hdr->height > 0) {
            if (hdr->width == width_ && hdr->height == height_) {
                got_ipc_frame = ipc_.ReadFrame(raw_ptr, buffer_size, ipc_w, ipc_h, ts_100ns);
            } else {
                const size_t src_bytes = static_cast<size_t>(hdr->width) * hdr->height * 4;
                if (src_bytes <= SharedFrameHeader::kMaxFrameBytes) {
                    if (ipc_frame_scratch_.size() < src_bytes) {
                        ipc_frame_scratch_.resize(src_bytes);
                    }
                    if (ipc_.ReadFrame(ipc_frame_scratch_.data(), src_bytes, ipc_w, ipc_h, ts_100ns) &&
                        ipc_w > 0 && ipc_h > 0) {
                        for (uint32_t dy = 0; dy < height_; ++dy) {
                            const uint32_t sy = (dy * ipc_h) / height_;
                            uint8_t* dst_row = raw_ptr + static_cast<size_t>(dy) * width_ * 4;
                            const uint8_t* src_row = ipc_frame_scratch_.data() + static_cast<size_t>(sy) * ipc_w * 4;
                            for (uint32_t dx = 0; dx < width_; ++dx) {
                                const uint32_t sx = (dx * ipc_w) / width_;
                                dst_row[dx * 4 + 0] = src_row[sx * 4 + 0];
                                dst_row[dx * 4 + 1] = src_row[sx * 4 + 1];
                                dst_row[dx * 4 + 2] = src_row[sx * 4 + 2];
                                dst_row[dx * 4 + 3] = 255;
                            }
                        }
                        got_ipc_frame = true;
                    }
                }
            }
        }
    }

    if (!got_ipc_frame) {
        RenderStandbyPattern(raw_ptr, width_, height_);
        LARGE_INTEGER qpc{}, freq{};
        QueryPerformanceCounter(&qpc);
        QueryPerformanceFrequency(&freq);
        ts_100ns = (freq.QuadPart > 0) ? static_cast<uint64_t>((qpc.QuadPart * 10000000ull) / freq.QuadPart) : 0;
    }

    media_buffer->Unlock();
    media_buffer->SetCurrentLength(buffer_size);

    Microsoft::WRL::ComPtr<IMFSample> sample;
    hr = MFCreateSample(&sample);
    if (FAILED(hr)) return hr;

    sample->AddBuffer(media_buffer.Get());
    sample->SetSampleTime(static_cast<LONGLONG>(ts_100ns));
    sample->SetSampleDuration(333333LL); // 30 FPS (33.33 ms in 100-ns units)
    if (pToken) {
        sample->SetUnknown(MFSampleExtension_Token, pToken);
    }

    ++sample_count_;
    return event_queue_->QueueEventParamUnk(MEMediaSample, GUID_NULL, S_OK, sample.Get());
}

HRESULT SmartCamMfMediaStream::StartStream(const PROPVARIANT* start_pos) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    if (ipc_.IsOpen()) {
        ipc_.TouchConsumerHeartbeat();
    }
    return event_queue_->QueueEventParamVar(MEStreamStarted, GUID_NULL, S_OK, start_pos);
}

HRESULT SmartCamMfMediaStream::StopStream() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->QueueEventParamVar(MEStreamStopped, GUID_NULL, S_OK, nullptr);
}

HRESULT SmartCamMfMediaStream::PauseStream() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->QueueEventParamVar(MEStreamPaused, GUID_NULL, S_OK, nullptr);
}

HRESULT SmartCamMfMediaStream::ShutdownStream() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_) return S_OK;
    is_shutdown_ = true;
    for (auto& buf : buffer_pool_) {
        buf.Reset();
    }
    ipc_frame_scratch_.clear();
    ipc_frame_scratch_.shrink_to_fit();
    ipc_.Close();
    if (event_queue_) {
        event_queue_->Shutdown();
    }
    return S_OK;
}

// ============================================================================
// SmartCamMfMediaSource Implementation
// ============================================================================

SmartCamMfMediaSource::SmartCamMfMediaSource() = default;

SmartCamMfMediaSource::~SmartCamMfMediaSource() {
    Shutdown();
}

HRESULT SmartCamMfMediaSource::Initialize() {
    HRESULT hr = MFCreateEventQueue(&event_queue_);
    if (FAILED(hr)) return hr;

    hr = MFCreateAttributes(&source_attributes_, 4);
    if (FAILED(hr)) return hr;

    // Configure MediaTypes:
    //  [0] 1440x1080 @ 30fps RGB32/BGRA32 (32-bit SDR universal compatibility)
    //  [1] 1440x1080 @ 30fps ARGB64/BGRA64 (64-bit Deep Color studio 16-bit per channel)
    Microsoft::WRL::ComPtr<IMFMediaType> media_type;
    hr = MFCreateMediaType(&media_type);
    if (FAILED(hr)) return hr;

    media_type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    media_type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_RGB32);
    media_type->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    MFSetAttributeSize(media_type.Get(), MF_MT_FRAME_SIZE, 1440, 1080);
    MFSetAttributeRatio(media_type.Get(), MF_MT_FRAME_RATE, 30, 1);
    MFSetAttributeRatio(media_type.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    media_type->SetUINT32(MF_MT_DEFAULT_STRIDE, 1440 * 4);
    media_type->SetUINT32(MF_MT_FIXED_SIZE_SAMPLES, TRUE);
    media_type->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT, TRUE);
    media_type->SetUINT32(MF_MT_SAMPLE_SIZE, 1440 * 1080 * 4);

    // MFVideoFormat_ABGR64 / ARGB64 (64-bit per pixel: 16-bit per channel Deep Color)
    static constexpr GUID kMfVideoFormatArgb64 = {
        0x6e6415e6, 0x5c24, 0x425f, {0x93, 0xcd, 0x80, 0x10, 0x2b, 0x3d, 0x1c, 0xca}
    };
    Microsoft::WRL::ComPtr<IMFMediaType> media_type_64;
    hr = MFCreateMediaType(&media_type_64);
    if (FAILED(hr)) return hr;

    media_type_64->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    media_type_64->SetGUID(MF_MT_SUBTYPE, kMfVideoFormatArgb64);
    media_type_64->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    MFSetAttributeSize(media_type_64.Get(), MF_MT_FRAME_SIZE, 1440, 1080);
    MFSetAttributeRatio(media_type_64.Get(), MF_MT_FRAME_RATE, 30, 1);
    MFSetAttributeRatio(media_type_64.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    media_type_64->SetUINT32(MF_MT_DEFAULT_STRIDE, 1440 * 8);
    media_type_64->SetUINT32(MF_MT_FIXED_SIZE_SAMPLES, TRUE);
    media_type_64->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT, TRUE);
    media_type_64->SetUINT32(MF_MT_SAMPLE_SIZE, 1440 * 1080 * 8);

    IMFMediaType* types[2] = {media_type.Get(), media_type_64.Get()};
    Microsoft::WRL::ComPtr<IMFStreamDescriptor> stream_desc;
    hr = MFCreateStreamDescriptor(0, 2, types, &stream_desc);
    if (FAILED(hr)) return hr;

    Microsoft::WRL::ComPtr<IMFMediaTypeHandler> handler;
    hr = stream_desc->GetMediaTypeHandler(&handler);
    if (FAILED(hr)) return hr;
    handler->SetCurrentMediaType(media_type.Get());

    IMFStreamDescriptor* descriptors[1] = {stream_desc.Get()};
    hr = MFCreatePresentationDescriptor(1, descriptors, &pres_descriptor_);
    if (FAILED(hr)) return hr;
    pres_descriptor_->SelectStream(0);

    video_stream_.Attach(new (std::nothrow) SmartCamMfMediaStream(this, stream_desc.Get()));
    if (!video_stream_) return E_OUTOFMEMORY;

    return video_stream_->Initialize();
}

STDMETHODIMP SmartCamMfMediaSource::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) return E_POINTER;
    if (riid == IID_IUnknown || riid == IID_IMFMediaEventGenerator ||
        riid == IID_IMFMediaSource || riid == IID_IMFMediaSourceEx) {
        *ppv = static_cast<IMFMediaSourceEx*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IMFGetService) {
        *ppv = static_cast<IMFGetService*>(this);
        AddRef();
        return S_OK;
    }
    *ppv = nullptr;
    return E_NOINTERFACE;
}

STDMETHODIMP_(ULONG) SmartCamMfMediaSource::AddRef() {
    return InterlockedIncrement(&ref_count_);
}

STDMETHODIMP_(ULONG) SmartCamMfMediaSource::Release() {
    const ULONG count = InterlockedDecrement(&ref_count_);
    if (count == 0) {
        delete this;
    }
    return count;
}

STDMETHODIMP SmartCamMfMediaSource::GetEvent(DWORD dwFlags, IMFMediaEvent** ppEvent) {
    Microsoft::WRL::ComPtr<IMFMediaEventQueue> queue;
    {
        std::lock_guard<std::mutex> lock(mutex_);
        if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
        queue = event_queue_;
    }
    return queue->GetEvent(dwFlags, ppEvent);
}

STDMETHODIMP SmartCamMfMediaSource::BeginGetEvent(IMFAsyncCallback* pCallback, IUnknown* punkState) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->BeginGetEvent(pCallback, punkState);
}

STDMETHODIMP SmartCamMfMediaSource::EndGetEvent(IMFAsyncResult* pResult, IMFMediaEvent** ppEvent) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->EndGetEvent(pResult, ppEvent);
}

STDMETHODIMP SmartCamMfMediaSource::QueueEvent(MediaEventType met, REFGUID guidExtendedType, HRESULT hrStatus, const PROPVARIANT* pvValue) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_) return MF_E_SHUTDOWN;
    return event_queue_->QueueEventParamVar(met, guidExtendedType, hrStatus, pvValue);
}

STDMETHODIMP SmartCamMfMediaSource::GetCharacteristics(DWORD* pdwCharacteristics) {
    if (!pdwCharacteristics) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_) return MF_E_SHUTDOWN;
    *pdwCharacteristics = MFMEDIASOURCE_IS_LIVE;
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaSource::CreatePresentationDescriptor(IMFPresentationDescriptor** ppPresentationDescriptor) {
    if (!ppPresentationDescriptor) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !pres_descriptor_) return MF_E_SHUTDOWN;
    return pres_descriptor_->Clone(ppPresentationDescriptor);
}

STDMETHODIMP SmartCamMfMediaSource::Start(IMFPresentationDescriptor* pPresentationDescriptor,
                                          const GUID* /*pguidTimeFormat*/,
                                          const PROPVARIANT* pvarStartPosition) {
    if (!pPresentationDescriptor) return E_INVALIDARG;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_ || !video_stream_) return MF_E_SHUTDOWN;

    PROPVARIANT start_pos;
    PropVariantInit(&start_pos);
    if (pvarStartPosition) {
        PropVariantCopy(&start_pos, pvarStartPosition);
    }

    event_queue_->QueueEventParamUnk(MENewStream, GUID_NULL, S_OK, video_stream_.Get());
    video_stream_->StartStream(&start_pos);
    HRESULT hr = event_queue_->QueueEventParamVar(MESourceStarted, GUID_NULL, S_OK, &start_pos);
    PropVariantClear(&start_pos);
    return hr;
}

STDMETHODIMP SmartCamMfMediaSource::Stop() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_ || !video_stream_) return MF_E_SHUTDOWN;
    video_stream_->StopStream();
    return event_queue_->QueueEventParamVar(MESourceStopped, GUID_NULL, S_OK, nullptr);
}

STDMETHODIMP SmartCamMfMediaSource::Pause() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !event_queue_ || !video_stream_) return MF_E_SHUTDOWN;
    video_stream_->PauseStream();
    return event_queue_->QueueEventParamVar(MESourcePaused, GUID_NULL, S_OK, nullptr);
}

STDMETHODIMP SmartCamMfMediaSource::Shutdown() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_) return S_OK;
    is_shutdown_ = true;
    if (video_stream_) {
        video_stream_->ShutdownStream();
        video_stream_.Reset();
    }
    if (event_queue_) {
        event_queue_->Shutdown();
    }
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaSource::GetSourceAttributes(IMFAttributes** ppAttributes) {
    if (!ppAttributes) return E_POINTER;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !source_attributes_) return MF_E_SHUTDOWN;
    *ppAttributes = source_attributes_.Get();
    (*ppAttributes)->AddRef();
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaSource::GetStreamAttributes(DWORD dwStreamIdentifier, IMFAttributes** ppAttributes) {
    if (!ppAttributes) return E_POINTER;
    if (dwStreamIdentifier != 0) return E_INVALIDARG;
    std::lock_guard<std::mutex> lock(mutex_);
    if (is_shutdown_ || !video_stream_) return MF_E_SHUTDOWN;
    *ppAttributes = video_stream_->GetAttributesRaw();
    if (*ppAttributes) (*ppAttributes)->AddRef();
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaSource::SetD3DManager(IUnknown* /*pManager*/) {
    return S_OK;
}

STDMETHODIMP SmartCamMfMediaSource::GetService(REFGUID /*guidService*/, REFIID riid, LPVOID* ppvObject) {
    return QueryInterface(riid, ppvObject);
}

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam
