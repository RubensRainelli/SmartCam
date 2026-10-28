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

use windows::core::GUID;

/// CLSID for SmartCam Windows Media Foundation Virtual Camera Source:
/// `{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}`
pub const CLSID_SMARTCAM_MF_SOURCE: GUID =
    GUID::from_u128(0x9d4b8e71_3f5a_4e9c_a8b2_71c9d0e54f21);

/// PINNAME_VIDEO_CAPTURE: `{FB6C4281-0353-11D1-905F-0000C0CC16BA}`
pub const PINNAME_VIDEO_CAPTURE: GUID =
    GUID::from_u128(0xfb6c4281_0353_11d1_905f_0000c0cc16ba);

/// MFVideoFormat_ARGB64: `{6e6415e6-5c24-425f-93cd-80102b3d1cca}`
pub const MF_VIDEO_FORMAT_ARGB64: GUID =
    GUID::from_u128(0x6e6415e6_5c24_425f_93cd_80102b3d1cca);

/// KSCATEGORY_VIDEO_CAMERA: `{E5323777-F976-4F5B-9B55-B94699C46E44}`
pub const KSCATEGORY_VIDEO_CAMERA: GUID =
    GUID::from_u128(0xe5323777_f976_4f5b_9b55_b94699c46e44);

/// Default stream width (1440).
pub const DEFAULT_STREAM_WIDTH: u32 = 1440;

/// Default stream height (1080).
pub const DEFAULT_STREAM_HEIGHT: u32 = 1080;

/// Default target framerate (30).
pub const DEFAULT_STREAM_FPS: u32 = 30;

/// Default sample duration in 100-ns units (~33.33ms = 333,333 units).
pub const DEFAULT_SAMPLE_DURATION_100NS: i64 = 333_333;
