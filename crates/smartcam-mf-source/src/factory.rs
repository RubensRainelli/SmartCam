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

use crate::source::SmartCamMfMediaSource;

pub static SERVER_LOCKS: AtomicI32 = AtomicI32::new(0);

#[implement(IClassFactory)]
pub struct SmartCamClassFactory;

impl Default for SmartCamClassFactory {
    fn default() -> Self {
        Self
    }
}

impl SmartCamClassFactory {
    pub fn new() -> Self {
        Self
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

        let media_source: IMFMediaSourceEx = SmartCamMfMediaSource::new()?.into();
        let unk: IUnknown = media_source.into();
        unsafe { unk.query(riid, ppvobject).ok() }
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
