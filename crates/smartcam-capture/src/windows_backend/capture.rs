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
    core::{GUID, PWSTR},
    Win32::{
        Media::MediaFoundation::{
            IMFActivate, IMFAttributes, IMFSourceReader,
            MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromMediaSource,
            MFEnumDeviceSources, MFShutdown, MFStartup,
            MFVideoFormat_RGB32, MFMediaType_Video,
            MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
            MF_SOURCE_READERF_ERROR, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
        },
        System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED},
    },
};

use crate::{
    header::SMARTCAM_MF_CLSID_STR,
    ir::is_ir_camera_name,
    types::{CameraCaptureConfig, CameraDeviceInfo, CameraSource},
};

pub fn mf_set_attribute_size(attrs: &IMFAttributes, guid: &GUID, width: u32, height: u32) -> windows::core::Result<()> {
    unsafe { attrs.SetUINT64(guid, ((width as u64) << 32) | (height as u64)) }
}

pub fn mf_set_attribute_ratio(attrs: &IMFAttributes, guid: &GUID, num: u32, den: u32) -> windows::core::Result<()> {
    unsafe { attrs.SetUINT64(guid, ((num as u64) << 32) | (den as u64)) }
}

pub fn mf_get_attribute_size(attrs: &IMFAttributes, guid: &GUID, width: &mut u32, height: &mut u32) -> windows::core::Result<()> {
    unsafe {
        let val = attrs.GetUINT64(guid)?;
        *width = (val >> 32) as u32;
        *height = (val & 0xFFFF_FFFF) as u32;
        Ok(())
    }
}

pub fn mf_get_attribute_ratio(attrs: &IMFAttributes, guid: &GUID, num: &mut u32, den: &mut u32) -> windows::core::Result<()> {
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

/// Lists all physical hardware video capture devices connected to the Windows system.
pub fn enumerate_camera_devices() -> Vec<CameraDeviceInfo> {
    let mut devices = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if MFStartup(MF_VERSION, 0).is_err() {
            CoUninitialize();
            return devices;
        }

        let mut attr = None;
        if MFCreateAttributes(&mut attr, 1).is_ok() {
            if let Some(attr) = attr {
                let _ = attr.SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                );

                let mut pp_devices: *mut Option<IMFActivate> = std::ptr::null_mut();
                let mut count = 0u32;

                if MFEnumDeviceSources(&attr, &mut pp_devices, &mut count).is_ok() && !pp_devices.is_null() {
                    let activates = std::slice::from_raw_parts(pp_devices, count as usize);
                    for activate_opt in activates.iter() {
                        if let Some(activate) = activate_opt {
                            let mut friendly_name = String::new();
                            let mut symbolic_link = String::new();

                            let mut name_ptr = PWSTR::null();
                            let mut name_len = 0u32;
                            if activate.GetAllocatedString(
                                &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                                &mut name_ptr,
                                &mut name_len,
                            ).is_ok() && !name_ptr.is_null() {
                                friendly_name = pwstr_to_string(name_ptr);
                                CoTaskMemFree(Some(name_ptr.0 as *mut c_void));
                            }

                            let mut sym_ptr = PWSTR::null();
                            let mut sym_len = 0u32;
                            if activate.GetAllocatedString(
                                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                                &mut sym_ptr,
                                &mut sym_len,
                            ).is_ok() && !sym_ptr.is_null() {
                                symbolic_link = pwstr_to_string(sym_ptr);
                                CoTaskMemFree(Some(sym_ptr.0 as *mut c_void));
                            }

                            if friendly_name != "SmartCam"
                                && !friendly_name.contains("SmartCam Virtual Camera")
                                && !symbolic_link.contains(SMARTCAM_MF_CLSID_STR)
                            {
                                let is_infrared = is_ir_camera_name(&friendly_name, &symbolic_link);
                                let index = devices.len() as u32;
                                devices.push(CameraDeviceInfo {
                                    index,
                                    friendly_name,
                                    symbolic_link,
                                    is_infrared,
                                });
                            }
                        }
                    }
                    CoTaskMemFree(Some(pp_devices as *mut c_void));
                }
            }
        }

        let _ = MFShutdown();
        CoUninitialize();
    }
    devices
}

/// Native Windows Media Foundation hardware webcam capture (`IMFSourceReader`).
pub struct MfCameraCapture {
    reader: Option<IMFSourceReader>,
    actual_config: CameraCaptureConfig,
    frame_counter: u64,
    mf_started: bool,
    is_open: bool,
}

unsafe impl Send for MfCameraCapture {}

impl Default for MfCameraCapture {
    fn default() -> Self {
        Self {
            reader: None,
            actual_config: CameraCaptureConfig::default(),
            frame_counter: 0,
            mf_started: false,
            is_open: false,
        }
    }
}

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
}

impl CameraSource for MfCameraCapture {
    fn open(&mut self, config: &CameraCaptureConfig) -> bool {
        self.close();
        self.actual_config = config.clone();

        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_err() && hr != windows::core::HRESULT(-2147417850) { // RPC_E_CHANGED_MODE
                return false;
            }

            if MFStartup(MF_VERSION, 0).is_err() {
                return false;
            }
            self.mf_started = true;

            let mut enum_attr = None;
            if MFCreateAttributes(&mut enum_attr, 1).is_err() {
                self.close();
                return false;
            }
            let enum_attr = match enum_attr {
                Some(a) => a,
                None => {
                    self.close();
                    return false;
                }
            };

            let _ = enum_attr.SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            );

            let mut pp_devices: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            if MFEnumDeviceSources(&enum_attr, &mut pp_devices, &mut count).is_err()
                || pp_devices.is_null()
                || count == 0
            {
                self.close();
                return false;
            }

            let activates = std::slice::from_raw_parts(pp_devices, count as usize);
            let mut selected_idx = 0usize;

            if !config.device_id.is_empty() {
                for (i, act_opt) in activates.iter().enumerate() {
                    if let Some(act) = act_opt {
                        let mut sym_ptr = PWSTR::null();
                        let mut sym_len = 0u32;
                        if act.GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                            &mut sym_ptr,
                            &mut sym_len,
                        ).is_ok() && !sym_ptr.is_null() {
                            let sym = pwstr_to_string(sym_ptr);
                            CoTaskMemFree(Some(sym_ptr.0 as *mut c_void));
                            if sym.contains(&config.device_id) {
                                selected_idx = i;
                                break;
                            }
                        }
                    }
                }
            } else if (config.device_index as usize) < count as usize {
                selected_idx = config.device_index as usize;
            }

            let media_source = if let Some(Some(ref act)) = activates.get(selected_idx) {
                act.ActivateObject::<windows::Win32::Media::MediaFoundation::IMFMediaSource>()
            } else {
                CoTaskMemFree(Some(pp_devices as *mut c_void));
                self.close();
                return false;
            };

            CoTaskMemFree(Some(pp_devices as *mut c_void));

            let media_source = match media_source {
                Ok(ms) => ms,
                Err(_) => {
                    self.close();
                    return false;
                }
            };

            // Enable video processing in source reader
            let mut reader_attr = None;
            let _ = MFCreateAttributes(&mut reader_attr, 2);
            if let Some(ref r_attr) = reader_attr {
                let _ = r_attr.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);
            }

            let reader = match MFCreateSourceReaderFromMediaSource(&media_source, reader_attr.as_ref()) {
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
                let _ = mf_set_attribute_size(&out_t, &MF_MT_FRAME_SIZE, config.width, config.height);
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
                        if reader.SetCurrentMediaType(
                            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                            None,
                            &fb_t,
                        ).is_err() {
                            self.close();
                            return false;
                        }
                    }
                }
            }

            // Query negotiated resolution & framerate
            if let Ok(current_type) = reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32) {
                let mut w = 0u32;
                let mut h = 0u32;
                if mf_get_attribute_size(&current_type, &MF_MT_FRAME_SIZE, &mut w, &mut h).is_ok() {
                    self.actual_config.width = w;
                    self.actual_config.height = h;
                }
                let mut fps_n = 0u32;
                let mut fps_d = 1u32;
                if mf_get_attribute_ratio(&current_type, &MF_MT_FRAME_RATE, &mut fps_n, &mut fps_d).is_ok() && fps_d > 0 {
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
        let reader = match self.reader.as_ref() {
            Some(r) => r,
            None => return None,
        };

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

            let success = if self.actual_config.bit_depth == 64 {
                dst_bgra.resize(expected_bytes_64, 0);
                if (cur_len as usize) >= expected_bytes_64 {
                    std::ptr::copy_nonoverlapping(raw_ptr, dst_bgra.as_mut_ptr(), expected_bytes_64);
                    true
                } else if (cur_len as usize) >= expected_bytes_32 {
                    // Convert 8-bit to 16-bit BGRA: each channel val * 257 (0..255 -> 0..65535)
                    let src_slice = std::slice::from_raw_parts(raw_ptr, expected_bytes_32);
                    let dst_u16 = std::slice::from_raw_parts_mut(dst_bgra.as_mut_ptr() as *mut u16, (w * h * 4) as usize);
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
