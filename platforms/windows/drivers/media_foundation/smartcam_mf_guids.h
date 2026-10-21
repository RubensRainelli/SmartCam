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

#ifndef SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_GUIDS_H_
#define SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_GUIDS_H_

#include <windows.h>
#include <initguid.h>

// {9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}
// CLSID for SmartCam Windows 11 Media Foundation Virtual Camera Source
DEFINE_GUID(CLSID_SmartCamMfVirtualCameraSource,
            0x9d4b8e71, 0x3f5a, 0x4e9c, 0xa8, 0xb2, 0x71, 0xc9, 0xd0, 0xe5, 0x4f, 0x21);

// Windows 10/11 FrameServer Stream Attribute GUIDs (fallback definitions for SDKs/MinGW-w64)
#ifndef MF_DEVICESTREAM_STREAM_CATEGORY
DEFINE_GUID(MF_DEVICESTREAM_STREAM_CATEGORY,
            0x2939e7b8, 0xa62e, 0x4579, 0xb6, 0x74, 0xd4, 0x07, 0x3d, 0xfa, 0xbb, 0xb8);
#endif
#ifndef MF_DEVICESTREAM_STREAM_ID
DEFINE_GUID(MF_DEVICESTREAM_STREAM_ID,
            0x11bd5120, 0xd124, 0x446b, 0x88, 0xe6, 0x17, 0x03, 0x0d, 0x57, 0xff, 0xae);
#endif
#ifndef MF_DEVICESTREAM_FRAMESERVER_SHARED
DEFINE_GUID(MF_DEVICESTREAM_FRAMESERVER_SHARED,
            0x1cb378e9, 0xb279, 0x41d4, 0xaf, 0x97, 0x34, 0xa2, 0x43, 0xe6, 0x83, 0x20);
#endif

namespace smartcam {
namespace platforms {
namespace windows {

constexpr const wchar_t* kSmartCamClsidString = L"{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}";
constexpr const wchar_t* kSmartCamDefaultFriendlyName = L"SmartCam";

}  // namespace windows
}  // namespace platforms
}  // namespace smartcam

#endif  // SMARTCAM_PLATFORMS_WINDOWS_DRIVERS_MF_GUIDS_H_
