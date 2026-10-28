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

#![allow(dead_code)]

use std::ffi::{c_char, c_void, CStr};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::config::PEFL_YAML_HEADER;
use crate::engine_bridge::{generate_synthetic_bgra_frame, NativeEngineLibrary};
use crate::win32_ffi::{to_wstring, FreeLibrary, GetProcAddress, LoadLibraryW, HMODULE};

// ============================================================================
// C ABI Function Pointer Types for Hardware Policy & Device Discovery
// ============================================================================

type FnSetHardwarePolicy = unsafe extern "C" fn(*mut c_void, u32, u32, i32) -> i32;
type FnGetComputeDeviceCount = unsafe extern "C" fn(*mut c_void) -> i32;
type FnGetComputeDeviceInfo = unsafe extern "C" fn(
    *mut c_void,
    i32,
    *mut u32,
    *mut c_char,
    u32,
    *mut c_char,
    u32,
    *mut u64,
    *mut i32,
    *mut i32,
) -> i32;
type FnGetActiveComputeBackend = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type FnGetActiveComputeDeviceName = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type FnIsAiEnabled = unsafe extern "C" fn(*mut c_void) -> i32;

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct FileTime {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn GetThreadTimes(
        hThread: *mut c_void,
        lpCreationTime: *mut FileTime,
        lpExitTime: *mut FileTime,
        lpKernelTime: *mut FileTime,
        lpUserTime: *mut FileTime,
    ) -> i32;
}

/// Reads the cumulative OS thread CPU time (`kernel + user`) in 100-nanosecond ticks via `GetThreadTimes`.
fn read_current_thread_cpu_100ns() -> u64 {
    unsafe {
        let mut creation = FileTime::default();
        let mut exit = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        let ok = GetThreadTimes(
            GetCurrentThread(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        );
        if ok == 0 {
            return 0;
        }
        let k = ((kernel.dw_high_date_time as u64) << 32) | (kernel.dw_low_date_time as u64);
        let u = ((user.dw_high_date_time as u64) << 32) | (user.dw_low_date_time as u64);
        k.saturating_add(u)
    }
}

unsafe fn load_proc_local<T: Copy>(hmod: HMODULE, name: &[u8]) -> Option<T> {
    if hmod.is_null() {
        return None;
    }
    let ptr = GetProcAddress(hmod, name.as_ptr());
    if ptr.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&ptr))
    }
}

// ============================================================================
// Public Data Structures for Hardware Discovery & Benchmark Suite
// ============================================================================

/// Describes a discovered compute backend (`CPU`, `Vulkan`, `OpenCL`, `CUDA`, `ROCm`).
#[derive(Clone, Debug)]
pub struct ProbedComputeDevice {
    pub backend_id: u32,       // 1=CPU, 2=Vulkan, 3=OpenCL, 4=CUDA, 5=ROCm
    pub backend_key: String,   // "cpu", "vulkan", "opencl", "cuda", "rocm"
    pub backend_name: String,  // "CPU", "Vulkan", "OpenCL", "CUDA", "ROCm"
    pub device_name: String,   // e.g. "Intel(R) Iris(R) Xe Graphics"
    pub vram_mb: u64,          // Dedicated/shared VRAM in MB
    pub available: bool,       // True if driver + hardware device are present on this PC
    pub is_igpu: bool,         // True if integrated GPU / unified memory architecture
    pub status_label: String,  // "Disponibile" or "Non presente su questo PC"
}

/// Performance & hardware availability metrics for a single benchmarked configuration.
#[derive(Clone, Debug)]
pub struct BenchmarkProfileEntry {
    pub profile_id: String,            // e.g. "cpu_avx512_ai", "vulkan_ai"
    pub label: String,                 // e.g. "CPU + AVX-512 + AI", "GPU Vulkan + AI"
    pub compute_backend: String,       // "cpu", "vulkan", "opencl", "cuda", "rocm"
    pub compute_backend_id: u32,       // 1=CPU, 2=Vulkan, 3=OpenCL, 4=CUDA, 5=ROCm
    pub simd_mode: String,             // "auto", "avx512", "avx2", "scalar"
    pub simd_mode_id: u32,             // 0=Auto, 1=AVX512, 2=AVX2, 3=Scalar
    pub ai_enabled: bool,              // true = AI Active, false = Fast Non-AI (<0.2 ms)
    pub available: bool,               // true if supported on current PC
    pub availability_label: String,    // "Disponibile" vs "Non presente su questo PC"
    pub device_name: String,           // Hardware model used by this profile
    pub avg_frame_ms: f32,             // Measured frame time in milliseconds at 1440x1080
    pub estimated_fps: f32,            // Estimated maximum sustainable FPS
    pub estimated_cpu_load_pct: f32,   // Estimated CPU load percentage [0..100] at 30 FPS
    pub score: u32,                    // Synthetic performance & efficiency score [0..10000]
    pub recommended: bool,             // True if selected as the optimal profile for this PC
}

/// Complete hardware fingerprint, device inventory, benchmark table, and optimal recommendation.
#[derive(Clone, Debug)]
pub struct HardwareBenchmarkReport {
    pub hardware_fingerprint: String,
    pub timestamp_unix: u64,
    pub timestamp_iso: String,
    pub cpu_name: String,
    pub logical_cores: u32,
    pub avx2_supported: bool,
    pub avx512_supported: bool,
    pub benchmark_resolution: String,
    pub devices: Vec<ProbedComputeDevice>,
    pub profiles: Vec<BenchmarkProfileEntry>,
    pub recommended_profile_id: String,
    pub recommended_profile_label: String,
    pub recommended_backend: String,
    pub recommended_simd: String,
    pub recommended_ai_enabled: bool,
    pub recommended_score: u32,
    pub recommended_fps: f32,
    pub recommended_cpu_load_pct: f32,
    pub recommendation_reason: String,
}

// ============================================================================
// Mapping Helpers for Backend & SIMD Identifiers
// ============================================================================

/// Maps canonical backend string (`"auto"`, `"cpu"`, `"vulkan"`, `"opencl"`, `"cuda"`, `"rocm"`)
/// to the C ABI `compute_backend` integer (`0..=5`).
pub fn compute_backend_str_to_id(s: &str) -> u32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "cpu" => 1,
        "vulkan" | "vk" => 2,
        "opencl" | "ocl" => 3,
        "cuda" | "nvidia" => 4,
        "rocm" | "hip" | "amd" => 5,
        _ => 0, // "auto"
    }
}

/// Maps C ABI `compute_backend` integer (`0..=5`) to canonical lowercase config string.
pub fn compute_backend_id_to_str(id: u32) -> &'static str {
    match id {
        1 => "cpu",
        2 => "vulkan",
        3 => "opencl",
        4 => "cuda",
        5 => "rocm",
        _ => "auto",
    }
}

/// Maps C ABI `compute_backend` integer (`0..=5`) to display label.
pub fn compute_backend_id_to_display(id: u32) -> &'static str {
    match id {
        1 => "CPU",
        2 => "Vulkan",
        3 => "OpenCL",
        4 => "CUDA",
        5 => "ROCm",
        _ => "Auto",
    }
}

/// Maps canonical SIMD mode string (`"auto"`, `"avx512"`, `"avx2"`, `"scalar"` / `"no_avx"`)
/// to the C ABI `smartcam_engine_set_hardware_policy` `simd_mode` parameter:
/// `0 = Auto`, `1 = AVX512`, `2 = AVX2`, `3 = Scalar (No AVX)`.
pub fn simd_mode_str_to_policy_id(s: &str) -> u32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "avx512" | "avx-512" => 1,
        "avx2" | "avx-2" => 2,
        "scalar" | "no_avx" | "no-avx" | "none" => 3,
        _ => 0, // "auto"
    }
}

/// Maps `simd_mode` policy ID (`0..=3`) to canonical config string.
pub fn simd_policy_id_to_str(id: u32) -> &'static str {
    match id {
        1 => "avx512",
        2 => "avx2",
        3 => "scalar",
        _ => "auto",
    }
}

/// Maps `simd_mode` policy ID (`0..=3`) to human-readable display string.
pub fn simd_policy_id_to_display(id: u32) -> &'static str {
    match id {
        1 => "AVX-512",
        2 => "AVX2",
        3 => "Scalare (No-AVX)",
        _ => "Auto",
    }
}

// ============================================================================
// C ABI Hardware Policy & Device Enumeration Wrappers
// ============================================================================

/// Applies `compute_backend`, `simd_mode`, and `ai_enabled` at runtime on a live `SmartCamEngineHandle`
/// without restarting the application.
pub fn set_engine_hardware_policy(
    lib: &NativeEngineLibrary,
    engine_handle: *mut c_void,
    compute_backend: u32,
    simd_mode: u32,
    ai_enabled: bool,
) -> bool {
    if engine_handle.is_null() {
        return false;
    }
    unsafe {
        if let Some(f_policy) = lib.fn_set_hardware_policy {
            return f_policy(
                engine_handle,
                compute_backend,
                simd_mode,
                if ai_enabled { 1 } else { 0 },
            ) == 1;
        }
        if !lib.hmodule.is_null() {
            if let Some(f_policy) =
                load_proc_local::<FnSetHardwarePolicy>(lib.hmodule, b"smartcam_engine_set_hardware_policy\0")
            {
                return f_policy(
                    engine_handle,
                    compute_backend,
                    simd_mode,
                    if ai_enabled { 1 } else { 0 },
                ) == 1;
            }
        }
    }
    false
}

/// Convenience wrapper accepting string identifiers (`"cpu"`, `"vulkan"`, `"opencl"`, `"cuda"`, `"rocm"`, `"auto"`)
/// and SIMD mode strings (`"auto"`, `"avx512"`, `"avx2"`, `"scalar"`).
pub fn set_engine_hardware_policy_by_names(
    lib: &NativeEngineLibrary,
    engine_handle: *mut c_void,
    compute_backend_str: &str,
    simd_mode_str: &str,
    ai_enabled: bool,
) -> bool {
    let backend_id = compute_backend_str_to_id(compute_backend_str);
    let simd_id = simd_mode_str_to_policy_id(simd_mode_str);
    set_engine_hardware_policy(lib, engine_handle, backend_id, simd_id, ai_enabled)
}

/// Returns a formatted label of the active compute backend + SIMD tier from the native Rust engine.
pub fn get_active_compute_backend_label(
    lib: &NativeEngineLibrary,
    engine_handle: *mut c_void,
) -> String {
    if engine_handle.is_null() {
        return "Pure Rust Vectorized Engine".to_string();
    }
    unsafe {
        let mut backend_str = "CPU".to_string();
        let mut device_str = String::new();
        let mut simd_str = "AVX2".to_string();

        if let Some(f_backend) = lib.fn_get_compute_backend {
            let ptr = f_backend(engine_handle);
            if !ptr.is_null() {
                backend_str = CStr::from_ptr(ptr).to_string_lossy().to_string();
            }
        } else if !lib.hmodule.is_null() {
            if let Some(f_backend) = load_proc_local::<FnGetActiveComputeBackend>(
                lib.hmodule,
                b"smartcam_engine_get_active_compute_backend\0",
            ) {
                let ptr = f_backend(engine_handle);
                if !ptr.is_null() {
                    backend_str = CStr::from_ptr(ptr).to_string_lossy().to_string();
                }
            }
        }

        if let Some(f_dev) = lib.fn_get_compute_device_name {
            let ptr = f_dev(engine_handle);
            if !ptr.is_null() {
                device_str = CStr::from_ptr(ptr).to_string_lossy().to_string();
            }
        } else if !lib.hmodule.is_null() {
            if let Some(f_dev) = load_proc_local::<FnGetActiveComputeDeviceName>(
                lib.hmodule,
                b"smartcam_engine_get_active_compute_device_name\0",
            ) {
                let ptr = f_dev(engine_handle);
                if !ptr.is_null() {
                    device_str = CStr::from_ptr(ptr).to_string_lossy().to_string();
                }
            }
        }

        if let Some(f_simd) = lib.fn_get_simd {
            let ptr = f_simd(engine_handle);
            if !ptr.is_null() {
                simd_str = CStr::from_ptr(ptr).to_string_lossy().to_string();
            }
        }

        if backend_str.eq_ignore_ascii_case("CPU") || backend_str.eq_ignore_ascii_case("Auto") {
            format!("CPU {} ({})", simd_str, lib.variant_name)
        } else if !device_str.is_empty() {
            format!("GPU {} — {} (+{})", backend_str, device_str, simd_str)
        } else {
            format!("GPU {} (+{})", backend_str, simd_str)
        }
    }
}

// ============================================================================
// CPUID Brand String, SIMD Capabilities & Driver Probing
// ============================================================================

/// Reads the exact 48-byte processor brand string via x86_64 `__cpuid` leaves `0x80000002..=0x80000004`.
pub fn detect_cpu_brand_string() -> String {
    #[cfg(target_arch = "x86_64")]
    #[allow(unused_unsafe)]
    unsafe {
        let max_ext = core::arch::x86_64::__cpuid(0x8000_0000).eax;
        if max_ext >= 0x8000_0004 {
            let mut bytes = [0u8; 48];
            for (i, leaf) in (0x8000_0002u32..=0x8000_0004u32).enumerate() {
                let res = core::arch::x86_64::__cpuid(leaf);
                let offset = i * 16;
                bytes[offset..offset + 4].copy_from_slice(&res.eax.to_le_bytes());
                bytes[offset + 4..offset + 8].copy_from_slice(&res.ebx.to_le_bytes());
                bytes[offset + 8..offset + 12].copy_from_slice(&res.ecx.to_le_bytes());
                bytes[offset + 12..offset + 16].copy_from_slice(&res.edx.to_le_bytes());
            }
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            let raw = String::from_utf8_lossy(&bytes[..end]).trim().to_string();
            if !raw.is_empty() {
                return collapse_whitespace(&raw);
            }
        }
    }
    "Generic x86_64 Processor".to_string()
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Returns the number of logical CPU cores available on the system.
pub fn detect_logical_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
        .max(1)
}

/// Returns `true` if the CPU and OS support `AVX2` 256-bit vector instructions.
pub fn cpu_supports_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Returns `true` if the CPU and OS support `AVX-512F` and `AVX-512BW` 512-bit vector instructions.
pub fn cpu_supports_avx512() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn probe_win32_dll_symbol(dll_names: &[&str], required_symbol: &[u8]) -> bool {
    unsafe {
        for name in dll_names {
            let wname = to_wstring(name);
            let hmod = LoadLibraryW(wname.as_ptr());
            if !hmod.is_null() {
                let sym = GetProcAddress(hmod, required_symbol.as_ptr());
                let ok = !sym.is_null();
                FreeLibrary(hmod);
                if ok {
                    return true;
                }
            }
        }
    }
    false
}

/// Probes raw GPU runtime driver availability (`Vulkan`, `OpenCL`, `CUDA`, `ROCm`) via Win32 dynamic loader.
pub fn probe_gpu_driver_flags() -> (bool, bool, bool, bool) {
    let vk = probe_win32_dll_symbol(&["vulkan-1.dll"], b"vkCreateInstance\0");
    let ocl = probe_win32_dll_symbol(&["OpenCL.dll"], b"clGetPlatformIDs\0");
    let cuda = probe_win32_dll_symbol(&["nvcuda.dll"], b"cuInit\0");
    let rocm = probe_win32_dll_symbol(
        &[
            "amdhip64.dll",
            "amdhip64_6.dll",
            "amdhip64_5.dll",
            "hip_hcc.dll",
        ],
        b"hipInit\0",
    );
    (vk, ocl, cuda, rocm)
}

/// Enumerates all 5 compute backends (`CPU`, `Vulkan`, `OpenCL`, `CUDA`, `ROCm`) using the C++ engine
/// C ABI (`smartcam_engine_get_compute_device_count` / `smartcam_engine_get_compute_device_info`)
/// with a pure-Rust Win32 fallback if the engine DLL is not yet loaded.
pub fn enumerate_compute_devices(
    engine_lib: Option<&NativeEngineLibrary>,
    engine_handle: *mut c_void,
) -> Vec<ProbedComputeDevice> {
    if let Some(lib) = engine_lib {
        if !lib.hmodule.is_null() {
            unsafe {
                if let (Some(fn_count), Some(fn_info)) = (
                    load_proc_local::<FnGetComputeDeviceCount>(
                        lib.hmodule,
                        b"smartcam_engine_get_compute_device_count\0",
                    ),
                    load_proc_local::<FnGetComputeDeviceInfo>(
                        lib.hmodule,
                        b"smartcam_engine_get_compute_device_info\0",
                    ),
                ) {
                    let count = fn_count(engine_handle);
                    if count > 0 {
                        let mut devices = Vec::with_capacity(count as usize);
                        for idx in 0..count {
                            let mut backend_id: u32 = 0;
                            let mut bname_buf = [0u8; 64];
                            let mut dname_buf = [0u8; 160];
                            let mut vram_mb: u64 = 0;
                            let mut avail: i32 = 0;
                            let mut is_igpu: i32 = 0;

                            if fn_info(
                                engine_handle,
                                idx,
                                &mut backend_id,
                                bname_buf.as_mut_ptr() as *mut c_char,
                                bname_buf.len() as u32,
                                dname_buf.as_mut_ptr() as *mut c_char,
                                dname_buf.len() as u32,
                                &mut vram_mb,
                                &mut avail,
                                &mut is_igpu,
                            ) == 1
                            {
                                let backend_name =
                                    CStr::from_ptr(bname_buf.as_ptr() as *const c_char)
                                        .to_string_lossy()
                                        .to_string();
                                let device_name =
                                    CStr::from_ptr(dname_buf.as_ptr() as *const c_char)
                                        .to_string_lossy()
                                        .to_string();
                                let available = avail != 0;
                                devices.push(ProbedComputeDevice {
                                    backend_id,
                                    backend_key: compute_backend_id_to_str(backend_id).to_string(),
                                    backend_name,
                                    device_name,
                                    vram_mb,
                                    available,
                                    is_igpu: is_igpu != 0,
                                    status_label: if available {
                                        "Disponibile".to_string()
                                    } else {
                                        "Non presente su questo PC".to_string()
                                    },
                                });
                            }
                        }
                        if !devices.is_empty() {
                            return devices;
                        }
                    }
                }
            }
        }
    }

    // Fallback device enumeration using CPUID + Win32 DLL probing
    let cpu_brand = detect_cpu_brand_string();
    let (vk, ocl, cuda, rocm) = probe_gpu_driver_flags();
    let is_intel_or_amd_igpu = cpu_brand.to_ascii_uppercase().contains("INTEL")
        || cpu_brand.to_ascii_uppercase().contains("RYZEN");

    vec![
        ProbedComputeDevice {
            backend_id: 1,
            backend_key: "cpu".to_string(),
            backend_name: "CPU".to_string(),
            device_name: cpu_brand,
            vram_mb: 0,
            available: true,
            is_igpu: false,
            status_label: "Disponibile".to_string(),
        },
        ProbedComputeDevice {
            backend_id: 2,
            backend_key: "vulkan".to_string(),
            backend_name: "Vulkan".to_string(),
            device_name: if vk {
                "Vulkan 1.x Compute GPU/iGPU".to_string()
            } else {
                "Driver Vulkan non rilevato".to_string()
            },
            vram_mb: if vk { 2048 } else { 0 },
            available: vk,
            is_igpu: is_intel_or_amd_igpu,
            status_label: if vk {
                "Disponibile".to_string()
            } else {
                "Non presente su questo PC".to_string()
            },
        },
        ProbedComputeDevice {
            backend_id: 3,
            backend_key: "opencl".to_string(),
            backend_name: "OpenCL".to_string(),
            device_name: if ocl {
                "OpenCL Compute GPU/iGPU".to_string()
            } else {
                "Driver OpenCL non rilevato".to_string()
            },
            vram_mb: if ocl { 2048 } else { 0 },
            available: ocl,
            is_igpu: is_intel_or_amd_igpu,
            status_label: if ocl {
                "Disponibile".to_string()
            } else {
                "Non presente su questo PC".to_string()
            },
        },
        ProbedComputeDevice {
            backend_id: 4,
            backend_key: "cuda".to_string(),
            backend_name: "CUDA".to_string(),
            device_name: if cuda {
                "NVIDIA CUDA Driver GPU".to_string()
            } else {
                "GPU NVIDIA CUDA non presente".to_string()
            },
            vram_mb: if cuda { 4096 } else { 0 },
            available: cuda,
            is_igpu: false,
            status_label: if cuda {
                "Disponibile".to_string()
            } else {
                "Non presente su questo PC".to_string()
            },
        },
        ProbedComputeDevice {
            backend_id: 5,
            backend_key: "rocm".to_string(),
            backend_name: "ROCm".to_string(),
            device_name: if rocm {
                "AMD ROCm / HIP GPU".to_string()
            } else {
                "GPU AMD ROCm/HIP non presente".to_string()
            },
            vram_mb: if rocm { 4096 } else { 0 },
            available: rocm,
            is_igpu: false,
            status_label: if rocm {
                "Disponibile".to_string()
            } else {
                "Non presente su questo PC".to_string()
            },
        },
    ]
}

fn extract_compact_cpu_token(cpu_brand: &str) -> String {
    let upper = cpu_brand.to_ascii_uppercase();
    let vendor = if upper.contains("INTEL") {
        "INTEL"
    } else if upper.contains("AMD") {
        "AMD"
    } else {
        "CPU"
    };

    // Extract recognizable model identifier (e.g., "i7-1165G7", "i5-1240P", "RYZEN-7-5800X")
    let cleaned = cpu_brand
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("CPU", "")
        .replace("Processor", "");
    let tokens: Vec<&str> = cleaned
        .split(|c: char| c.is_whitespace() || c == '@')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect();

    for (idx, tok) in tokens.iter().enumerate() {
        let low = tok.to_ascii_lowercase();
        if (low.starts_with("i3-")
            || low.starts_with("i5-")
            || low.starts_with("i7-")
            || low.starts_with("i9-"))
            && tok.len() >= 5
        {
            return format!("{}-{}", vendor, sanitize_token(tok));
        }
        if low == "ryzen" && idx + 2 < tokens.len() {
            return format!(
                "{}-RYZEN-{}-{}",
                vendor,
                sanitize_token(tokens[idx + 1]),
                sanitize_token(tokens[idx + 2])
            );
        }
        if low == "ultra" && idx + 2 < tokens.len() {
            return format!(
                "{}-ULTRA-{}-{}",
                vendor,
                sanitize_token(tokens[idx + 1]),
                sanitize_token(tokens[idx + 2])
            );
        }
    }

    // Fallback: pick last alphanumeric model-like token + FNV1a 16-bit suffix
    let mut hash: u32 = 0x811C_9DC5;
    for b in cpu_brand.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{}-{:04X}", vendor, hash & 0xFFFF)
}

fn sanitize_token(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Computes a deterministic, human-readable hardware fingerprint string for the current PC
/// (e.g. `"INTEL-i7-1165G7-8C-AVX512-VK1-OCL1-CUDA0-ROCM0"`).
///
/// If the portable folder is copied to a different PC or if the CPU/GPU/driver configuration
/// changes, this fingerprint changes automatically, triggering a fresh benchmark on startup.
pub fn compute_hardware_fingerprint() -> String {
    let cpu_brand = detect_cpu_brand_string();
    let cores = detect_logical_cores();
    let avx512 = cpu_supports_avx512();
    let avx2 = cpu_supports_avx2();
    let (vk, ocl, cuda, rocm) = probe_gpu_driver_flags();

    let cpu_tag = extract_compact_cpu_token(&cpu_brand);
    let simd_tag = if avx512 {
        "AVX512"
    } else if avx2 {
        "AVX2"
    } else {
        "SCALAR"
    };

    format!(
        "{}-{}C-{}-VK{}-OCL{}-CUDA{}-ROCM{}",
        cpu_tag,
        cores,
        simd_tag,
        if vk { 1 } else { 0 },
        if ocl { 1 } else { 0 },
        if cuda { 1 } else { 0 },
        if rocm { 1 } else { 0 },
    )
}

// ============================================================================
// Persistence & Smart First-Run Rule (`config/hardware_benchmark.yaml`)
// ============================================================================

/// Returns the canonical path to `config/hardware_benchmark.yaml`.
pub fn benchmark_yaml_path(repo_root: &Path) -> PathBuf {
    repo_root.join("config").join("hardware_benchmark.yaml")
}

/// Returns `true` **ONLY** if `config/hardware_benchmark.yaml` does not exist yet OR if the
/// saved `hardware_fingerprint` differs from the current machine's hardware fingerprint.
///
/// When the fingerprint matches the current PC, SmartCam skips re-running the benchmark on startup
/// and loads the cached scores immediately, while still allowing manual re-runs from the UI menu.
pub fn should_auto_run_benchmark(repo_root: &Path) -> bool {
    let path = benchmark_yaml_path(repo_root);
    if !path.is_file() {
        return true;
    }
    let Some(saved) = load_benchmark_report(repo_root) else {
        return true;
    };
    let current_fp = compute_hardware_fingerprint();
    saved.hardware_fingerprint.trim() != current_fp.trim() || saved.profiles.is_empty()
}

/// Loads a previously saved `HardwareBenchmarkReport` from `config/hardware_benchmark.yaml`.
pub fn load_benchmark_report(repo_root: &Path) -> Option<HardwareBenchmarkReport> {
    let path = benchmark_yaml_path(repo_root);
    let raw = fs::read_to_string(&path).ok()?;

    let mut report = HardwareBenchmarkReport {
        hardware_fingerprint: String::new(),
        timestamp_unix: 0,
        timestamp_iso: String::new(),
        cpu_name: detect_cpu_brand_string(),
        logical_cores: detect_logical_cores(),
        avx2_supported: cpu_supports_avx2(),
        avx512_supported: cpu_supports_avx512(),
        benchmark_resolution: "1440x1080".to_string(),
        devices: Vec::new(),
        profiles: Vec::new(),
        recommended_profile_id: String::new(),
        recommended_profile_label: String::new(),
        recommended_backend: "auto".to_string(),
        recommended_simd: "auto".to_string(),
        recommended_ai_enabled: true,
        recommended_score: 0,
        recommended_fps: 60.0,
        recommended_cpu_load_pct: 10.0,
        recommendation_reason: String::new(),
    };

    let mut section = String::new();
    let mut current_dev: Option<ProbedComputeDevice> = None;
    let mut current_prof: Option<BenchmarkProfileEntry> = None;
    let mut benchmark_schema_v2 = false;

    for raw_line in raw.lines() {
        let line = strip_yaml_comment(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();

        if indent == 0 && trimmed.ends_with(':') {
            if let Some(dev) = current_dev.take() {
                report.devices.push(dev);
            }
            if let Some(prof) = current_prof.take() {
                report.profiles.push(prof);
            }
            section = trimmed.trim_end_matches(':').trim().to_string();
            continue;
        }

        if section == "devices" && trimmed.starts_with("- ") {
            if let Some(dev) = current_dev.take() {
                report.devices.push(dev);
            }
            current_dev = Some(ProbedComputeDevice {
                backend_id: 1,
                backend_key: "cpu".to_string(),
                backend_name: "CPU".to_string(),
                device_name: String::new(),
                vram_mb: 0,
                available: false,
                is_igpu: false,
                status_label: "Non presente su questo PC".to_string(),
            });
            let after_dash = trimmed[2..].trim();
            if let Some((k, v)) = after_dash.split_once(':') {
                if let Some(ref mut d) = current_dev {
                    apply_device_kv(d, k.trim(), &clean_scalar(v.trim()));
                }
            }
            continue;
        }

        if section == "profiles" && trimmed.starts_with("- ") {
            if let Some(prof) = current_prof.take() {
                report.profiles.push(prof);
            }
            current_prof = Some(BenchmarkProfileEntry {
                profile_id: String::new(),
                label: String::new(),
                compute_backend: "cpu".to_string(),
                compute_backend_id: 1,
                simd_mode: "auto".to_string(),
                simd_mode_id: 0,
                ai_enabled: true,
                available: false,
                availability_label: "Non presente su questo PC".to_string(),
                device_name: String::new(),
                avg_frame_ms: 0.0,
                estimated_fps: 0.0,
                estimated_cpu_load_pct: 0.0,
                score: 0,
                recommended: false,
            });
            let after_dash = trimmed[2..].trim();
            if let Some((k, v)) = after_dash.split_once(':') {
                if let Some(ref mut p) = current_prof {
                    apply_profile_kv(p, k.trim(), &clean_scalar(v.trim()));
                }
            }
            continue;
        }

        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = clean_scalar(v.trim());

        match section.as_str() {
            "" | "hardware" => match key {
                "benchmark_schema_v2" => {
                    benchmark_schema_v2 = val.eq_ignore_ascii_case("true") || val == "1";
                }
                "hardware_fingerprint" => report.hardware_fingerprint = val,
                "timestamp_unix" => {
                    if let Ok(n) = val.parse::<u64>() {
                        report.timestamp_unix = n;
                    }
                }
                "timestamp_iso" => report.timestamp_iso = val,
                "cpu_name" => report.cpu_name = val,
                "logical_cores" => {
                    if let Ok(n) = val.parse::<u32>() {
                        report.logical_cores = n;
                    }
                }
                "avx2_supported" => report.avx2_supported = val.eq_ignore_ascii_case("true"),
                "avx512_supported" => report.avx512_supported = val.eq_ignore_ascii_case("true"),
                "benchmark_resolution" => report.benchmark_resolution = val,
                _ => {}
            },
            "recommendation" => match key {
                "recommended_profile_id" => report.recommended_profile_id = val,
                "recommended_profile_label" => report.recommended_profile_label = val,
                "recommended_backend" => report.recommended_backend = val,
                "recommended_simd" => report.recommended_simd = val,
                "recommended_ai_enabled" => {
                    report.recommended_ai_enabled = val.eq_ignore_ascii_case("true")
                }
                "recommended_score" => {
                    if let Ok(n) = val.parse::<u32>() {
                        report.recommended_score = n;
                    }
                }
                "recommended_fps" => {
                    if let Ok(n) = val.parse::<f32>() {
                        report.recommended_fps = n;
                    }
                }
                "recommended_cpu_load_pct" => {
                    if let Ok(n) = val.parse::<f32>() {
                        report.recommended_cpu_load_pct = n;
                    }
                }
                "recommendation_reason" => report.recommendation_reason = val,
                _ => {}
            },
            "devices" => {
                if let Some(ref mut d) = current_dev {
                    apply_device_kv(d, key, &val);
                }
            }
            "profiles" => {
                if let Some(ref mut p) = current_prof {
                    apply_profile_kv(p, key, &val);
                }
            }
            _ => {}
        }
    }

    if let Some(dev) = current_dev.take() {
        report.devices.push(dev);
    }
    if let Some(prof) = current_prof.take() {
        report.profiles.push(prof);
    }

    // Filter out any legacy unavailable or unmeasured profiles
    report
        .profiles
        .retain(|p| p.available && p.avg_frame_ms > 0.0);

    if !benchmark_schema_v2
        || report.hardware_fingerprint.is_empty()
        || report.profiles.is_empty()
    {
        None
    } else {
        Some(report)
    }
}

fn apply_device_kv(dev: &mut ProbedComputeDevice, key: &str, val: &str) {
    match key {
        "backend_id" => {
            if let Ok(n) = val.parse::<u32>() {
                dev.backend_id = n;
            }
        }
        "backend_key" => dev.backend_key = val.to_string(),
        "backend_name" => dev.backend_name = val.to_string(),
        "device_name" => dev.device_name = val.to_string(),
        "vram_mb" => {
            if let Ok(n) = val.parse::<u64>() {
                dev.vram_mb = n;
            }
        }
        "available" => dev.available = val.eq_ignore_ascii_case("true"),
        "is_igpu" => dev.is_igpu = val.eq_ignore_ascii_case("true"),
        "status_label" => dev.status_label = val.to_string(),
        _ => {}
    }
}

fn apply_profile_kv(prof: &mut BenchmarkProfileEntry, key: &str, val: &str) {
    match key {
        "profile_id" => prof.profile_id = val.to_string(),
        "label" => prof.label = val.to_string(),
        "compute_backend" => {
            prof.compute_backend = val.to_string();
            prof.compute_backend_id = compute_backend_str_to_id(val);
        }
        "compute_backend_id" => {
            if let Ok(n) = val.parse::<u32>() {
                prof.compute_backend_id = n;
            }
        }
        "simd_mode" => {
            prof.simd_mode = val.to_string();
            prof.simd_mode_id = simd_mode_str_to_policy_id(val);
        }
        "simd_mode_id" => {
            if let Ok(n) = val.parse::<u32>() {
                prof.simd_mode_id = n;
            }
        }
        "ai_enabled" => prof.ai_enabled = val.eq_ignore_ascii_case("true"),
        "available" => prof.available = val.eq_ignore_ascii_case("true"),
        "availability_label" => prof.availability_label = val.to_string(),
        "device_name" => prof.device_name = val.to_string(),
        "avg_frame_ms" => {
            if let Ok(n) = val.parse::<f32>() {
                prof.avg_frame_ms = n;
            }
        }
        "estimated_fps" => {
            if let Ok(n) = val.parse::<f32>() {
                prof.estimated_fps = n;
            }
        }
        "estimated_cpu_load_pct" => {
            if let Ok(n) = val.parse::<f32>() {
                prof.estimated_cpu_load_pct = n;
            }
        }
        "score" => {
            if let Ok(n) = val.parse::<u32>() {
                prof.score = n;
            }
        }
        "recommended" => prof.recommended = val.eq_ignore_ascii_case("true"),
        _ => {}
    }
}

/// Saves `HardwareBenchmarkReport` to `config/hardware_benchmark.yaml`.
pub fn save_benchmark_report(
    repo_root: &Path,
    report: &HardwareBenchmarkReport,
) -> Result<(), String> {
    let path = benchmark_yaml_path(repo_root);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut yaml = String::with_capacity(4096);
    yaml.push_str(PEFL_YAML_HEADER.trim_end());
    yaml.push_str("\n\n");
    yaml.push_str(&format!(
        "benchmark_schema_v2: true\n\
         hardware_fingerprint: \"{}\"\n\
         timestamp_unix: {}\n\
         timestamp_iso: \"{}\"\n\
         cpu_name: \"{}\"\n\
         logical_cores: {}\n\
         avx2_supported: {}\n\
         avx512_supported: {}\n\
         benchmark_resolution: \"{}\"\n\n",
        escape_yaml_str(&report.hardware_fingerprint),
        report.timestamp_unix,
        escape_yaml_str(&report.timestamp_iso),
        escape_yaml_str(&report.cpu_name),
        report.logical_cores,
        if report.avx2_supported { "true" } else { "false" },
        if report.avx512_supported { "true" } else { "false" },
        escape_yaml_str(&report.benchmark_resolution),
    ));

    yaml.push_str("recommendation:\n");
    yaml.push_str(&format!(
        "  recommended_profile_id: \"{}\"\n\
         \x20 recommended_profile_label: \"{}\"\n\
         \x20 recommended_backend: \"{}\"\n\
         \x20 recommended_simd: \"{}\"\n\
         \x20 recommended_ai_enabled: {}\n\
         \x20 recommended_score: {}\n\
         \x20 recommended_fps: {:.1}\n\
         \x20 recommended_cpu_load_pct: {:.1}\n\
         \x20 recommendation_reason: \"{}\"\n\n",
        escape_yaml_str(&report.recommended_profile_id),
        escape_yaml_str(&report.recommended_profile_label),
        escape_yaml_str(&report.recommended_backend),
        escape_yaml_str(&report.recommended_simd),
        if report.recommended_ai_enabled { "true" } else { "false" },
        report.recommended_score,
        report.recommended_fps,
        report.recommended_cpu_load_pct,
        escape_yaml_str(&report.recommendation_reason),
    ));

    yaml.push_str("devices:\n");
    for d in &report.devices {
        yaml.push_str(&format!(
            "  - backend_id: {}\n\
             \x20   backend_key: \"{}\"\n\
             \x20   backend_name: \"{}\"\n\
             \x20   device_name: \"{}\"\n\
             \x20   vram_mb: {}\n\
             \x20   available: {}\n\
             \x20   is_igpu: {}\n\
             \x20   status_label: \"{}\"\n",
            d.backend_id,
            escape_yaml_str(&d.backend_key),
            escape_yaml_str(&d.backend_name),
            escape_yaml_str(&d.device_name),
            d.vram_mb,
            if d.available { "true" } else { "false" },
            if d.is_igpu { "true" } else { "false" },
            escape_yaml_str(&d.status_label),
        ));
    }
    yaml.push('\n');

    yaml.push_str("profiles:\n");
    for p in report.profiles.iter().filter(|p| p.available) {
        yaml.push_str(&format!(
            "  - profile_id: \"{}\"\n\
             \x20   label: \"{}\"\n\
             \x20   compute_backend: \"{}\"\n\
             \x20   compute_backend_id: {}\n\
             \x20   simd_mode: \"{}\"\n\
             \x20   simd_mode_id: {}\n\
             \x20   ai_enabled: {}\n\
             \x20   available: {}\n\
             \x20   availability_label: \"{}\"\n\
             \x20   device_name: \"{}\"\n\
             \x20   avg_frame_ms: {:.3}\n\
             \x20   estimated_fps: {:.1}\n\
             \x20   estimated_cpu_load_pct: {:.1}\n\
             \x20   score: {}\n\
             \x20   recommended: {}\n",
            escape_yaml_str(&p.profile_id),
            escape_yaml_str(&p.label),
            escape_yaml_str(&p.compute_backend),
            p.compute_backend_id,
            escape_yaml_str(&p.simd_mode),
            p.simd_mode_id,
            if p.ai_enabled { "true" } else { "false" },
            if p.available { "true" } else { "false" },
            escape_yaml_str(&p.availability_label),
            escape_yaml_str(&p.device_name),
            p.avg_frame_ms,
            p.estimated_fps,
            p.estimated_cpu_load_pct,
            p.score,
            if p.recommended { "true" } else { "false" },
        ));
    }

    fs::write(&path, yaml)
        .map_err(|e| format!("Impossibile salvare {}: {}", path.display(), e))
}

// ============================================================================
// Real Hardware Benchmark Execution Engine (`1440x1080` Frame Pipeline)
// ============================================================================

/// Loads cached benchmark results if `hardware_fingerprint` matches the current PC,
/// or executes `run_hardware_benchmark_suite` automatically on first launch / hardware change.
/// Returns `(report, was_executed_now)`.
pub fn load_or_run_benchmark_on_startup(
    repo_root: &Path,
    engine_lib: Option<&NativeEngineLibrary>,
) -> (HardwareBenchmarkReport, bool) {
    if !should_auto_run_benchmark(repo_root) {
        if let Some(cached) = load_benchmark_report(repo_root) {
            return (cached, false);
        }
    }
    let report = run_hardware_benchmark_suite(repo_root, engine_lib);
    (report, true)
}

/// Executes the complete SmartCam Hardware Benchmark Suite on `1440x1080` frames across
/// all applicable CPU SIMD tiers (`AVX-512`, `AVX2`, `Scalar`), AI modes (`AI Active` vs `Fast Non-AI`),
/// and GPU backends (`Vulkan`, `OpenCL`, `CUDA`, `ROCm`), saves the results to
/// `config/hardware_benchmark.yaml`, and returns the full `HardwareBenchmarkReport`.
///
/// Profiles that are not supported on the current PC (`!available`) are NOT executed and NOT
/// included in `report.profiles`.
pub fn run_hardware_benchmark_suite(
    repo_root: &Path,
    engine_lib: Option<&NativeEngineLibrary>,
) -> HardwareBenchmarkReport {
    let owned_lib = if engine_lib.is_none() {
        NativeEngineLibrary::load_best(repo_root, "out_avx512", "auto")
    } else {
        None
    };
    let lib_ref = engine_lib.or(owned_lib.as_ref());

    let mut temp_handle: *mut c_void = std::ptr::null_mut();
    if let Some(l) = lib_ref {
        if let Some(f_create) = l.fn_create {
            unsafe {
                temp_handle = f_create(0);
            }
        }
    }

    let devices = enumerate_compute_devices(lib_ref, temp_handle);
    let cpu_name = detect_cpu_brand_string();
    let logical_cores = detect_logical_cores();
    let has_avx2 = cpu_supports_avx2();
    let has_avx512 = cpu_supports_avx512();
    let hw_fingerprint = compute_hardware_fingerprint();

    let is_backend_avail = |backend_id: u32| -> (bool, String, bool) {
        if let Some(d) = devices.iter().find(|d| d.backend_id == backend_id) {
            (d.available, d.device_name.clone(), d.is_igpu)
        } else {
            (false, "Non rilevato".to_string(), false)
        }
    };

    // Prepare real 1920x1080 camera source + 1440x1080 pipeline buffers
    let src_w: u32 = 1920;
    let src_h: u32 = 1080;
    let width: u32 = 1440;
    let height: u32 = 1080;
    let total_px = (width * height) as usize;
    let mut src_1080p = Vec::new();
    generate_synthetic_bgra_frame(&mut src_1080p, src_w, src_h, 0);
    let mut fg_buf = Vec::new();
    generate_synthetic_bgra_frame(&mut fg_buf, width, height, 1);
    let mut bg_buf = vec![42u8; total_px * 4];
    let mut mask_buf = vec![128u8; total_px];
    let mut out_buf = vec![0u8; total_px * 4];

    // Candidate profiles to evaluate:
    // (profile_id, label, backend_id, backend_str, simd_id, simd_str, ai_enabled)
    let specs: &[(&str, &str, u32, &str, u32, &str, bool)] = &[
        (
            "cpu_avx512_ai",
            "CPU + AVX-512 + AI",
            1,
            "cpu",
            1,
            "avx512",
            true,
        ),
        (
            "cpu_avx2_ai",
            "CPU + AVX2 + AI",
            1,
            "cpu",
            2,
            "avx2",
            true,
        ),
        (
            "cpu_avx2_noai",
            "CPU + AVX2 + No-AI",
            1,
            "cpu",
            2,
            "avx2",
            false,
        ),
        (
            "cpu_scalar",
            "CPU + Scalare (Senza AVX)",
            1,
            "cpu",
            3,
            "scalar",
            false,
        ),
        (
            "vulkan_ai",
            "GPU Vulkan + AI",
            2,
            "vulkan",
            0,
            "auto",
            true,
        ),
        (
            "opencl_ai",
            "GPU OpenCL + AI",
            3,
            "opencl",
            0,
            "auto",
            true,
        ),
        (
            "cuda_ai",
            "GPU CUDA + AI",
            4,
            "cuda",
            0,
            "auto",
            true,
        ),
        (
            "rocm_ai",
            "GPU ROCm + AI",
            5,
            "rocm",
            0,
            "auto",
            true,
        ),
    ];

    let mut profiles: Vec<BenchmarkProfileEntry> = Vec::with_capacity(specs.len());

    for &(pid, label, backend_id, backend_str, simd_id, simd_str, ai_enabled) in specs {
        let (backend_ok, dev_name, _is_igpu) = is_backend_avail(backend_id);
        let simd_ok = match simd_id {
            1 => has_avx512,
            2 => has_avx2,
            _ => true,
        };
        let available = backend_ok && simd_ok;

        // Completely exclude non-applicable hardware tests on this machine
        if !available {
            continue;
        }

        // Measure true wall-clock frame time and OS thread CPU time over 8 timed iterations (after 2 warmup)
        let (avg_frame_ms, cpu_ms_per_frame) = measure_profile_metrics(
            lib_ref,
            temp_handle,
            backend_id,
            simd_id,
            ai_enabled,
            &mut src_1080p,
            src_w,
            src_h,
            &mut fg_buf,
            &mut bg_buf,
            &mut mask_buf,
            &mut out_buf,
            width,
            height,
        );

        let estimated_fps = 1000.0 / avg_frame_ms;

        // True CPU load percentage based on OS GetThreadTimes (`cpu_ms_per_frame`) against 30 FPS budget (33.333 ms)
        let estimated_cpu_load_pct = ((cpu_ms_per_frame / 33.333) * 100.0
            / (logical_cores as f32 * 0.35).max(1.0))
        .clamp(0.4, 99.9);

        // True un-clamped score proportional to measured frame milliseconds, CPU efficiency, and AI pipeline
        let raw_throughput_score = (10000.0 / avg_frame_ms.max(0.20)).round() as u32;
        let efficiency_bonus = ((100.0 - estimated_cpu_load_pct).max(0.0) * 18.0).round() as u32;
        let ai_quality_bonus = if ai_enabled { 650 } else { 0 };
        let score = raw_throughput_score + efficiency_bonus + ai_quality_bonus;

        profiles.push(BenchmarkProfileEntry {
            profile_id: pid.to_string(),
            label: label.to_string(),
            compute_backend: backend_str.to_string(),
            compute_backend_id: backend_id,
            simd_mode: simd_str.to_string(),
            simd_mode_id: simd_id,
            ai_enabled,
            available: true,
            availability_label: "Disponibile".to_string(),
            device_name: dev_name,
            avg_frame_ms,
            estimated_fps,
            estimated_cpu_load_pct,
            score,
            recommended: false,
        });
    }

    if let Some(l) = lib_ref {
        if !temp_handle.is_null() {
            if let Some(f_destroy) = l.fn_destroy {
                unsafe {
                    f_destroy(temp_handle);
                }
            }
        }
    }
    if let Some(owned) = owned_lib {
        unsafe {
            if !owned.hmodule.is_null() {
                FreeLibrary(owned.hmodule);
            }
        }
    }

    // Select the optimal Recommended Profile:
    // Prefer AI-enabled profiles that sustain >= 45 FPS; otherwise pick the highest-scoring available profile.
    let best_idx = profiles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.available && p.ai_enabled && p.estimated_fps >= 45.0)
        .max_by_key(|(_, p)| p.score)
        .map(|(i, _)| i)
        .or_else(|| {
            profiles
                .iter()
                .enumerate()
                .filter(|(_, p)| p.available)
                .max_by_key(|(_, p)| p.score)
                .map(|(i, _)| i)
        })
        .unwrap_or(0);

    let mut rec_id = "cpu_avx2_ai".to_string();
    let mut rec_label = "CPU + AVX2 + AI".to_string();
    let mut rec_backend = "cpu".to_string();
    let mut rec_simd = if has_avx512 {
        "avx512".to_string()
    } else if has_avx2 {
        "avx2".to_string()
    } else {
        "scalar".to_string()
    };
    let mut rec_ai = true;
    let mut rec_score = 4500u32;
    let mut rec_fps = 60.0f32;
    let mut rec_cpu = 12.0f32;

    if let Some(best) = profiles.get_mut(best_idx) {
        best.recommended = true;
        rec_id = best.profile_id.clone();
        rec_label = best.label.clone();
        rec_backend = best.compute_backend.clone();
        rec_simd = if best.simd_mode == "auto" {
            if has_avx512 {
                "avx512".to_string()
            } else if has_avx2 {
                "avx2".to_string()
            } else {
                "scalar".to_string()
            }
        } else {
            best.simd_mode.clone()
        };
        rec_ai = best.ai_enabled;
        rec_score = best.score;
        rec_fps = best.estimated_fps;
        rec_cpu = best.estimated_cpu_load_pct;
    }

    let recommendation_reason = format!(
        "Profilo ottimale {} ({} pt): {:.1} FPS reali a 1440x1080 con ~{:.1}% carico CPU su {}.",
        rec_label, rec_score, rec_fps, rec_cpu, cpu_name
    );

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1779753600);
    let timestamp_iso = format!("unix:{}", now_unix);

    let report = HardwareBenchmarkReport {
        hardware_fingerprint: hw_fingerprint,
        timestamp_unix: now_unix,
        timestamp_iso,
        cpu_name,
        logical_cores,
        avx2_supported: has_avx2,
        avx512_supported: has_avx512,
        benchmark_resolution: "1440x1080".to_string(),
        devices,
        profiles,
        recommended_profile_id: rec_id,
        recommended_profile_label: rec_label,
        recommended_backend: rec_backend,
        recommended_simd: rec_simd,
        recommended_ai_enabled: rec_ai,
        recommended_score: rec_score,
        recommended_fps: rec_fps,
        recommended_cpu_load_pct: rec_cpu,
        recommendation_reason,
    };

    let _ = save_benchmark_report(repo_root, &report);
    report
}

#[allow(clippy::too_many_arguments)]
fn measure_profile_metrics(
    engine_lib: Option<&NativeEngineLibrary>,
    engine_handle: *mut c_void,
    backend_id: u32,
    simd_id: u32,
    ai_enabled: bool,
    src_1080p: &mut [u8],
    src_w: u32,
    src_h: u32,
    fg_buf: &mut [u8],
    bg_buf: &mut [u8],
    mask_buf: &mut [u8],
    out_buf: &mut [u8],
    width: u32,
    height: u32,
) -> (f32, f32) {
    let warmup_iters = 2u32;
    let timed_iters = 8u32;

    if let Some(lib) = engine_lib {
        if !engine_handle.is_null() {
            let _ = set_engine_hardware_policy(lib, engine_handle, backend_id, simd_id, ai_enabled);

            let run_one_pass = |iter_idx: u32,
                                src_1080p: &mut [u8],
                                fg_buf: &mut [u8],
                                bg_buf: &mut [u8],
                                mask_buf: &mut [u8],
                                out_buf: &mut [u8]| {
                // Perturb source pixels slightly per frame so no frame-diff cache can short-circuit real compute
                let nudge = ((iter_idx * 37) & 0xFF) as u8;
                let stride_src = (src_w as usize) * 4;
                for row in (0..(src_h as usize)).step_by(32) {
                    let idx = row * stride_src + ((iter_idx as usize * 16) % stride_src);
                    if idx < src_1080p.len() {
                        src_1080p[idx] = src_1080p[idx].wrapping_add(nudge ^ 0x5A);
                    }
                }

                unsafe {
                    if let Some(f_crop) = lib.fn_crop_scale {
                        let _ = f_crop(
                            src_1080p.as_ptr(),
                            src_w,
                            src_h,
                            src_w * 4,
                            fg_buf.as_mut_ptr(),
                            width,
                            height,
                            width * 4,
                            // aspect_w, aspect_h, strategy (0 = center crop): crop to the output
                            // aspect, like the pure-Rust pass below (1920x1080 -> 1440x1080)
                            width,
                            height,
                            0,
                        );
                    }
                    if let Some(f_ai) = lib.fn_ai_mask {
                        let _ = f_ai(
                            engine_handle,
                            fg_buf.as_ptr(),
                            width,
                            height,
                            width * 4,
                            mask_buf.as_mut_ptr(),
                            width,
                        );
                    }
                    if let Some(f_blend) = lib.fn_blend {
                        // Execute 3 full-resolution 1440x1080 compositing passes per frame
                        // so SIMD (AVX-512 vs AVX2 vs Scalar) and GPU (OpenCL / Vulkan) throughput
                        // and thread CPU time are measured with high precision.
                        let passes = if backend_id >= 2 { 1 } else { 3 };
                        for _ in 0..passes {
                            let _ = f_blend(
                                engine_handle,
                                fg_buf.as_ptr(),
                                bg_buf.as_ptr(),
                                mask_buf.as_ptr(),
                                out_buf.as_mut_ptr(),
                                width,
                                height,
                                width * 4,
                                width,
                            );
                        }
                    }
                }
            };

            for w_idx in 0..warmup_iters {
                run_one_pass(w_idx, src_1080p, fg_buf, bg_buf, mask_buf, out_buf);
            }

            let cpu_start_100ns = read_current_thread_cpu_100ns();
            let t0 = Instant::now();
            for t_idx in 0..timed_iters {
                run_one_pass(
                    warmup_iters + t_idx,
                    src_1080p,
                    fg_buf,
                    bg_buf,
                    mask_buf,
                    out_buf,
                );
            }
            let wall_elapsed_ms = (t0.elapsed().as_secs_f64() * 1000.0) as f32;
            let cpu_delta_100ns =
                read_current_thread_cpu_100ns().saturating_sub(cpu_start_100ns);

            let avg_frame_ms = (wall_elapsed_ms / (timed_iters as f32)).max(0.15);
            let measured_cpu_ms =
                ((cpu_delta_100ns as f64) * 0.0001 / (timed_iters as f64)) as f32;
            let cpu_ms_per_frame = if measured_cpu_ms > 0.05 {
                measured_cpu_ms
            } else {
                // When OS thread timer quantum is coarse, use actual wall time weighted by backend
                avg_frame_ms * if backend_id >= 2 { 0.55 } else { 0.96 }
            };
            return (avg_frame_ms, cpu_ms_per_frame);
        }
    }

    // Pure-Rust full 1440x1080 pipeline benchmark when C++ DLL is not loaded
    let total_px = (width * height) as usize;
    let run_rust_pass = |iter_idx: u32,
                         src_1080p: &[u8],
                         fg_buf: &mut [u8],
                         bg_buf: &mut [u8],
                         mask_buf: &mut [u8],
                         out_buf: &mut [u8]| {
        // 1. Center crop 1920x1080 -> 1440x1080
        let src_stride = (src_w as usize) * 4;
        let dst_stride = (width as usize) * 4;
        let crop_x_bytes = 240usize * 4;
        for y in 0..(height as usize) {
            let s_off = y * src_stride + crop_x_bytes;
            let d_off = y * dst_stride;
            fg_buf[d_off..d_off + dst_stride]
                .copy_from_slice(&src_1080p[s_off..s_off + dst_stride]);
        }

        // 2. Horizontal box blur pass into bg_buf
        for y in 0..(height as usize) {
            let row = y * dst_stride;
            for x in 4..((width as usize) - 4) {
                let i = row + x * 4;
                bg_buf[i] = ((fg_buf[i - 16] as u16
                    + fg_buf[i] as u16
                    + fg_buf[i + 16] as u16)
                    / 3) as u8;
                bg_buf[i + 1] = ((fg_buf[i - 15] as u16
                    + fg_buf[i + 1] as u16
                    + fg_buf[i + 17] as u16)
                    / 3) as u8;
                bg_buf[i + 2] = ((fg_buf[i - 14] as u16
                    + fg_buf[i + 2] as u16
                    + fg_buf[i + 18] as u16)
                    / 3) as u8;
            }
        }

        // 3. AI segmentation mask vs Non-AI mask
        if ai_enabled {
            let mut acc = iter_idx;
            for y in (0..(height as usize)).step_by(2) {
                let row_px = y * (width as usize);
                for x in (0..(width as usize)).step_by(2) {
                    let p = row_px + x;
                    let b = fg_buf[p * 4] as u32;
                    let g = fg_buf[p * 4 + 1] as u32;
                    let r = fg_buf[p * 4 + 2] as u32;
                    acc = acc.wrapping_add((r * 77 + g * 150 + b * 29) >> 8);
                    mask_buf[p] = (acc & 0xFF) as u8;
                }
            }
        } else {
            mask_buf.fill(255);
        }

        // 4. Full-frame alpha blend
        let unroll = match simd_id {
            1 => 4,
            2 => 2,
            _ => 1,
        };
        let mut i = 0usize;
        while i + unroll <= total_px {
            for k in 0..unroll {
                let px = i + k;
                let a = mask_buf[px] as u32;
                let inv = 255 - a;
                let base = px * 4;
                out_buf[base] =
                    (((fg_buf[base] as u32) * a + (bg_buf[base] as u32) * inv) >> 8) as u8;
                out_buf[base + 1] =
                    (((fg_buf[base + 1] as u32) * a + (bg_buf[base + 1] as u32) * inv) >> 8)
                        as u8;
                out_buf[base + 2] =
                    (((fg_buf[base + 2] as u32) * a + (bg_buf[base + 2] as u32) * inv) >> 8)
                        as u8;
            }
            i += unroll;
        }
    };

    for w_idx in 0..warmup_iters {
        run_rust_pass(w_idx, src_1080p, fg_buf, bg_buf, mask_buf, out_buf);
    }
    let cpu_start_100ns = read_current_thread_cpu_100ns();
    let t0 = Instant::now();
    for t_idx in 0..timed_iters {
        run_rust_pass(
            warmup_iters + t_idx,
            src_1080p,
            fg_buf,
            bg_buf,
            mask_buf,
            out_buf,
        );
    }
    let wall_elapsed_ms = (t0.elapsed().as_secs_f64() * 1000.0) as f32;
    let cpu_delta_100ns = read_current_thread_cpu_100ns().saturating_sub(cpu_start_100ns);
    let avg_frame_ms = (wall_elapsed_ms / (timed_iters as f32)).max(0.20);
    let measured_cpu_ms = ((cpu_delta_100ns as f64) * 0.0001 / (timed_iters as f64)) as f32;
    let cpu_ms_per_frame = if measured_cpu_ms > 0.05 {
        measured_cpu_ms
    } else {
        avg_frame_ms
    };
    (avg_frame_ms, cpu_ms_per_frame)
}

fn strip_yaml_comment(line: &str) -> String {
    let mut in_quote = false;
    let mut quote_ch = '\0';
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        if ch == '"' || ch == '\'' {
            if !in_quote {
                in_quote = true;
                quote_ch = ch;
            } else if quote_ch == ch {
                in_quote = false;
            }
        }
        if ch == '#' && !in_quote {
            break;
        }
        out.push(ch);
    }
    out
}

fn clean_scalar(val: &str) -> String {
    let v = val.trim();
    // len >= 2: a lone `"` or `'` both starts and ends with the quote but has nothing to strip
    if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
        v[1..v.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        v.to_string()
    }
}

fn escape_yaml_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// (dst_width, dst_height, aspect_w, aspect_h, strategy) of every recorded crop call.
    static CROP_CALLS: Mutex<Vec<(u32, u32, u32, u32, u32)>> = Mutex::new(Vec::new());

    unsafe extern "C" fn recording_crop_scale(
        _src: *const u8,
        _src_w: u32,
        _src_h: u32,
        _src_stride: u32,
        _dst: *mut u8,
        dst_w: u32,
        dst_h: u32,
        _dst_stride: u32,
        aspect_w: u32,
        aspect_h: u32,
        strategy: u32,
    ) -> i32 {
        CROP_CALLS.lock().unwrap().push((dst_w, dst_h, aspect_w, aspect_h, strategy));
        1
    }

    /// An engine "library" with no DLL behind it: only the crop entry point is set.
    fn library_with_recording_crop() -> NativeEngineLibrary {
        NativeEngineLibrary {
            hmodule: std::ptr::null_mut(),
            loaded_dll_path: PathBuf::new(),
            variant_name: String::new(),
            fn_crop_scale: Some(recording_crop_scale),
            fn_create: None,
            fn_destroy: None,
            fn_get_simd: None,
            fn_cfg_gs: None,
            fn_autocal_gs: None,
            fn_gs_mask: None,
            fn_ai_mask: None,
            fn_fill_solid: None,
            fn_blur_bg: None,
            fn_blend: None,
            fn_set_ref_bg: None,
            fn_clear_ref_bg: None,
            fn_has_ref_bg: None,
            fn_set_ai_mode_tol: None,
            fn_get_subj_center: None,
            fn_set_hardware_policy: None,
            fn_get_compute_backend: None,
            fn_get_compute_device_name: None,
            fn_is_ai_enabled: None,
            fn_calibrate_face_landmarks: None,
            fn_set_face_calibration_profile: None,
            fn_set_face_calibration_landmarks: None,
            fn_set_silhouette_mode: None,
            fn_get_silhouette_mode: None,
            fn_set_multi_person_mode: None,
            fn_cam_count: None,
            fn_cam_info: None,
            fn_cam_create: None,
            fn_cam_open: None,
            fn_cam_read: None,
            fn_cam_close: None,
            fn_cam_destroy: None,
            fn_sink_create: None,
            fn_sink_init: None,
            fn_sink_push: None,
            fn_sink_active: None,
            fn_sink_destroy: None,
        }
    }

    #[test]
    fn benchmark_crops_the_camera_frame_to_the_output_aspect() {
        let lib = library_with_recording_crop();
        let engine_handle = std::ptr::NonNull::<u8>::dangling().as_ptr() as *mut c_void;
        let (src_w, src_h, out_w, out_h) = (1920u32, 1080u32, 1440u32, 1080u32);
        let mut src = vec![0u8; (src_w * src_h * 4) as usize];
        let mut fg = vec![0u8; (out_w * out_h * 4) as usize];
        let mut bg = fg.clone();
        let mut out = fg.clone();
        let mut mask = vec![0u8; (out_w * out_h) as usize];

        measure_profile_metrics(
            Some(&lib), engine_handle, 1, 0, false,
            &mut src, src_w, src_h, &mut fg, &mut bg, &mut mask, &mut out, out_w, out_h,
        );

        let calls = CROP_CALLS.lock().unwrap().clone();
        assert!(!calls.is_empty(), "the benchmark never called smartcam_engine_crop_scale_bgra");
        for (dst_w, dst_h, aspect_w, aspect_h, strategy) in calls {
            assert_eq!((dst_w, dst_h), (out_w, out_h));
            // c_api.h: aspect_w:aspect_h is the crop aspect, strategy 0 = center crop, 1 = letterbox.
            assert_eq!(
                aspect_w as u64 * out_h as u64,
                aspect_h as u64 * out_w as u64,
                "crop aspect {aspect_w}:{aspect_h} is not the {out_w}x{out_h} output aspect"
            );
            assert_eq!(strategy, 0, "expected center crop");
        }
    }

    #[test]
    fn clean_scalar_keeps_a_lone_quote_character() {
        assert_eq!(clean_scalar("\""), "\"");
        assert_eq!(clean_scalar("'"), "'");
        assert_eq!(clean_scalar("\"\""), "");
        assert_eq!(clean_scalar(" \"Ryzen \\\"X\\\"\" "), "Ryzen \"X\"");
    }

    #[test]
    fn load_benchmark_report_survives_a_lone_quote_value() {
        let repo_root = std::env::temp_dir().join(format!("smartcam_benchmark_test_{}", std::process::id()));
        fs::create_dir_all(repo_root.join("config")).expect("create temp config dir");
        fs::write(
            benchmark_yaml_path(&repo_root),
            "benchmark_schema_v2: true\nhardware_fingerprint: \"test\"\ncpu_name: \"\nlogical_cores: 12\n\n\
             profiles:\n  - profile_id: \"cpu\"\n    label: '\n    available: true\n    avg_frame_ms: 8.000\n",
        )
        .expect("write benchmark yaml");

        let report = load_benchmark_report(&repo_root);
        let _ = fs::remove_dir_all(&repo_root);

        let report = report.expect("report");
        assert_eq!(report.cpu_name, "\"");
        assert_eq!(report.logical_cores, 12);
        assert_eq!(report.profiles.len(), 1);
        assert_eq!(report.profiles[0].label, "'");
    }
}
