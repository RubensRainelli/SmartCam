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
        let _ = std::fs::remove_dir_all(r"C:\Users\Public\SmartCam");
        let prog_data =
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        let staged_dir = std::path::PathBuf::from(prog_data).join("SmartCam");
        let staged_dll = staged_dir.join("smartcam_mf_source.dll");
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let release_candidate = dir
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.join("release").join("smartcam_mf_source.dll"))
                    .unwrap_or_default();
                let candidates = [
                    dir.join("smartcam_mf_source.dll"),
                    release_candidate,
                    dir.parent()
                        .map(|p| p.join("smartcam_mf_source.dll"))
                        .unwrap_or_default(),
                ];
                if let Some(src) = candidates.iter().find(|p| p.is_file() && *p != &staged_dll) {
                    let _ = std::fs::create_dir_all(&staged_dir);
                    let already_identical = match (std::fs::read(src), std::fs::read(&staged_dll)) {
                        (Ok(a), Ok(b)) => a == b,
                        _ => false,
                    };
                    if !already_identical {
                        let old_dll = staged_dir.join("smartcam_mf_source.dll.old");
                        let _ = std::fs::remove_file(&old_dll);
                        if staged_dll.exists() {
                            let _ = std::fs::rename(&staged_dll, &old_dll);
                        }
                        let _ = std::fs::copy(src, &staged_dll);
                        let _ = std::fs::remove_file(&old_dll);
                    }
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
                    // Try System lifetime (1 = MFVirtualCameraLifetime_System) first so the virtual
                    // camera remains persistently enumerable in Chrome/Edge/Teams even when SmartCam
                    // is closed or in the background; fall back to Session lifetime (0) if needed.
                    let mut create_hr = pfn(
                        0, // SoftwareCameraSource
                        1, // System lifetime (persistent across sessions)
                        0, // CurrentUser access
                        PCWSTR(wide_name.as_ptr()),
                        PCWSTR(wide_clsid.as_ptr()),
                        &KSCATEGORY_VIDEO_CAMERA,
                        1,
                        &mut vcam,
                    );
                    if create_hr.is_err() {
                        create_hr = pfn(
                            0, // SoftwareCameraSource
                            0, // Session lifetime fallback
                            0, // CurrentUser access
                            PCWSTR(wide_name.as_ptr()),
                            PCWSTR(wide_clsid.as_ptr()),
                            &KSCATEGORY_VIDEO_CAMERA,
                            1,
                            &mut vcam,
                        );
                    }

                    if create_hr.is_ok() {
                        if let Some(ref v) = vcam {
                            let start_hr = v.Start(None);
                            if start_hr.is_err() {
                                eprintln!(
                                    "[SmartCam][Sink] IMFVirtualCamera::Start failed: {:?}",
                                    start_hr
                                );
                            }
                        }
                        self.vcam_handle = vcam;
                    } else {
                        eprintln!(
                            "[SmartCam][Sink] MFCreateVirtualCamera failed: {:?}",
                            create_hr
                        );
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

        static QPC_FREQ: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
        let freq = *QPC_FREQ.get_or_init(|| {
            let mut f = 0i64;
            unsafe {
                if QueryPerformanceFrequency(&mut f).is_ok() && f > 0 {
                    f
                } else {
                    0
                }
            }
        });

        let mut qpc = 0i64;
        let (ts_100ns, ok_qpc) = unsafe {
            if freq > 0 && QueryPerformanceCounter(&mut qpc).is_ok() {
                (((qpc as u128 * 10_000_000) / (freq as u128)) as u64, true)
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
        // Drop the COM reference without calling Stop() or Remove() so that a
        // System-lifetime virtual camera stays registered and enumerable in browsers
        // (Chrome, Edge, Meet, Teams, Zoom) even when SmartCam is closed.
        let _ = self.vcam_handle.take();

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

/// Ensures the Windows 11 Media Foundation Virtual Camera ("SmartCam") is registered and started
/// with System lifetime (`MFVirtualCameraLifetime_System`) so that Chromium browsers (Google Chrome,
/// Edge, Brave) and video apps (Teams, Meet, Zoom, OBS) always enumerate `"SmartCam"` even when the
/// `SmartCam.exe` GUI is closed or minimized.
pub fn ensure_system_virtual_camera_started() -> bool {
    MfVirtualCameraSink::sync_staged_mf_source_dll();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mf_ok = MFStartup(MF_VERSION, 0).is_ok();

        let mut dll = LoadLibraryW(w!("mfsensorgroup.dll")).ok();
        if dll.is_none() {
            dll = LoadLibraryW(w!("mfvirtualcamera.dll")).ok();
        }
        if dll.is_none() {
            dll = LoadLibraryW(w!("mfplat.dll")).ok();
        }

        let mut ok = false;
        if let Some(h_dll) = dll {
            let proc_name = std::ffi::CString::new("MFCreateVirtualCamera").unwrap();
            let proc =
                GetProcAddress(h_dll, windows::core::PCSTR(proc_name.as_ptr() as *const u8));
            if let Some(pfn_ptr) = proc {
                let pfn: PfnMfCreateVirtualCamera = std::mem::transmute(pfn_ptr);
                let wide_name: Vec<u16> = "SmartCam"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let wide_clsid: Vec<u16> = SMARTCAM_MF_CLSID_STR
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();

                let mut vcam: Option<IMFVirtualCamera> = None;
                let mut hr = pfn(
                    0, // SoftwareCameraSource
                    1, // MFVirtualCameraLifetime_System
                    0, // CurrentUser access
                    PCWSTR(wide_name.as_ptr()),
                    PCWSTR(wide_clsid.as_ptr()),
                    &KSCATEGORY_VIDEO_CAMERA,
                    1,
                    &mut vcam,
                );
                if hr.is_err() {
                    hr = pfn(
                        0, // SoftwareCameraSource
                        0, // MFVirtualCameraLifetime_Session fallback
                        0, // CurrentUser access
                        PCWSTR(wide_name.as_ptr()),
                        PCWSTR(wide_clsid.as_ptr()),
                        &KSCATEGORY_VIDEO_CAMERA,
                        1,
                        &mut vcam,
                    );
                }
                if hr.is_ok() {
                    if let Some(ref v) = vcam {
                        ok = v.Start(None).is_ok();
                    }
                    // Intentionally drop `vcam` without Stop() or Remove() so System lifetime persists
                    drop(vcam);
                }
            }
            let _ = FreeLibrary(h_dll);
        }

        if mf_ok {
            let _ = MFShutdown();
        }
        ok
    }
}

/// Stops and removes the System-lifetime Windows 11 Media Foundation Virtual Camera ("SmartCam").
/// Called only during driver uninstallation (`--unregister-driver`).
pub fn remove_system_virtual_camera() -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mf_ok = MFStartup(MF_VERSION, 0).is_ok();

        let mut dll = LoadLibraryW(w!("mfsensorgroup.dll")).ok();
        if dll.is_none() {
            dll = LoadLibraryW(w!("mfvirtualcamera.dll")).ok();
        }
        if dll.is_none() {
            dll = LoadLibraryW(w!("mfplat.dll")).ok();
        }

        let mut ok = false;
        if let Some(h_dll) = dll {
            let proc_name = std::ffi::CString::new("MFCreateVirtualCamera").unwrap();
            let proc =
                GetProcAddress(h_dll, windows::core::PCSTR(proc_name.as_ptr() as *const u8));
            if let Some(pfn_ptr) = proc {
                let pfn: PfnMfCreateVirtualCamera = std::mem::transmute(pfn_ptr);
                let wide_name: Vec<u16> = "SmartCam"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let wide_clsid: Vec<u16> = SMARTCAM_MF_CLSID_STR
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();

                for lifetime in [1u32, 0u32] {
                    let mut vcam: Option<IMFVirtualCamera> = None;
                    let hr = pfn(
                        0,
                        lifetime,
                        0,
                        PCWSTR(wide_name.as_ptr()),
                        PCWSTR(wide_clsid.as_ptr()),
                        &KSCATEGORY_VIDEO_CAMERA,
                        1,
                        &mut vcam,
                    );
                    if hr.is_ok() {
                        if let Some(ref v) = vcam {
                            let _ = v.Stop();
                            let _ = v.Remove();
                            let _ = v.Shutdown();
                            ok = true;
                        }
                    }
                }
            }
            let _ = FreeLibrary(h_dll);
        }

        if mf_ok {
            let _ = MFShutdown();
        }
        ok
    }
}
