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

use windows::{
    core::{w, GUID, HRESULT, PCWSTR},
    Win32::{
        Foundation::{FreeLibrary, HMODULE},
        Media::MediaFoundation::{IMFVirtualCamera, MFShutdown, MFStartup, MF_VERSION},
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            LibraryLoader::{GetProcAddress, LoadLibraryW},
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
    },
};

use crate::{
    header::{DEFAULT_SHARED_EVENT_NAME, SMARTCAM_MF_CLSID_STR},
    ipc::SharedFrameIpc,
    types::{VirtualCameraSink, VirtualCameraSinkConfig},
};

// KSCATEGORY_VIDEO_CAMERA {E5323777-F976-4F5B-9B55-B94699C46E44}
const KSCATEGORY_VIDEO_CAMERA: GUID = GUID::from_u128(0xe5323777_f976_4f5b_9b55_b94699c46e44);

type PfnMfCreateVirtualCamera = unsafe extern "system" fn(
    r#type: u32,
    lifetime: u32,
    access: u32,
    friendly_name: PCWSTR,
    source_id: PCWSTR,
    categories: *const GUID,
    category_count: u32,
    virtual_camera: *mut Option<IMFVirtualCamera>,
) -> HRESULT;

/// Media Foundation Virtual Camera Sink (`MfVirtualCameraSink`).
/// Manages high-performance shared-memory frame transfer to the Media Foundation Virtual Camera
/// source driver, registering dynamic session virtual camera on Windows 11 21H2+ via `mfsensorgroup.dll`.
#[derive(Default)]
pub struct MfVirtualCameraSink {
    ipc: Option<SharedFrameIpc>,
    vcam_handle: Option<IMFVirtualCamera>,
    vcam_dll: Option<HMODULE>,
    frame_count: u64,
    mf_started: bool,
    config: VirtualCameraSinkConfig,
}

unsafe impl Send for MfVirtualCameraSink {}

impl MfVirtualCameraSink {
    pub fn new() -> Self {
        Self::default()
    }

    fn sync_staged_mf_source_dll() {
        let public_dir = std::path::PathBuf::from(r"C:\Users\Public\SmartCam");
        let public_dll = public_dir.join("smartcam_mf_source.dll");
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidates = [
                    dir.join("smartcam_mf_source.dll"),
                    dir.parent()
                        .map(|p| p.join("smartcam_mf_source.dll"))
                        .unwrap_or_default(),
                ];
                for src in candidates.iter().filter(|p| p.exists() && *p != &public_dll) {
                    let _ = std::fs::create_dir_all(&public_dir);
                    if std::fs::copy(src, &public_dll).is_err() {
                        let old_dll = public_dir.join("smartcam_mf_source.dll.old");
                        let _ = std::fs::remove_file(&old_dll);
                        if std::fs::rename(&public_dll, &old_dll).is_ok() {
                            let _ = std::fs::copy(src, &public_dll);
                        }
                    }
                    break;
                }
            }
        }
    }
}

impl VirtualCameraSink for MfVirtualCameraSink {
    fn initialize(&mut self, config: &VirtualCameraSinkConfig) -> bool {
        self.shutdown();
        self.config = config.clone();
        Self::sync_staged_mf_source_dll();

        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_err() && hr != windows::core::HRESULT(-2147417850) {
                // RPC_E_CHANGED_MODE
                return false;
            }

            if MFStartup(MF_VERSION, 0).is_ok() {
                self.mf_started = true;
            }

            // 1. Initialize User-Mode Named Shared Memory IPC for frame transport
            let ipc = SharedFrameIpc::open_or_create(
                &self.config.shared_memory_name,
                DEFAULT_SHARED_EVENT_NAME,
                true,
            );
            let ipc = match ipc {
                Some(i) => i,
                None => {
                    self.shutdown();
                    return false;
                }
            };
            self.ipc = Some(ipc);

            // 2. Bind Windows 11 Native MFCreateVirtualCamera API (resides in mfsensorgroup.dll)
            let mut dll = LoadLibraryW(w!("mfsensorgroup.dll")).ok();
            if dll.is_none() {
                dll = LoadLibraryW(w!("mfvirtualcamera.dll")).ok();
            }
            if dll.is_none() {
                dll = LoadLibraryW(w!("mfplat.dll")).ok();
            }

            if let Some(h_dll) = dll {
                self.vcam_dll = Some(h_dll);
                let proc_name = std::ffi::CString::new("MFCreateVirtualCamera").unwrap();
                let proc =
                    GetProcAddress(h_dll, windows::core::PCSTR(proc_name.as_ptr() as *const u8));

                if let Some(pfn_ptr) = proc {
                    let pfn: PfnMfCreateVirtualCamera = std::mem::transmute(pfn_ptr);
                    // Always enforce single canonical name "SmartCam"
                    let wide_name: Vec<u16> = "SmartCam"
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    let wide_clsid: Vec<u16> = SMARTCAM_MF_CLSID_STR
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();

                    let mut vcam: Option<IMFVirtualCamera> = None;
                    let create_hr = pfn(
                        0, // SoftwareCameraSource
                        0, // Session lifetime
                        0, // CurrentUser access
                        PCWSTR(wide_name.as_ptr()),
                        PCWSTR(wide_clsid.as_ptr()),
                        &KSCATEGORY_VIDEO_CAMERA,
                        1,
                        &mut vcam,
                    );

                    if create_hr.is_ok() {
                        if let Some(ref v) = vcam {
                            let start_hr = v.Start(None);
                            if start_hr.is_err() {
                                eprintln!("[SmartCam][Sink] IMFVirtualCamera::Start failed: {:?}", start_hr);
                            }
                        }
                        self.vcam_handle = vcam;
                    } else {
                        eprintln!("[SmartCam][Sink] MFCreateVirtualCamera failed: {:?}", create_hr);
                    }
                }
            }

            true
        }
    }

    fn push_frame(&mut self, bgra: &[u8], width: u32, height: u32, fps: u32) -> bool {
        let ipc = match self.ipc.as_ref() {
            Some(i) => i,
            None => return false,
        };

        let mut qpc = 0i64;
        let mut freq = 0i64;
        let (ts_100ns, ok_qpc) = unsafe {
            let q1 = QueryPerformanceCounter(&mut qpc);
            let q2 = QueryPerformanceFrequency(&mut freq);
            if q1.is_ok() && q2.is_ok() && freq > 0 {
                ((qpc as u64 * 10_000_000) / (freq as u64), true)
            } else {
                (0, false)
            }
        };

        let timestamp_100ns = if ok_qpc && ts_100ns > 0 {
            ts_100ns
        } else {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.as_nanos() / 100) as u64)
                .unwrap_or(0)
        };

        self.frame_count += 1;
        ipc.write_frame(
            bgra,
            width,
            height,
            width * 4,
            fps,
            1,
            timestamp_100ns,
            self.frame_count,
        )
    }

    fn is_consumer_active(&self) -> bool {
        self.ipc
            .as_ref()
            .map(|i| i.is_consumer_active(2000))
            .unwrap_or(false)
    }

    fn shutdown(&mut self) {
        if let Some(vcam) = self.vcam_handle.take() {
            unsafe {
                let _ = vcam.Stop();
                let _ = vcam.Remove();
                let _ = vcam.Shutdown();
            }
        }

        if let Some(dll) = self.vcam_dll.take() {
            unsafe {
                let _ = FreeLibrary(dll);
            }
        }

        if let Some(mut ipc) = self.ipc.take() {
            ipc.close();
        }

        if self.mf_started {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
            self.mf_started = false;
        }

        self.frame_count = 0;
    }
}

impl Drop for MfVirtualCameraSink {
    fn drop(&mut self) {
        self.shutdown();
    }
}
