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

use std::ffi::c_void;
use windows::{
    core::{GUID, PCWSTR, PWSTR},
    Win32::{
        Media::MediaFoundation::{
            IMFActivate, IMFAttributes, IMFSourceReader, MFCreateAttributes, MFCreateMediaType,
            MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFMediaType_Video,
            MFShutdown, MFStartup, MFVideoFormat_RGB32, MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_CATEGORY,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_MT_FRAME_RATE,
            MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READERF_ERROR,
            MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
            MF_VERSION,
        },
        System::{
            Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED},
            Registry::{
                RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
            },
        },
    },
};

use crate::{
    header::SMARTCAM_MF_CLSID_STR,
    ir::is_ir_camera_name,
    types::{CameraCaptureConfig, CameraDeviceInfo, CameraSource},
};

/// `KSCATEGORY_SENSOR_CAMERA` (`{24E552D7-6523-47F7-A647-D3465BF1F5CA}`) used by Windows 10/11
/// for Infrared / Windows Hello UVC cameras.
const KSCATEGORY_SENSOR_CAMERA: GUID = GUID::from_u128(0x24E552D7_6523_47F7_A647_D3465BF1F5CA);

pub fn mf_set_attribute_size(
    attrs: &IMFAttributes,
    guid: &GUID,
    width: u32,
    height: u32,
) -> windows::core::Result<()> {
    unsafe { attrs.SetUINT64(guid, ((width as u64) << 32) | (height as u64)) }
}

pub fn mf_set_attribute_ratio(
    attrs: &IMFAttributes,
    guid: &GUID,
    num: u32,
    den: u32,
) -> windows::core::Result<()> {
    unsafe { attrs.SetUINT64(guid, ((num as u64) << 32) | (den as u64)) }
}

pub fn mf_get_attribute_size(
    attrs: &IMFAttributes,
    guid: &GUID,
    width: &mut u32,
    height: &mut u32,
) -> windows::core::Result<()> {
    unsafe {
        let val = attrs.GetUINT64(guid)?;
        *width = (val >> 32) as u32;
        *height = (val & 0xFFFF_FFFF) as u32;
        Ok(())
    }
}

pub fn mf_get_attribute_ratio(
    attrs: &IMFAttributes,
    guid: &GUID,
    num: &mut u32,
    den: &mut u32,
) -> windows::core::Result<()> {
    unsafe {
        let val = attrs.GetUINT64(guid)?;
        *num = (val >> 32) as u32;
        *den = (val & 0xFFFF_FFFF) as u32;
        Ok(())
    }
}

unsafe fn pwstr_to_string(pwstr: PWSTR) -> String {
    if pwstr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *pwstr.0.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(pwstr.0, len);
    String::from_utf16_lossy(slice)
}

/// Extracts the normalized PnP instance key `<bus>\<dev>\<inst>` from a Media Foundation symbolic link
/// `\\?\<bus>#<dev>#<inst>#{<guid>}\...`.
fn pnp_instance_key_from_symlink(symbolic_link: &str) -> Option<String> {
    let trimmed = symbolic_link
        .trim_start_matches(r"\\?\")
        .trim_start_matches(r"\?\");
    let parts: Vec<&str> = trimmed.split('#').collect();
    if parts.len() >= 3 && !parts[0].is_empty() && !parts[1].is_empty() && !parts[2].is_empty() {
        Some(format!(r"{}\{}\{}", parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

/// Resolves the hardware PnP `FriendlyName` (or `DeviceDesc`) from `HKLM\SYSTEM\CurrentControlSet\Enum\<bus>\<dev>\<inst>`
/// when Media Foundation returns a generic placeholder like `"AvStream Media Device"` for sensor cameras.
fn resolve_pnp_friendly_name(symbolic_link: &str) -> Option<String> {
    let pnp_key = pnp_instance_key_from_symlink(symbolic_link)?;
    let reg_path = format!(r"SYSTEM\CurrentControlSet\Enum\{}", pnp_key);
    let wide_path: Vec<u16> = reg_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_path.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_err()
        {
            return None;
        }

        let mut result = None;
        for val_name in ["FriendlyName", "DeviceDesc"] {
            let wide_val: Vec<u16> = val_name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut buf = [0u16; 256];
            let mut byte_len = (buf.len() * 2) as u32;
            if RegQueryValueExW(
                hkey,
                PCWSTR(wide_val.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr() as *mut u8),
                Some(&mut byte_len),
            )
            .is_ok()
                && byte_len >= 2
            {
                let u16_len = (byte_len as usize / 2).saturating_sub(1);
                let raw = String::from_utf16_lossy(&buf[..u16_len])
                    .trim_matches('\0')
                    .trim()
                    .to_string();
                let cleaned = if let Some((_, after_semi)) = raw.rsplit_once(';') {
                    after_semi.trim().to_string()
                } else {
                    raw
                };
                if !cleaned.is_empty() {
                    result = Some(cleaned);
                    break;
                }
            }
        }

        let _ = RegCloseKey(hkey);
        result
    }
}

unsafe fn collect_hardware_camera_activates() -> Vec<(IMFActivate, CameraDeviceInfo)> {
    let mut results: Vec<(IMFActivate, CameraDeviceInfo)> = Vec::new();
    let mut seen_keys: Vec<String> = Vec::new();

    for (category_opt, is_sensor_cat) in [(None, false), (Some(KSCATEGORY_SENSOR_CAMERA), true)] {
        let mut attr = None;
        if MFCreateAttributes(&mut attr, 2).is_err() {
            continue;
        }
        let Some(attr) = attr else {
            continue;
        };

        let _ = attr.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        );
        if let Some(ref cat_guid) = category_opt {
            let _ = attr.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_CATEGORY, cat_guid);
        }

        let mut pp_devices: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;

        if MFEnumDeviceSources(&attr, &mut pp_devices, &mut count).is_ok() && !pp_devices.is_null() {
            let activates = std::slice::from_raw_parts(pp_devices, count as usize);
            for activate in activates.iter().flatten() {
                let mut friendly_name = String::new();
                let mut symbolic_link = String::new();

                let mut name_ptr = PWSTR::null();
                let mut name_len = 0u32;
                if activate
                    .GetAllocatedString(
                        &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                        &mut name_ptr,
                        &mut name_len,
                    )
                    .is_ok()
                    && !name_ptr.is_null()
                {
                    friendly_name = pwstr_to_string(name_ptr);
                    CoTaskMemFree(Some(name_ptr.0 as *mut c_void));
                }

                let mut sym_ptr = PWSTR::null();
                let mut sym_len = 0u32;
                if activate
                    .GetAllocatedString(
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                        &mut sym_ptr,
                        &mut sym_len,
                    )
                    .is_ok()
                    && !sym_ptr.is_null()
                {
                    symbolic_link = pwstr_to_string(sym_ptr);
                    CoTaskMemFree(Some(sym_ptr.0 as *mut c_void));
                }

                if friendly_name.is_empty()
                    || friendly_name.eq_ignore_ascii_case("AvStream Media Device")
                {
                    if let Some(pnp_name) = resolve_pnp_friendly_name(&symbolic_link) {
                        friendly_name = pnp_name;
                    }
                }

                if friendly_name == "SmartCam"
                    || friendly_name.contains("SmartCam Virtual Camera")
                    || symbolic_link
                        .to_ascii_uppercase()
                        .contains(&SMARTCAM_MF_CLSID_STR.to_ascii_uppercase())
                {
                    continue;
                }

                let dedup_key = pnp_instance_key_from_symlink(&symbolic_link)
                    .unwrap_or_else(|| symbolic_link.clone())
                    .to_ascii_lowercase();
                if !dedup_key.is_empty() && seen_keys.contains(&dedup_key) {
                    continue;
                }
                if !dedup_key.is_empty() {
                    seen_keys.push(dedup_key);
                }

                let is_infrared =
                    is_sensor_cat || is_ir_camera_name(&friendly_name, &symbolic_link);
                let index = results.len() as u32;
                results.push((
                    activate.clone(),
                    CameraDeviceInfo {
                        index,
                        friendly_name,
                        symbolic_link,
                        is_infrared,
                    },
                ));
            }
            CoTaskMemFree(Some(pp_devices as *mut c_void));
        }
    }

    results
}

/// Lists all physical hardware video capture devices connected to the Windows system
/// (including standard RGB cameras and Infrared / Windows Hello sensor cameras).
pub fn enumerate_camera_devices() -> Vec<CameraDeviceInfo> {
    let mut devices = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if MFStartup(MF_VERSION, 0).is_err() {
            CoUninitialize();
            return devices;
        }

        devices = collect_hardware_camera_activates()
            .into_iter()
            .map(|(_, info)| info)
            .collect();

        let _ = MFShutdown();
        CoUninitialize();
    }
    devices
}

/// Native Windows Media Foundation hardware webcam capture (`IMFSourceReader`).
#[derive(Default)]
pub struct MfCameraCapture {
    reader: Option<IMFSourceReader>,
    actual_config: CameraCaptureConfig,
    frame_counter: u64,
    mf_started: bool,
    is_open: bool,
}

unsafe impl Send for MfCameraCapture {}

impl MfCameraCapture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn actual_config(&self) -> &CameraCaptureConfig {
        &self.actual_config
    }

    pub fn frame_counter(&self) -> u64 {
        self.frame_counter
    }

    /// Zero-intermediate-copy fast path for C API callers providing a pre-allocated destination buffer.
    pub unsafe fn read_frame_into_raw(
        &mut self,
        dst_ptr: *mut u8,
        dst_cap: usize,
        scratch: &mut Vec<u8>,
    ) -> Option<(u32, u32)> {
        if !self.is_open || dst_ptr.is_null() {
            return None;
        }
        if self.actual_config.bit_depth == 64 {
            let (w, h) = self.read_frame(scratch)?;
            let needed = (w as usize) * (h as usize) * 4;
            if needed <= dst_cap && scratch.len() >= needed {
                std::ptr::copy_nonoverlapping(scratch.as_ptr(), dst_ptr, needed);
                return Some((w, h));
            }
            return None;
        }

        let reader = self.reader.as_ref()?;
        let mut actual_stream_index = 0u32;
        let mut stream_flags = 0u32;
        let mut timestamp_100ns = 0i64;
        let mut sample = None;

        let hr = reader.ReadSample(
            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
            0,
            Some(&mut actual_stream_index),
            Some(&mut stream_flags),
            Some(&mut timestamp_100ns),
            Some(&mut sample),
        );

        if hr.is_err() || (stream_flags & MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
            return None;
        }
        let sample = sample?;
        let buffer = sample.ConvertToContiguousBuffer().ok()?;

        let mut raw_ptr = std::ptr::null_mut();
        let mut cur_len = 0u32;
        if buffer.Lock(&mut raw_ptr, None, Some(&mut cur_len)).is_err() || raw_ptr.is_null() {
            return None;
        }

        let w = self.actual_config.width;
        let h = self.actual_config.height;
        let expected_bytes_32 = (w as usize) * (h as usize) * 4;
        let expected_bytes_l8 = (w as usize) * (h as usize);

        let success = if (cur_len as usize) >= expected_bytes_32 && dst_cap >= expected_bytes_32 {
            std::ptr::copy_nonoverlapping(raw_ptr, dst_ptr, expected_bytes_32);
            true
        } else if (cur_len as usize) >= expected_bytes_l8
            && expected_bytes_l8 > 0
            && dst_cap >= expected_bytes_32
        {
            let src_slice = std::slice::from_raw_parts(raw_ptr, expected_bytes_l8);
            let dst_slice = std::slice::from_raw_parts_mut(dst_ptr, expected_bytes_32);
            for (i, &lum) in src_slice.iter().enumerate() {
                let base = i * 4;
                dst_slice[base] = lum;
                dst_slice[base + 1] = lum;
                dst_slice[base + 2] = lum;
                dst_slice[base + 3] = 255;
            }
            true
        } else {
            false
        };

        let _ = buffer.Unlock();

        if success {
            self.frame_counter += 1;
            Some((w, h))
        } else {
            None
        }
    }
}

impl CameraSource for MfCameraCapture {
    fn open(&mut self, config: &CameraCaptureConfig) -> bool {
        self.close();
        self.actual_config = config.clone();

        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_err() && hr != windows::core::HRESULT(-2147417850) {
                // RPC_E_CHANGED_MODE
                return false;
            }

            if MFStartup(MF_VERSION, 0).is_err() {
                return false;
            }
            self.mf_started = true;

            let activates = collect_hardware_camera_activates();
            if activates.is_empty() {
                self.close();
                return false;
            }

            let mut selected_idx = 0usize;
            if !config.device_id.is_empty() {
                for (i, (_, info)) in activates.iter().enumerate() {
                    if info.symbolic_link.contains(&config.device_id) {
                        selected_idx = i;
                        break;
                    }
                }
            } else if (config.device_index as usize) < activates.len() {
                selected_idx = config.device_index as usize;
            }

            let media_source = if let Some((ref act, _)) = activates.get(selected_idx) {
                act.ActivateObject::<windows::Win32::Media::MediaFoundation::IMFMediaSource>()
            } else {
                self.close();
                return false;
            };
            drop(activates);

            let media_source = match media_source {
                Ok(ms) => ms,
                Err(_) => {
                    self.close();
                    return false;
                }
            };

            // Enable video processing in source reader (converts NV12/YUY2/L8/L16 to RGB32)
            let mut reader_attr = None;
            let _ = MFCreateAttributes(&mut reader_attr, 2);
            if let Some(ref r_attr) = reader_attr {
                let _ = r_attr.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);
            }

            let reader =
                match MFCreateSourceReaderFromMediaSource(&media_source, reader_attr.as_ref()) {
                    Ok(r) => r,
                    Err(_) => {
                        self.close();
                        return false;
                    }
                };

            // Request target format (MFVideoFormat_RGB32)
            if let Ok(out_t) = MFCreateMediaType() {
                let _ = out_t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video);
                let _ = out_t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32);
                let _ =
                    mf_set_attribute_size(&out_t, &MF_MT_FRAME_SIZE, config.width, config.height);
                let _ = mf_set_attribute_ratio(&out_t, &MF_MT_FRAME_RATE, config.target_fps, 1);

                let set_hr = reader.SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    None,
                    &out_t,
                );
                if set_hr.is_err() {
                    // Fallback: keep camera's native resolution/fps, request RGB32
                    if let Ok(fb_t) = MFCreateMediaType() {
                        let _ = fb_t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video);
                        let _ = fb_t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32);
                        let _ = reader.SetCurrentMediaType(
                            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                            None,
                            &fb_t,
                        );
                    }
                }
            }

            // Query negotiated resolution & framerate
            if let Ok(current_type) =
                reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
            {
                let mut w = 0u32;
                let mut h = 0u32;
                if mf_get_attribute_size(&current_type, &MF_MT_FRAME_SIZE, &mut w, &mut h).is_ok() {
                    self.actual_config.width = w;
                    self.actual_config.height = h;
                }
                let mut fps_n = 0u32;
                let mut fps_d = 1u32;
                if mf_get_attribute_ratio(&current_type, &MF_MT_FRAME_RATE, &mut fps_n, &mut fps_d)
                    .is_ok()
                    && fps_d > 0
                {
                    self.actual_config.target_fps = fps_n / fps_d;
                }
            }

            self.reader = Some(reader);
            self.is_open = true;
            true
        }
    }

    fn read_frame(&mut self, dst_bgra: &mut Vec<u8>) -> Option<(u32, u32)> {
        if !self.is_open {
            return None;
        }
        let reader = self.reader.as_ref()?;

        unsafe {
            let mut actual_stream_index = 0u32;
            let mut stream_flags = 0u32;
            let mut timestamp_100ns = 0i64;
            let mut sample = None;

            let hr = reader.ReadSample(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                0,
                Some(&mut actual_stream_index),
                Some(&mut stream_flags),
                Some(&mut timestamp_100ns),
                Some(&mut sample),
            );

            if hr.is_err() || (stream_flags & MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                return None;
            }
            let sample = sample?;

            let buffer = match sample.ConvertToContiguousBuffer() {
                Ok(b) => b,
                Err(_) => return None,
            };

            let mut raw_ptr = std::ptr::null_mut();
            let mut cur_len = 0u32;
            if buffer.Lock(&mut raw_ptr, None, Some(&mut cur_len)).is_err() || raw_ptr.is_null() {
                return None;
            }

            let w = self.actual_config.width;
            let h = self.actual_config.height;
            let expected_bytes_32 = (w as usize) * (h as usize) * 4;
            let expected_bytes_64 = (w as usize) * (h as usize) * 8;
            let expected_bytes_l8 = (w as usize) * (h as usize);

            let success = if self.actual_config.bit_depth == 64 {
                dst_bgra.resize(expected_bytes_64, 0);
                if (cur_len as usize) >= expected_bytes_64 {
                    std::ptr::copy_nonoverlapping(
                        raw_ptr,
                        dst_bgra.as_mut_ptr(),
                        expected_bytes_64,
                    );
                    true
                } else if (cur_len as usize) >= expected_bytes_32 {
                    let src_slice = std::slice::from_raw_parts(raw_ptr, expected_bytes_32);
                    let dst_u16 = std::slice::from_raw_parts_mut(
                        dst_bgra.as_mut_ptr() as *mut u16,
                        (w * h * 4) as usize,
                    );
                    for i in 0..expected_bytes_32 {
                        dst_u16[i] = (src_slice[i] as u16) * 257;
                    }
                    true
                } else {
                    false
                }
            } else if (cur_len as usize) >= expected_bytes_32 {
                dst_bgra.resize(expected_bytes_32, 0);
                std::ptr::copy_nonoverlapping(raw_ptr, dst_bgra.as_mut_ptr(), expected_bytes_32);
                true
            } else if (cur_len as usize) >= expected_bytes_l8 && expected_bytes_l8 > 0 {
                // L8 monochrome IR sensor fallback -> expand 8-bit greyscale to BGRA32
                dst_bgra.resize(expected_bytes_32, 255);
                let src_slice = std::slice::from_raw_parts(raw_ptr, expected_bytes_l8);
                for (i, &lum) in src_slice.iter().enumerate() {
                    let base = i * 4;
                    dst_bgra[base] = lum;
                    dst_bgra[base + 1] = lum;
                    dst_bgra[base + 2] = lum;
                    dst_bgra[base + 3] = 255;
                }
                true
            } else {
                false
            };

            let _ = buffer.Unlock();

            if success {
                self.frame_counter += 1;
                Some((w, h))
            } else {
                None
            }
        }
    }

    fn close(&mut self) {
        self.is_open = false;
        self.reader = None;
        if self.mf_started {
            unsafe {
                let _ = MFShutdown();
            }
            self.mf_started = false;
        }
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
}

impl Drop for MfCameraCapture {
    fn drop(&mut self) {
        self.close();
    }
}
