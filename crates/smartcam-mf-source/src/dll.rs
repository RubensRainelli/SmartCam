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
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::{
    core::{Interface, GUID, HRESULT, PCWSTR},
    Win32::{
        Foundation::{
            BOOL, CLASS_E_CLASSNOTAVAILABLE, E_ACCESSDENIED, E_POINTER, HMODULE, MAX_PATH, S_FALSE,
            S_OK,
        },
        System::{
            Com::IClassFactory,
            LibraryLoader::{DisableThreadLibraryCalls, GetModuleFileNameW},
            Registry::{
                RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegSetValueExW, HKEY,
                HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, KEY_WRITE,
                REG_OPTION_NON_VOLATILE, REG_SZ,
            },
        },
    },
};

use crate::{
    constants::CLSID_SMARTCAM_MF_SOURCE,
    factory::{SmartCamClassFactory, SERVER_LOCKS},
};

static MODULE_HANDLE: AtomicUsize = AtomicUsize::new(0);

const FRIENDLY_NAME: &str = "SmartCam Virtual Camera";
const CLSID_STR: &str = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}";

unsafe fn register_in_root_hive(root: HKEY, module_path: &[u16]) -> bool {
    let clsid_key = format!("SOFTWARE\\Classes\\CLSID\\{}", CLSID_STR);
    let inproc_key = format!("SOFTWARE\\Classes\\CLSID\\{}\\InprocServer32", CLSID_STR);

    let wide_clsid: Vec<u16> = clsid_key.encode_utf16().chain(std::iter::once(0)).collect();
    let wide_inproc: Vec<u16> = inproc_key
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let wide_desc: Vec<u16> = FRIENDLY_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let wide_threading: Vec<u16> = "Both".encode_utf16().chain(std::iter::once(0)).collect();
    let wide_tm_val: Vec<u16> = "ThreadingModel"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut hkey = HKEY::default();
    let status = RegCreateKeyExW(
        root,
        PCWSTR(wide_clsid.as_ptr()),
        0,
        None,
        REG_OPTION_NON_VOLATILE,
        KEY_WRITE | KEY_WOW64_64KEY,
        None,
        &mut hkey,
        None,
    );
    if status.is_err() {
        return false;
    }

    let _ = RegSetValueExW(
        hkey,
        PCWSTR::null(),
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(
            wide_desc.as_ptr() as *const u8,
            wide_desc.len() * 2,
        )),
    );
    let _ = RegCloseKey(hkey);

    let mut hinproc = HKEY::default();
    let status = RegCreateKeyExW(
        root,
        PCWSTR(wide_inproc.as_ptr()),
        0,
        None,
        REG_OPTION_NON_VOLATILE,
        KEY_WRITE | KEY_WOW64_64KEY,
        None,
        &mut hinproc,
        None,
    );
    if status.is_err() {
        return false;
    }

    let _ = RegSetValueExW(
        hinproc,
        PCWSTR::null(),
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(
            module_path.as_ptr() as *const u8,
            module_path.len() * 2,
        )),
    );

    let _ = RegSetValueExW(
        hinproc,
        PCWSTR(wide_tm_val.as_ptr()),
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(
            wide_threading.as_ptr() as *const u8,
            wide_threading.len() * 2,
        )),
    );

    let _ = RegCloseKey(hinproc);
    true
}

unsafe fn unregister_from_root_hive(root: HKEY) {
    let inproc_key = format!("SOFTWARE\\Classes\\CLSID\\{}\\InprocServer32", CLSID_STR);
    let clsid_key = format!("SOFTWARE\\Classes\\CLSID\\{}", CLSID_STR);

    let wide_inproc: Vec<u16> = inproc_key
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let wide_clsid: Vec<u16> = clsid_key.encode_utf16().chain(std::iter::once(0)).collect();

    let _ = RegDeleteKeyW(root, PCWSTR(wide_inproc.as_ptr()));
    let _ = RegDeleteKeyW(root, PCWSTR(wide_clsid.as_ptr()));
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    hmodule: HMODULE,
    reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        MODULE_HANDLE.store(hmodule.0 as usize, Ordering::SeqCst);
        let _ = DisableThreadLibraryCalls(hmodule);
    }
    true.into()
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() || rclsid.is_null() || riid.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();

    if *rclsid != CLSID_SMARTCAM_MF_SOURCE {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    let factory: IClassFactory = SmartCamClassFactory::new().into();
    factory.query(riid, ppv)
}

#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    if SERVER_LOCKS.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    let mut module_path = [0u16; MAX_PATH as usize];
    let hmodule = HMODULE(MODULE_HANDLE.load(Ordering::SeqCst) as *mut c_void);
    let len = GetModuleFileNameW(hmodule, &mut module_path);
    if len == 0 {
        return HRESULT::from_win32(windows::Win32::Foundation::GetLastError().0);
    }

    let ok_hkcu = register_in_root_hive(HKEY_CURRENT_USER, &module_path[..=len as usize]);
    let ok_hklm = register_in_root_hive(HKEY_LOCAL_MACHINE, &module_path[..=len as usize]);

    if ok_hkcu || ok_hklm {
        S_OK
    } else {
        E_ACCESSDENIED
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    unregister_from_root_hive(HKEY_CURRENT_USER);
    unregister_from_root_hive(HKEY_LOCAL_MACHINE);
    S_OK
}
