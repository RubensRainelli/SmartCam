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

use std::sync::atomic::{AtomicI32, Ordering};
use windows::{
    core::{implement, IUnknown, Interface, Result, GUID},
    Win32::{
        Foundation::{BOOL, CLASS_E_NOAGGREGATION, E_POINTER},
        Media::MediaFoundation::IMFMediaSourceEx,
        System::Com::{IClassFactory, IClassFactory_Impl},
    },
};

use crate::{constants::trace_mf, source::SmartCamMfMediaSource};

pub static SERVER_LOCKS: AtomicI32 = AtomicI32::new(0);

/// Live COM objects of this DLL (class factories, media sources, media streams).
pub static LIVE_OBJECTS: AtomicI32 = AtomicI32::new(0);

/// Held by every COM object of this DLL so that `DllCanUnloadNow` keeps the DLL loaded while
/// any of them is alive (windows-rs `#[implement]` objects do not count module references).
pub struct ModuleRef(());

impl ModuleRef {
    pub fn new() -> Self {
        LIVE_OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self(())
    }
}

impl Default for ModuleRef {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ModuleRef {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[implement(IClassFactory)]
pub struct SmartCamClassFactory {
    _module: ModuleRef,
}

impl Default for SmartCamClassFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartCamClassFactory {
    pub fn new() -> Self {
        Self {
            _module: ModuleRef::new(),
        }
    }
}

impl IClassFactory_Impl for SmartCamClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        if punkouter.is_some() {
            return Err(windows::core::Error::from_hresult(CLASS_E_NOAGGREGATION));
        }
        if ppvobject.is_null() || riid.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }

        let requested_iid = unsafe { *riid };
        trace_mf(&format!("CreateInstance requested riid={:?}", requested_iid));
        let media_source: IMFMediaSourceEx = SmartCamMfMediaSource::create_com()?;
        let unk: IUnknown = media_source.into();
        let hr = unsafe { unk.query(riid, ppvobject) };
        trace_mf(&format!("CreateInstance query riid={:?} -> hr={:?}", requested_iid, hr));
        hr.ok()
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        if flock.as_bool() {
            SERVER_LOCKS.fetch_add(1, Ordering::SeqCst);
        } else {
            SERVER_LOCKS.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}
