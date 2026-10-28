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

use std::ffi::{c_char, c_void, CStr, CString};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use crate::win32_ffi::{
    to_wstring, BitmapData, GdipBitmapLockBits, GdipBitmapUnlockBits, GdipDisposeImage,
    GdipGetImageHeight, GdipGetImageWidth, GdiplusStartup, GdiplusStartupInput, GpRect,
    ImageLockModeRead, PixelFormat32bppARGB, ShellExecuteW, FALSE, SW_HIDE,
};

/// Universal C ABI version for SmartCam external plugins (`.dll` / `.so`).
pub const SMARTCAM_PLUGIN_ABI_VERSION_V1: u32 = 1;

/// Exported C symbol queried by SmartCam when scanning `plugins/*.dll`.
pub const SMARTCAM_PLUGIN_QUERY_SYMBOL: &[u8] = b"smartcam_plugin_query_v1\0";

/// Built-in plugin IDs.
pub const PLUGIN_ID_PHONE_CAMERA: &str = "phone_camera_source";
pub const PLUGIN_ID_MULTI_PERSON: &str = "multi_person_tracker";
pub const PLUGIN_ID_WEBRTC_RTSP: &str = "webrtc_rtsp_source";
pub const PLUGIN_ID_STUDIO_COLOR: &str = "studio_color_enhancer";
pub const PLUGIN_ID_TELEPROMPTER: &str = "virtual_teleprompter_overlay";

/// Default TCP port for the built-in SmartCam Smartphone Camera HTTP Server.
pub const DEFAULT_PHONE_SERVER_PORT: u16 = 8080;

/// Normalizes short CLI/config aliases (`"phone_camera"`, `"multi_person"`, `"webrtc_rtsp"`, `"studio_color"`, `"teleprompter"`)
/// to their canonical plugin IDs.
pub fn normalize_plugin_id(raw_id: &str) -> String {
    match raw_id.trim().to_ascii_lowercase().as_str() {
        "phone_camera" | "phone_camera_source" | "smartphone" | "phone" => {
            PLUGIN_ID_PHONE_CAMERA.to_string()
        }
        "multi_person" | "multi_person_tracker" | "multiperson" | "people" | "faces" => {
            PLUGIN_ID_MULTI_PERSON.to_string()
        }
        "webrtc_rtsp" | "webrtc_rtsp_source" | "rtsp" | "webrtc" => {
            PLUGIN_ID_WEBRTC_RTSP.to_string()
        }
        "studio_color" | "studio_color_enhancer" | "color" | "denoise" => {
            PLUGIN_ID_STUDIO_COLOR.to_string()
        }
        "teleprompter" | "virtual_teleprompter_overlay" => PLUGIN_ID_TELEPROMPTER.to_string(),
        other => other.to_string(),
    }
}

const GMEM_MOVEABLE: u32 = 0x0002;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(lpLibFileName: *const u16) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *const c_void;
    fn FreeLibrary(hLibModule: *mut c_void) -> i32;
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> *mut c_void;
    fn GlobalLock(hMem: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(hMem: *mut c_void) -> i32;
    fn GlobalFree(hMem: *mut c_void) -> *mut c_void;
}

#[link(name = "ole32")]
extern "system" {
    fn CreateStreamOnHGlobal(
        hGlobal: *mut c_void,
        fDeleteOnRelease: i32,
        ppstm: *mut *mut c_void,
    ) -> i32;
}

#[link(name = "gdiplus")]
extern "system" {
    fn GdipCreateBitmapFromStream(stream: *mut c_void, bitmap: *mut *mut c_void) -> i32;
}

/// Universal C ABI v1 plugin descriptor (`SmartCamPluginDescriptorV1`), matching `common/plugins/iplugin_source.h`.
#[repr(C)]
pub struct SmartCamPluginDescriptorV1 {
    pub abi_version: u32,
    pub id: *const c_char,
    pub name: *const c_char,
    pub version: *const c_char,
    pub author: *const c_char,
    pub description: *const c_char,
    pub category: *const c_char,
    pub enabled_by_default: u32,
    pub init: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub open: Option<
        unsafe extern "C" fn(
            instance: *mut c_void,
            width: u32,
            height: u32,
            fps: u32,
            endpoint_uri: *const c_char,
        ) -> i32,
    >,
    pub read_frame: Option<
        unsafe extern "C" fn(
            instance: *mut c_void,
            out_bgra: *mut u8,
            width: u32,
            height: u32,
            stride: u32,
            timeout_ms: u32,
        ) -> i32,
    >,
    pub process_frame: Option<
        unsafe extern "C" fn(
            instance: *mut c_void,
            inout_bgra: *mut u8,
            inout_mask_gray8: *mut u8,
            width: u32,
            height: u32,
            stride: u32,
        ) -> i32,
    >,
    pub set_parameter: Option<
        unsafe extern "C" fn(
            instance: *mut c_void,
            key: *const c_char,
            value: *const c_char,
        ) -> i32,
    >,
    pub close: Option<unsafe extern "C" fn(instance: *mut c_void)>,
    pub destroy: Option<unsafe extern "C" fn(instance: *mut c_void)>,
}

pub type SmartCamPluginQueryFn = unsafe extern "C" fn() -> *const SmartCamPluginDescriptorV1;

/// Functional category of a SmartCam plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginCategory {
    /// Produces video frames (`BGRA32`) from a smartphone, IP camera, NDI, or virtual feed.
    CameraInput,
    /// Applies in-place color grading, LUT, denoising, or HUD/teleprompter overlays.
    VideoFilter,
    /// Computes or refines the `GRAY8` foreground segmentation mask (e.g., multi-person tracking).
    Segmentation,
}

impl PluginCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginCategory::CameraInput => "camera_input",
            PluginCategory::VideoFilter => "video_filter",
            PluginCategory::Segmentation => "segmentation",
        }
    }

    pub fn from_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "camera_input" | "input" | "source" | "camera" => PluginCategory::CameraInput,
            "segmentation" | "mask" | "ai_mask" => PluginCategory::Segmentation,
            _ => PluginCategory::VideoFilter,
        }
    }

    pub fn display_label_it(&self) -> &'static str {
        match self {
            PluginCategory::CameraInput => "Sorgente Video / Fotocamera",
            PluginCategory::VideoFilter => "Filtro Video / Overlay Studio",
            PluginCategory::Segmentation => "Segmentazione AI / Maschera",
        }
    }
}

/// Descriptive metadata and live user-controlled state for a built-in or external DLL plugin.
#[derive(Clone, Debug)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: PluginCategory,
    pub enabled: bool,
    pub enabled_by_default: bool,
    pub config_endpoint: String,
    pub status_label: String,
    pub is_external_dll: bool,
    pub dll_path: Option<PathBuf>,
}

impl PluginMetadata {
    pub fn short_status_badge_it(&self) -> &'static str {
        if self.enabled {
            "🟢 ATTIVO"
        } else {
            "⚪ DISATTIVATO"
        }
    }
}

struct ExternalDllPlugin {
    id: String,
    module_handle: *mut c_void,
    descriptor: *const SmartCamPluginDescriptorV1,
    instance: *mut c_void,
    is_opened: bool,
}

unsafe impl Send for ExternalDllPlugin {}

impl Drop for ExternalDllPlugin {
    fn drop(&mut self) {
        unsafe {
            if !self.descriptor.is_null() {
                let desc = &*self.descriptor;
                if self.is_opened {
                    if let Some(close_fn) = desc.close {
                        close_fn(self.instance);
                    }
                    self.is_opened = false;
                }
                if !self.instance.is_null() {
                    if let Some(destroy_fn) = desc.destroy {
                        destroy_fn(self.instance);
                    }
                    self.instance = std::ptr::null_mut();
                }
            }
            if !self.module_handle.is_null() {
                FreeLibrary(self.module_handle);
                self.module_handle = std::ptr::null_mut();
            }
        }
    }
}

unsafe fn cstr_to_string(ptr: *const c_char, fallback: &str) -> String {
    if ptr.is_null() {
        return fallback.to_string();
    }
    CStr::from_ptr(ptr)
        .to_str()
        .unwrap_or(fallback)
        .trim()
        .to_string()
}

/// Detects the primary local LAN IPv4 address of this PC (e.g. `"192.168.1.55"`),
/// preferring direct USB RNDIS (`192.168.42.x`) or Windows Mobile Hotspot (`192.168.137.1`) if active.
pub fn detect_local_lan_ip() -> String {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = sock.local_addr() {
                let ip = local_addr.ip().to_string();
                if !ip.is_empty() && ip != "0.0.0.0" && !ip.starts_with("127.") {
                    return ip;
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

/// Probes whether a direct Windows Mobile Hotspot (`192.168.137.1`) or Android/iOS USB Tethering
/// interface (`192.168.42.x` / `172.20.10.x`) is bound on this PC to bypass router Wi-Fi AP isolation.
pub fn detect_direct_hotspot_or_usb_ip() -> Option<String> {
    let candidates = [
        ("192.168.137.1", "192.168.137.2:80"),
        ("192.168.42.100", "192.168.42.129:80"),
        ("172.20.10.2", "172.20.10.1:80"),
    ];
    for (local_bind, probe_peer) in candidates {
        if TcpListener::bind((local_bind, 0)).is_ok() {
            return Some(local_bind.to_string());
        }
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(probe_peer).is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    let ip = addr.ip().to_string();
                    if ip.starts_with("192.168.137.")
                        || ip.starts_with("192.168.42.")
                        || ip.starts_with("172.20.10.")
                    {
                        return Some(ip);
                    }
                }
            }
        }
    }
    None
}

// ============================================================================
// Pure Rust Endpoint Parser, TLS Certificate Generator & Smartphone Server
// ============================================================================

/// Connection state for the Smartphone Camera Plugin (`phone_camera_source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PhoneConnectionState {
    /// Plugin disabled or closed.
    Disconnected = 0,
    /// Waiting for smartphone stream; renders HD Pairing Card Frame.
    PairingStandby = 1,
    /// Actively receiving BGRA32 video frames from the smartphone.
    StreamingActive = 2,
}

/// Parsed network endpoint for Smartphone Wi-Fi / USB / RTSP camera streaming,
/// ported from `common/plugins/phone_camera_plugin.cpp` (`ParseEndpointUri`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPhoneEndpoint {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub is_usb: bool,
}

impl Default for ParsedPhoneEndpoint {
    fn default() -> Self {
        Self {
            scheme: "http".to_string(),
            host: "192.168.1.100".to_string(),
            port: DEFAULT_PHONE_SERVER_PORT,
            path: "/video".to_string(),
            is_usb: false,
        }
    }
}

impl ParsedPhoneEndpoint {
    /// Parses an endpoint URI (e.g. `"http://192.168.1.100:8080/video"`, `"rtsp://192.168.1.120:8554/live"`,
    /// or `"usb://127.0.0.1:8080/video"`) into scheme, host, port, path, and USB flag.
    pub fn parse(uri: &str) -> Self {
        let mut out = Self::default();
        let trimmed = uri.trim();
        if trimmed.is_empty() {
            return out;
        }

        let work = if let Some((scheme_part, rest)) = trimmed.split_once("://") {
            out.scheme = scheme_part.trim().to_ascii_lowercase();
            rest
        } else {
            trimmed
        };

        let (host_port, path_part) = match work.find('/') {
            Some(idx) => (&work[..idx], &work[idx..]),
            None => (work, "/video"),
        };
        out.path = path_part.to_string();

        if let Some((h, p)) = host_port.split_once(':') {
            out.host = h.trim().to_string();
            if let Ok(parsed_port) = p.trim().parse::<u16>() {
                if parsed_port > 0 {
                    out.port = parsed_port;
                }
            }
        } else if !host_port.trim().is_empty() {
            out.host = host_port.trim().to_string();
            out.port = if out.scheme == "rtsp" {
                8554
            } else {
                DEFAULT_PHONE_SERVER_PORT
            };
        }

        out.is_usb = out.scheme == "usb"
            || out.host == "127.0.0.1"
            || out.host.eq_ignore_ascii_case("localhost");
        out
    }
}

/// Self-signed TLS 1.2/1.3 server configuration and DER certificate generated in-process via `rcgen` + `rustls`.
pub struct PhoneTlsMaterial {
    pub server_config: Arc<ServerConfig>,
    pub cert_der: Vec<u8>,
}

/// Generates an in-memory self-signed X.509 certificate and `rustls::ServerConfig` with Subject Alternative Names
/// for `localhost`, `127.0.0.1`, `192.168.137.1` (Windows Mobile Hotspot), and the current LAN / USB Tethering IP.
pub fn generate_phone_tls_material() -> Option<PhoneTlsMaterial> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "SmartCam Mobile HTTPS");
    params.distinguished_name = dn;

    let mut sans = Vec::new();
    if let Ok(localhost_ia5) = "localhost".to_string().try_into() {
        sans.push(SanType::DnsName(localhost_ia5));
    }

    let mut ip_sans = vec![
        IpAddr::from([127, 0, 0, 1]),
        IpAddr::from([192, 168, 137, 1]),
    ];
    if let Ok(lan_ip) = detect_local_lan_ip().parse::<IpAddr>() {
        if !ip_sans.contains(&lan_ip) {
            ip_sans.push(lan_ip);
        }
    }
    if let Some(direct_ip) =
        detect_direct_hotspot_or_usb_ip().and_then(|s| s.parse::<IpAddr>().ok())
    {
        if !ip_sans.contains(&direct_ip) {
            ip_sans.push(direct_ip);
        }
    }
    for ip in ip_sans {
        sans.push(SanType::IpAddress(ip));
    }
    params.subject_alt_names = sans;

    let key_pair = KeyPair::generate().ok()?;
    let cert = params.self_signed(&key_pair).ok()?;
    let cert_der_bytes = cert.der().as_ref().to_vec();
    let cert_der = CertificateDer::from(cert_der_bytes.clone());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server_config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .ok()?
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .ok()?;

    Some(PhoneTlsMaterial {
        server_config: Arc::new(server_config),
        cert_der: cert_der_bytes,
    })
}

/// Returns the cached in-process `PhoneTlsMaterial` (`rcgen` + `rustls`), initializing it once on first use.
pub fn get_or_init_phone_tls_material() -> Option<&'static PhoneTlsMaterial> {
    static TLS_MATERIAL: OnceLock<Option<PhoneTlsMaterial>> = OnceLock::new();
    TLS_MATERIAL
        .get_or_init(generate_phone_tls_material)
        .as_ref()
}

struct PhoneFrameSharedState {
    bgra: Vec<u8>,
    width: u32,
    height: u32,
    last_received: Option<Instant>,
    source_tag: String,
}

impl Default for PhoneFrameSharedState {
    fn default() -> Self {
        Self {
            bgra: Vec::new(),
            width: 0,
            height: 0,
            last_received: None,
            source_tag: String::new(),
        }
    }
}

struct PhoneCameraServer {
    stop_flag: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    bound_port: Arc<AtomicU16>,
    received_frames: Arc<AtomicU64>,
    tcp_connections: Arc<AtomicU64>,
    active_clients: Arc<AtomicUsize>,
    last_client_log: Arc<Mutex<String>>,
    shared_frame: Arc<Mutex<PhoneFrameSharedState>>,
    remote_pull_url: Arc<Mutex<String>>,
    http_thread: Option<thread::JoinHandle<()>>,
    tls_thread: Option<thread::JoinHandle<()>>,
    pull_thread: Option<thread::JoinHandle<()>>,
}

struct ActiveClientGuard(Arc<AtomicUsize>);
impl Drop for ActiveClientGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl PhoneCameraServer {
    fn new(
        port: u16,
        shared_frame: Arc<Mutex<PhoneFrameSharedState>>,
        received_frames: Arc<AtomicU64>,
        tcp_connections: Arc<AtomicU64>,
        last_client_log: Arc<Mutex<String>>,
        remote_pull_url: Arc<Mutex<String>>,
    ) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let is_running = Arc::new(AtomicBool::new(false));
        let bound_port = Arc::new(AtomicU16::new(port));
        let active_clients = Arc::new(AtomicUsize::new(0));

        let mut srv = Self {
            stop_flag,
            is_running,
            bound_port,
            received_frames,
            tcp_connections,
            active_clients,
            last_client_log,
            shared_frame,
            remote_pull_url,
            http_thread: None,
            tls_thread: None,
            pull_thread: None,
        };

        srv.spawn_threads(port);
        srv
    }

    fn spawn_threads(&mut self, port: u16) {
        let stop_http = Arc::clone(&self.stop_flag);
        let running_http = Arc::clone(&self.is_running);
        let port_http = Arc::clone(&self.bound_port);
        let frames_http = Arc::clone(&self.received_frames);
        let conns_http = Arc::clone(&self.tcp_connections);
        let active_http = Arc::clone(&self.active_clients);
        let log_http = Arc::clone(&self.last_client_log);
        let shared_http = Arc::clone(&self.shared_frame);

        // Bind synchronously on `0.0.0.0:<port>` (with short retry in case an old socket is closing)
        // so `is_running` is immediately accurate and there is zero race condition!
        let bind_addr = format!("0.0.0.0:{}", port);
        let mut bound_listener: Option<TcpListener> = None;
        for _ in 0..12 {
            if let Ok(l) = TcpListener::bind(&bind_addr) {
                bound_listener = Some(l);
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }

        if let Some(listener) = bound_listener {
            let _ = listener.set_nonblocking(true);
            port_http.store(port, Ordering::SeqCst);
            running_http.store(true, Ordering::SeqCst);

            let jh = thread::spawn(move || {
                const MAX_CONCURRENT_HTTP_CLIENTS: usize = 8;
                while !stop_http.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, peer_addr)) => {
                            if stop_http.load(Ordering::Relaxed) {
                                break;
                            }
                            if active_http.load(Ordering::Relaxed) >= MAX_CONCURRENT_HTTP_CLIENTS {
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                                continue;
                            }
                            active_http.fetch_add(1, Ordering::Relaxed);
                            let client_guard = ActiveClientGuard(Arc::clone(&active_http));
                            conns_http.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut lg) = log_http.lock() {
                                *lg = format!("Connessione TCP da {}", peer_addr.ip());
                            }
                            let stop_conn = Arc::clone(&stop_http);
                            let frames_conn = Arc::clone(&frames_http);
                            let log_conn = Arc::clone(&log_http);
                            let shared_conn = Arc::clone(&shared_http);
                            thread::spawn(move || {
                                let _guard = client_guard;
                                handle_phone_http_client(
                                    stream,
                                    peer_addr,
                                    port,
                                    stop_conn,
                                    frames_conn,
                                    log_conn,
                                    shared_conn,
                                );
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(8));
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                }
                running_http.store(false, Ordering::SeqCst);
            });
            self.http_thread = Some(jh);
        } else {
            running_http.store(false, Ordering::SeqCst);
        }

        // Also spawn the native Rust TLS/HTTPS loopback terminator (`127.0.0.1:<tls_port>`) using `rcgen` + `rustls`
        // so that both `0.0.0.0:<port>` (via single-port `0x16 0x03` multiplexing) and `127.0.0.1:<tls_port>`
        // serve TLS 1.2/1.3 in-process with zero external child processes!
        let _ = get_or_init_phone_tls_material();
        let stop_tls = Arc::clone(&self.stop_flag);
        let frames_tls = Arc::clone(&self.received_frames);
        let log_tls = Arc::clone(&self.last_client_log);
        let shared_tls = Arc::clone(&self.shared_frame);
        let tls_port = if port <= 64000 {
            port + 1000
        } else {
            port - 1000
        };
        let tls_bind_addr = format!("127.0.0.1:{}", tls_port);
        let mut bound_tls_listener: Option<TcpListener> = None;
        for _ in 0..8 {
            if let Ok(l) = TcpListener::bind(&tls_bind_addr) {
                bound_tls_listener = Some(l);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let tls_jh = thread::spawn(move || {
            run_local_tls_loopback_terminator(
                bound_tls_listener,
                port,
                stop_tls,
                frames_tls,
                log_tls,
                shared_tls,
            );
        });
        self.tls_thread = Some(tls_jh);

        // Also spawn the remote HTTP MJPEG / Snapshot puller for external apps (IP Webcam / DroidCam / RTSP-HTTP bridge)
        let stop_pull = Arc::clone(&self.stop_flag);
        let frames_pull = Arc::clone(&self.received_frames);
        let shared_pull = Arc::clone(&self.shared_frame);
        let url_pull = Arc::clone(&self.remote_pull_url);

        let pull_jh = thread::spawn(move || {
            run_external_phone_pull_worker(stop_pull, url_pull, frames_pull, shared_pull, port);
        });
        self.pull_thread = Some(pull_jh);
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let port = self.bound_port.load(Ordering::SeqCst);
        if port > 0 {
            let _ = TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], port)),
                Duration::from_millis(30),
            );
            let tls_port = if port <= 64000 {
                port + 1000
            } else {
                port - 1000
            };
            let _ = TcpStream::connect_timeout(
                &SocketAddr::from(([127, 0, 0, 1], tls_port)),
                Duration::from_millis(30),
            );
        }
        if let Some(jh) = self.http_thread.take() {
            let _ = jh.join();
        }
        if let Some(jh) = self.tls_thread.take() {
            let _ = jh.join();
        }
        if let Some(jh) = self.pull_thread.take() {
            let _ = jh.join();
        }
        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl Drop for PhoneCameraServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn handle_phone_http_client(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    server_port: u16,
    stop_flag: Arc<AtomicBool>,
    received_frames: Arc<AtomicU64>,
    last_client_log: Arc<Mutex<String>>,
    shared_frame: Arc<Mutex<PhoneFrameSharedState>>,
) {
    // CRITICAL ON WINDOWS (Winsock2): `accept()` inherits `nonblocking = true` from `listener`!
    // Explicitly restore blocking mode on the accepted client stream so `read()` honors `read_timeout`
    // instead of immediately failing with `WSAEWOULDBLOCK` before the phone's HTTP GET arrives!
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(5000)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(5000)));

    let mut initial_buf = Vec::with_capacity(16384);
    let mut temp = [0u8; 8192];
    let first_deadline = Instant::now() + Duration::from_millis(5000);

    while initial_buf.len() < 3 && !stop_flag.load(Ordering::Relaxed) {
        if Instant::now() >= first_deadline {
            return;
        }
        match stream.read(&mut temp) {
            Ok(0) => return,
            Ok(n) => initial_buf.extend_from_slice(&temp[..n]),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return,
        }
    }

    // Auto-detect TLS ClientHello (`0x16 0x03`) on the SAME port (`8080`):
    // Terminate TLS 1.2/1.3 directly in-process in pure Rust via `rustls::ServerConnection` + `StreamOwned`
    // so `https://<IP>:8080` works out-of-the-box on the exact same port as `http://<IP>:8080` with <1MB RAM!
    if initial_buf.len() >= 3 && initial_buf[0] == 0x16 && initial_buf[1] == 0x03 {
        if let Ok(mut lg) = last_client_log.lock() {
            *lg = format!(
                "🔒 Connessione HTTPS sicura da {} (Porta {})",
                peer_addr.ip(),
                server_port
            );
        }
        let Some(tls_mat) = get_or_init_phone_tls_material() else {
            return;
        };
        let Ok(mut tls_conn) = ServerConnection::new(Arc::clone(&tls_mat.server_config)) else {
            return;
        };
        let mut cursor = Cursor::new(&initial_buf[..]);
        while (cursor.position() as usize) < initial_buf.len() {
            if tls_conn.read_tls(&mut cursor).is_err() {
                return;
            }
            if tls_conn.process_new_packets().is_err() {
                return;
            }
        }
        let mut tls_stream = StreamOwned::new(tls_conn, stream);
        serve_phone_http_stream(
            &mut tls_stream,
            Vec::with_capacity(65536),
            peer_addr,
            server_port,
            &stop_flag,
            &received_frames,
            &last_client_log,
            &shared_frame,
        );
        return;
    }

    serve_phone_http_stream(
        &mut stream,
        initial_buf,
        peer_addr,
        server_port,
        &stop_flag,
        &received_frames,
        &last_client_log,
        &shared_frame,
    );
}

#[allow(clippy::too_many_arguments)]
fn serve_phone_http_stream<S: Read + Write>(
    stream: &mut S,
    mut read_buf: Vec<u8>,
    peer_addr: SocketAddr,
    server_port: u16,
    stop_flag: &Arc<AtomicBool>,
    received_frames: &Arc<AtomicU64>,
    last_client_log: &Arc<Mutex<String>>,
    shared_frame: &Arc<Mutex<PhoneFrameSharedState>>,
) {
    let mut temp = [0u8; 16384];

    // Support persistent keep-alive connections so mobile browsers can stream 25-30 FPS over a single socket
    while !stop_flag.load(Ordering::Relaxed) {
        let req_deadline = Instant::now() + Duration::from_millis(5000);
        let header_end = loop {
            if let Some(pos) = find_subslice(&read_buf, b"\r\n\r\n") {
                break Some(pos);
            }
            if read_buf.len() > 65536 || Instant::now() >= req_deadline {
                return;
            }
            match stream.read(&mut temp) {
                Ok(0) => return,
                Ok(n) => read_buf.extend_from_slice(&temp[..n]),
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::Interrupted =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        };

        let Some(hdr_end_idx) = header_end else {
            return;
        };
        let body_start = hdr_end_idx + 4;
        let header_str = String::from_utf8_lossy(&read_buf[..hdr_end_idx]).to_string();
        let mut lines = header_str.lines();
        let first_line = lines.next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let method = parts.next().unwrap_or("GET").to_ascii_uppercase();
        let path = parts.next().unwrap_or("/");

        let mut content_length: usize = 0;
        let mut keep_alive = true;
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                let key_low = k.trim().to_ascii_lowercase();
                let val_trim = v.trim();
                if key_low == "content-length" {
                    content_length = val_trim.parse::<usize>().unwrap_or(0);
                } else if key_low == "connection" && val_trim.eq_ignore_ascii_case("close") {
                    keep_alive = false;
                }
            }
        }

        if content_length > 16 * 1024 * 1024 {
            return;
        }

        let total_needed = body_start + content_length;
        let body_deadline = Instant::now() + Duration::from_millis(5000);
        while read_buf.len() < total_needed {
            if Instant::now() >= body_deadline {
                return;
            }
            match stream.read(&mut temp) {
                Ok(0) => return,
                Ok(n) => read_buf.extend_from_slice(&temp[..n]),
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::Interrupted =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        }

        let body = &read_buf[body_start..total_needed];

        if method == "OPTIONS" {
            let resp = "HTTP/1.1 204 No Content\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                        Access-Control-Allow-Headers: Content-Type\r\n\
                        Content-Length: 0\r\n\r\n";
            if stream.write_all(resp.as_bytes()).is_err() {
                return;
            }
        } else if method == "POST" && path.starts_with("/frame") {
            let (q_w, q_h, q_fmt) = parse_frame_query(path);
            let mut decoded_ok = false;

            if (q_fmt == "rgba" || q_fmt == "bgra" || q_fmt == "raw")
                && q_w >= 16
                && q_h >= 16
                && q_w <= 4096
                && q_h <= 4096
                && body.len() == (q_w as usize) * (q_h as usize) * 4
            {
                if let Ok(mut guard) = shared_frame.lock() {
                    let needed = body.len();
                    if guard.bgra.len() != needed {
                        guard.bgra.resize(needed, 255);
                    }
                    guard.bgra.copy_from_slice(body);
                    if q_fmt != "bgra" {
                        // Convert HTML5 Canvas ImageData RGBA -> SmartCam BGRA32 in-place
                        for px in guard.bgra.chunks_exact_mut(4) {
                            px.swap(0, 2);
                            px[3] = 255;
                        }
                    }
                    guard.width = q_w;
                    guard.height = q_h;
                    guard.last_received = Some(Instant::now());
                    guard.source_tag = format!("Smartphone Live ({})", peer_addr.ip());
                }
                received_frames.fetch_add(1, Ordering::Relaxed);
                decoded_ok = true;
            } else if !body.is_empty() {
                if let Some((bgra, w, h)) = decode_image_bytes_gdiplus(body) {
                    if let Ok(mut guard) = shared_frame.lock() {
                        guard.bgra = bgra;
                        guard.width = w;
                        guard.height = h;
                        guard.last_received = Some(Instant::now());
                        guard.source_tag = format!("Smartphone HD ({})", peer_addr.ip());
                    }
                    received_frames.fetch_add(1, Ordering::Relaxed);
                    decoded_ok = true;
                }
            }

            let count = received_frames.load(Ordering::Relaxed);
            if decoded_ok {
                if let Ok(mut lg) = last_client_log.lock() {
                    *lg = format!("🟢 Streaming da {} ({} frame)", peer_addr.ip(), count);
                }
            }
            let json_body = format!("{{\"ok\":{},\"frames\":{}}}", decoded_ok, count);
            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/json\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Connection: keep-alive\r\n\
                 Content-Length: {}\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            if stream.write_all(resp.as_bytes()).is_err() {
                return;
            }
        } else if method == "GET" && path.starts_with("/status") {
            let count = received_frames.load(Ordering::Relaxed);
            let json_body = format!(
                "{{\"ok\":true,\"port\":{},\"frames\":{}}}",
                server_port, count
            );
            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/json\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Cache-Control: no-store\r\n\
                 Content-Length: {}\r\n\r\n{}",
                json_body.len(),
                json_body
            );
            if stream.write_all(resp.as_bytes()).is_err() {
                return;
            }
        } else {
            if let Ok(mut lg) = last_client_log.lock() {
                *lg = format!("✅ Pagina aperta da {} (HTTP {})", peer_addr.ip(), path);
            }
            let lan_ip = detect_local_lan_ip();
            let html = build_smartphone_transmitter_html(&lan_ip, server_port);
            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: text/html; charset=utf-8\r\n\
                 Cache-Control: no-store, no-cache, must-revalidate\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Content-Length: {}\r\n\r\n{}",
                html.len(),
                html
            );
            if stream.write_all(resp.as_bytes()).is_err() {
                return;
            }
        }

        let _ = stream.flush();
        if !keep_alive {
            return;
        }
        read_buf.drain(..total_needed);
        if read_buf.capacity() > 262_144 {
            read_buf.shrink_to(65536);
        }
    }
}

fn parse_frame_query(path: &str) -> (u32, u32, String) {
    let mut w = 0u32;
    let mut h = 0u32;
    let mut fmt = "rgba".to_string();
    if let Some((_, query)) = path.split_once('?') {
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k.trim() {
                    "w" => w = v.trim().parse::<u32>().unwrap_or(0),
                    "h" => h = v.trim().parse::<u32>().unwrap_or(0),
                    "fmt" => fmt = v.trim().to_ascii_lowercase(),
                    _ => {}
                }
            }
        }
    }
    (w, h, fmt)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn ensure_gdiplus_initialized() -> bool {
    static GDIPLUS_TOKEN: OnceLock<usize> = OnceLock::new();
    let tok = *GDIPLUS_TOKEN.get_or_init(|| unsafe {
        let mut token: usize = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: std::ptr::null(),
            SuppressBackgroundThread: FALSE,
            SuppressExternalCodecs: FALSE,
        };
        if GdiplusStartup(&mut token, &input, std::ptr::null_mut()) == 0 {
            token
        } else {
            0
        }
    });
    tok != 0
}

/// Decodes in-memory JPEG, PNG, or BMP bytes into top-down `BGRA32` pixels via native Windows GDI+ `IStream`.
pub fn decode_image_bytes_gdiplus(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if bytes.len() < 16 || !ensure_gdiplus_initialized() {
        return None;
    }
    unsafe {
        let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if hglobal.is_null() {
            return None;
        }

        let locked_ptr = GlobalLock(hglobal) as *mut u8;
        if locked_ptr.is_null() {
            let _ = GlobalFree(hglobal);
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked_ptr, bytes.len());
        let _ = GlobalUnlock(hglobal);

        let mut stream: *mut c_void = std::ptr::null_mut();
        if CreateStreamOnHGlobal(hglobal, 1, &mut stream) != 0 || stream.is_null() {
            let _ = GlobalFree(hglobal);
            return None;
        }

        let mut bmp: *mut c_void = std::ptr::null_mut();
        let status = GdipCreateBitmapFromStream(stream, &mut bmp);
        if status != 0 || bmp.is_null() {
            release_com_interface(stream);
            return None;
        }

        let mut width: u32 = 0;
        let mut height: u32 = 0;
        let _ = GdipGetImageWidth(bmp, &mut width);
        let _ = GdipGetImageHeight(bmp, &mut height);

        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            let _ = GdipDisposeImage(bmp);
            release_com_interface(stream);
            return None;
        }

        let rect = GpRect {
            X: 0,
            Y: 0,
            Width: width as i32,
            Height: height as i32,
        };
        let mut bdata: BitmapData = std::mem::zeroed();
        if GdipBitmapLockBits(
            bmp,
            &rect,
            ImageLockModeRead,
            PixelFormat32bppARGB,
            &mut bdata,
        ) != 0
            || bdata.Scan0.is_null()
        {
            let _ = GdipDisposeImage(bmp);
            release_com_interface(stream);
            return None;
        }

        let row_bytes = (width * 4) as usize;
        let mut out = vec![255u8; row_bytes * (height as usize)];
        for y in 0..(height as usize) {
            let src_row = if bdata.Stride >= 0 {
                bdata.Scan0.add(y * (bdata.Stride as usize))
            } else {
                let abs_stride = (-bdata.Stride) as usize;
                bdata.Scan0.sub(((height as usize) - 1 - y) * abs_stride)
            };
            let dst_row = &mut out[y * row_bytes..(y + 1) * row_bytes];
            std::ptr::copy_nonoverlapping(src_row, dst_row.as_mut_ptr(), row_bytes);
            for px in dst_row.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }

        let _ = GdipBitmapUnlockBits(bmp, &mut bdata);
        let _ = GdipDisposeImage(bmp);
        release_com_interface(stream);
        Some((out, width, height))
    }
}

unsafe fn release_com_interface(unk: *mut c_void) {
    if unk.is_null() {
        return;
    }
    let vtable = *(unk as *const *const usize);
    if !vtable.is_null() {
        let release_fn: unsafe extern "system" fn(*mut c_void) -> u32 =
            std::mem::transmute(*vtable.add(2));
        let _ = release_fn(unk);
    }
}

/// Worker thread that pulls JPEG snapshots or MJPEG streams from external smartphone camera apps
/// (such as Android "IP Webcam", "DroidCam", or HTTP IP cameras) when the configured endpoint
/// points to a remote host different from the PC's own HTTP server.
fn run_external_phone_pull_worker(
    stop_flag: Arc<AtomicBool>,
    remote_pull_url: Arc<Mutex<String>>,
    received_frames: Arc<AtomicU64>,
    shared_frame: Arc<Mutex<PhoneFrameSharedState>>,
    local_server_port: u16,
) {
    let local_ip = detect_local_lan_ip();
    while !stop_flag.load(Ordering::Relaxed) {
        let target_url = remote_pull_url
            .lock()
            .map(|g| g.trim().to_string())
            .unwrap_or_default();

        // Skip pulling if we recently received pushed frames from the mobile web page
        let push_active = shared_frame
            .lock()
            .ok()
            .and_then(|g| g.last_received)
            .map(|t| t.elapsed() < Duration::from_secs(3))
            .unwrap_or(false);

        if push_active || target_url.is_empty() {
            thread::sleep(Duration::from_millis(400));
            continue;
        }

        if let Some((host, port, path)) = parse_http_endpoint(&target_url) {
            // Do not pull from our own local server port
            let is_self =
                (host == local_ip || host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost"))
                    && port == local_server_port;
            let is_default_placeholder = host == "192.168.1.100" && port == 8080;

            if !is_self && !is_default_placeholder {
                // Try snapshot endpoint first (or the exact user path)
                let candidate_paths = if path == "/video" || path == "/" {
                    vec!["/shot.jpg".to_string(), path.clone()]
                } else {
                    vec![path.clone()]
                };

                let mut pulled = false;
                for p in candidate_paths {
                    if let Some(jpeg_bytes) = fetch_http_jpeg_frame(&host, port, &p) {
                        if let Some((bgra, w, h)) = decode_image_bytes_gdiplus(&jpeg_bytes) {
                            if let Ok(mut guard) = shared_frame.lock() {
                                guard.bgra = bgra;
                                guard.width = w;
                                guard.height = h;
                                guard.last_received = Some(Instant::now());
                                guard.source_tag = format!("Pull {}:{}", host, port);
                            }
                            received_frames.fetch_add(1, Ordering::Relaxed);
                            pulled = true;
                            break;
                        }
                    }
                }

                if pulled {
                    thread::sleep(Duration::from_millis(45));
                    continue;
                }
            }
        }

        thread::sleep(Duration::from_millis(750));
    }
}

fn parse_http_endpoint(raw_url: &str) -> Option<(String, u16, String)> {
    let trimmed = raw_url.trim();
    let without_scheme = if let Some(rest) = trimmed.strip_prefix("http://") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("https://") {
        rest
    } else {
        return None;
    };

    let (host_port, path) = match without_scheme.find('/') {
        Some(idx) => (&without_scheme[..idx], &without_scheme[idx..]),
        None => (without_scheme, "/"),
    };

    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h.trim().to_string(), p.trim().parse::<u16>().ok()?),
        None => (host_port.trim().to_string(), 80),
    };

    if host.is_empty() {
        return None;
    }
    Some((host, port, path.to_string()))
}

/// Extracts a complete JPEG frame from an HTTP snapshot or `multipart/x-mixed-replace` MJPEG stream buffer.
/// Supports both explicit multipart `Content-Length` headers after `--boundary` markers and raw JPEG
/// SOI (`0xFF 0xD8 0xFF`) / EOI (`0xFF 0xD9`) delimiters. Returns `Some((jpeg_bytes, bytes_consumed))`.
pub fn extract_mjpeg_frame_from_buffer(buf: &[u8]) -> Option<(Vec<u8>, usize)> {
    // 1. Check for multipart MJPEG boundary header with explicit `Content-Length`
    if let Some(boundary_pos) = find_subslice(buf, b"--") {
        let after_boundary = &buf[boundary_pos..];
        if let Some(hdr_end_rel) = find_subslice(after_boundary, b"\r\n\r\n") {
            let hdr_slice = &after_boundary[..hdr_end_rel];
            let hdr_str = String::from_utf8_lossy(hdr_slice);
            let mut declared_len: Option<usize> = None;
            let mut is_jpeg_part = false;
            for line in hdr_str.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let k_low = k.trim().to_ascii_lowercase();
                    let v_trim = v.trim();
                    if k_low == "content-length" {
                        declared_len = v_trim.parse::<usize>().ok();
                    } else if k_low == "content-type"
                        && v_trim.to_ascii_lowercase().contains("jpeg")
                    {
                        is_jpeg_part = true;
                    }
                }
            }
            let payload_start = boundary_pos + hdr_end_rel + 4;
            if let Some(len) = declared_len {
                if len >= 4 && (is_jpeg_part || payload_start + 3 <= buf.len()) {
                    if buf.len() >= payload_start + len {
                        let candidate = &buf[payload_start..payload_start + len];
                        if candidate.starts_with(&[0xFF, 0xD8]) {
                            return Some((candidate.to_vec(), payload_start + len));
                        }
                    } else {
                        // Wait for more bytes to complete the declared Content-Length
                        return None;
                    }
                }
            }
        }
    }

    // 2. Standard JPEG SOI (0xFF 0xD8 0xFF) .. EOI (0xFF 0xD9) marker extraction
    if let Some(soi) = find_subslice(buf, &[0xFF, 0xD8, 0xFF]) {
        if let Some(eoi_rel) = find_subslice(&buf[soi + 3..], &[0xFF, 0xD9]) {
            let end = soi + 3 + eoi_rel + 2;
            return Some((buf[soi..end].to_vec(), end));
        }
    }
    None
}

fn fetch_http_jpeg_frame(host: &str, port: u16, path: &str) -> Option<Vec<u8>> {
    let addr_str = format!("{}:{}", host, port);
    let sock_addr = addr_str.to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&sock_addr, Duration::from_millis(650)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(900)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(650)));

    let req = format!(
        "GET {} HTTP/1.0\r\nHost: {}:{}\r\nUser-Agent: SmartCam/2.0\r\nConnection: close\r\n\r\n",
        path, host, port
    );
    stream.write_all(req.as_bytes()).ok()?;

    let mut buf = Vec::with_capacity(65536);
    let mut chunk = [0u8; 8192];
    while buf.len() < 1024 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some((jpeg_bytes, _consumed)) = extract_mjpeg_frame_from_buffer(&buf) {
                    return Some(jpeg_bytes);
                }
            }
            Err(_) => break,
        }
    }

    if let Some((jpeg_bytes, _)) = extract_mjpeg_frame_from_buffer(&buf) {
        return Some(jpeg_bytes);
    }
    if let Some(soi) = find_subslice(&buf, &[0xFF, 0xD8, 0xFF]) {
        return Some(buf[soi..].to_vec());
    }
    None
}

/// Bridges an incoming TLS connection (`ClientHello` starting with `0x16 0x03`) to the local
/// pure-Rust `rustls` TLS loopback listener (`127.0.0.1:<tls_loopback_port>`).
fn bridge_tls_connection_to_loopback(
    mut client_stream: TcpStream,
    initial_bytes: Vec<u8>,
    tls_loopback_port: u16,
) {
    let target = SocketAddr::from(([127, 0, 0, 1], tls_loopback_port));
    let Ok(mut tls_stream) = TcpStream::connect_timeout(&target, Duration::from_millis(1200))
    else {
        return;
    };
    let _ = tls_stream.set_nodelay(true);
    let _ = tls_stream.set_read_timeout(Some(Duration::from_millis(8000)));
    let _ = tls_stream.set_write_timeout(Some(Duration::from_millis(8000)));

    if tls_stream.write_all(&initial_bytes).is_err() {
        return;
    }
    let _ = tls_stream.flush();

    let Ok(mut client_reader) = client_stream.try_clone() else {
        return;
    };
    let Ok(mut tls_writer) = tls_stream.try_clone() else {
        return;
    };

    let c2t = thread::spawn(move || {
        let mut buf = [0u8; 16384];
        while let Ok(n) = client_reader.read(&mut buf) {
            if n == 0 || tls_writer.write_all(&buf[..n]).is_err() {
                break;
            }
            let _ = tls_writer.flush();
        }
        let _ = tls_writer.shutdown(std::net::Shutdown::Write);
    });

    let mut buf = [0u8; 16384];
    while let Ok(n) = tls_stream.read(&mut buf) {
        if n == 0 || client_stream.write_all(&buf[..n]).is_err() {
            break;
        }
        let _ = client_stream.flush();
    }
    let _ = client_stream.shutdown(std::net::Shutdown::Both);
    let _ = c2t.join();
}

/// Runs an in-process 100% pure-Rust TLS 1.2/1.3 listener (`rcgen` + `rustls`) on `127.0.0.1:<tls_port>`
/// with `< 1 MB` RAM overhead and zero external subprocesses (`powershell.exe` / `.NET SslStream` completely removed).
fn run_local_tls_loopback_terminator(
    bound_listener: Option<TcpListener>,
    http_port: u16,
    stop_flag: Arc<AtomicBool>,
    received_frames: Arc<AtomicU64>,
    last_client_log: Arc<Mutex<String>>,
    shared_frame: Arc<Mutex<PhoneFrameSharedState>>,
) {
    let tls_port = if http_port <= 64000 {
        http_port + 1000
    } else {
        http_port - 1000
    };

    let listener = match bound_listener {
        Some(l) => l,
        None => match TcpListener::bind(("127.0.0.1", tls_port)) {
            Ok(l) => l,
            Err(_) => return,
        },
    };
    let _ = listener.set_nonblocking(true);

    let Some(tls_mat) = get_or_init_phone_tls_material() else {
        return;
    };
    let server_config = Arc::clone(&tls_mat.server_config);

    while !stop_flag.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer_addr)) => {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(Duration::from_millis(5000)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(5000)));

                let cfg_conn = Arc::clone(&server_config);
                let stop_conn = Arc::clone(&stop_flag);
                let frames_conn = Arc::clone(&received_frames);
                let log_conn = Arc::clone(&last_client_log);
                let shared_conn = Arc::clone(&shared_frame);

                thread::spawn(move || {
                    let Ok(tls_conn) = ServerConnection::new(cfg_conn) else {
                        return;
                    };
                    let mut tls_stream = StreamOwned::new(tls_conn, stream);
                    serve_phone_http_stream(
                        &mut tls_stream,
                        Vec::with_capacity(65536),
                        peer_addr,
                        http_port,
                        &stop_conn,
                        &frames_conn,
                        &log_conn,
                        &shared_conn,
                    );
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(8));
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn build_smartphone_transmitter_html(lan_ip: &str, port: u16) -> String {
    let http_url = format!("http://{}:{}", lan_ip, port);
    let _https_url = format!("https://{}:{}", lan_ip, port);
    format!(
        r##"<!DOCTYPE html>
<html lang="it">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no,viewport-fit=cover">
<meta name="theme-color" content="#000000">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-status-bar-style" content="black">
<title>SmartCam OLED Mobile Transmitter ({http_url})</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; -webkit-tap-highlight-color: transparent; }}
  html, body {{
    background: #000000;
    color: #d1d5db;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    min-height: 100vh;
    overflow-x: hidden;
    user-select: none;
    -webkit-user-select: none;
  }}
  body {{ padding: 8px; text-align: center; }}
  .card {{
    max-width: 540px;
    margin: 0 auto;
    background: #030303;
    border: 1px solid #1f2937;
    border-radius: 14px;
    padding: 12px;
  }}
  .top-row {{
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-bottom: 6px;
  }}
  h1 {{ font-size: 16px; color: #10b981; font-weight: 700; text-align: left; }}
  .wake-pill {{
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 999px;
    background: #051b11;
    color: #34d399;
    border: 1px solid #065f46;
    white-space: nowrap;
  }}
  .status {{
    display: block;
    width: 100%;
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 700;
    background: #04130c;
    color: #6ee7b7;
    border: 1px solid #047857;
    margin-bottom: 8px;
  }}
  .https-alert {{
    display: none;
    background: #170d02;
    border: 1px solid #d97706;
    border-radius: 10px;
    padding: 10px;
    margin-bottom: 10px;
    text-align: left;
    font-size: 12px;
    color: #fde68a;
    line-height: 1.4;
  }}
  .https-alert button {{
    margin-top: 8px;
    width: 100%;
    background: #10b981;
    color: #000000;
    font-weight: 800;
    border: none;
    padding: 11px;
    border-radius: 8px;
    font-size: 14px;
  }}
  .video-wrap {{
    position: relative;
    width: 100%;
    background: #000000;
    border: 1px solid #18181b;
    border-radius: 10px;
    overflow: hidden;
    margin-bottom: 8px;
    min-height: 180px;
    max-height: 290px;
    display: flex;
    align-items: center;
    justify-content: center;
  }}
  canvas#previewCanvas {{
    max-width: 100%;
    max-height: 280px;
    width: auto;
    height: auto;
    object-fit: contain;
    display: block;
  }}
  .preview-off-msg {{
    display: none;
    padding: 36px 14px;
    color: #3f3f46;
    font-size: 12px;
    text-align: center;
  }}
  .telemetry-bar {{
    font-size: 11px;
    color: #6b7280;
    margin-bottom: 8px;
    display: flex;
    justify-content: space-between;
    padding: 0 4px;
  }}
  .grid-2 {{
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px;
    margin-bottom: 7px;
    text-align: left;
  }}
  .field-group {{
    display: flex;
    flex-direction: column;
    gap: 3px;
  }}
  .field-group.full {{
    grid-column: 1 / -1;
  }}
  label.fld-lbl {{
    font-size: 11px;
    color: #9ca3af;
    font-weight: 600;
    padding-left: 2px;
  }}
  select, button, .btn-label {{
    background: #09090b;
    color: #e5e7eb;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 9px 10px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    width: 100%;
  }}
  select:focus, button:focus {{
    outline: none;
    border-color: #10b981;
  }}
  .btn-row {{
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px;
    margin-bottom: 7px;
  }}
  button.primary {{
    background: #059669;
    color: #ffffff;
    border: 1px solid #10b981;
    font-weight: 800;
    padding: 11px;
    font-size: 14px;
    grid-column: 1 / -1;
  }}
  button.oled-btn {{
    background: #050505;
    color: #a1a1aa;
    border: 1px solid #27272a;
  }}
  button.oled-btn.active {{
    background: #042f2e;
    color: #5eead4;
    border-color: #0d9488;
  }}
  button.blackout-btn {{
    grid-column: 1 / -1;
    background: #000000;
    color: #34d399;
    border: 1px solid #065f46;
    padding: 11px;
    font-size: 13px;
    font-weight: 700;
  }}
  input[type=file] {{ display: none; }}
  /* Full-Screen Minimum-Brightness Pure Black OLED Saver Overlay (Zero Pixels Lit) */
  #oledBlackoutOverlay {{
    display: none;
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    background: #000000;
    z-index: 999999;
    touch-action: manipulation;
    cursor: none;
  }}
</style>
</head>
<body>

<!-- 100% Pure Black Fullscreen OLED Saver Overlay (Zero Text/Pixels — Double-Tap or Swipe Back to Return) -->
<div id="oledBlackoutOverlay"></div>

<div class="card">
  <div class="top-row">
    <h1>📱 SmartCam OLED Studio</h1>
    <span id="wakeBadge" class="wake-pill">☀️ Anti-Standby: In attesa</span>
  </div>

  <div id="statusBadge" class="status">🟢 Connesso al PC — Premi Avvia Fotocamera</div>

  <!-- Shown only if the mobile browser opened HTTP instead of HTTPS -->
  <div id="httpsBanner" class="https-alert">
    <b>🔒 Sblocco Permesso Fotocamera Mobile (HTTPS):</b><br>
    I browser Android/iOS richiedono <b>HTTPS</b> per mostrare il popup del permesso fotocamera e l'elenco obiettivi. Tocca il pulsante verde qui sotto (se compare l'avviso del certificato locale, tocca <i>"Avanzate → Procedi comunque"</i>):
    <button onclick="switchToHttpsNow()">🔒 Passa a HTTPS e Richiedi Permesso Fotocamera</button>
  </div>

  <div class="video-wrap" id="videoWrapBox">
    <video id="camVideo" autoplay playsinline muted style="display:none"></video>
    <canvas id="previewCanvas" width="640" height="480"></canvas>
    <div id="previewOffMsg" class="preview-off-msg">
      🌑 ANTEPRIMA LOCALE SPENTA (Risparmio OLED)<br>
      <span>La fotocamera continua a trasmettere al PC in background</span>
    </div>
  </div>

  <div class="telemetry-bar">
    <span id="sensorInfoTxt">Sensore: —</span>
    <span id="txInfoTxt">Invio PC: —</span>
  </div>

  <!-- Dropdown Controls: Camera Device, Scale/Bandwidth, FPS, Rotation -->
  <div class="grid-2">
    <div class="field-group full">
      <label class="fld-lbl" for="cameraDeviceSel">📷 Fotocamera / Obiettivo del Telefono:</label>
      <select id="cameraDeviceSel" onchange="onCameraSelectionChange()">
        <option value="env_default">📸 Fotocamera Posteriore (Predefinita)</option>
        <option value="user_default">🤳 Fotocamera Anteriore (Selfie)</option>
      </select>
    </div>

    <div class="field-group">
      <label class="fld-lbl" for="scaleSel">📐 Risoluzione & Banda:</label>
      <select id="scaleSel" onchange="updateStreamDimensions()">
        <option value="1.00">100% (Nativa Sensore)</option>
        <option value="0.75">75% (Alta Qualità)</option>
        <option value="0.55" selected>55% (Fluido / Risparmio Banda)</option>
        <option value="0.38">38% (Bassa Banda)</option>
        <option value="0.25">25% (Minimo Consumo)</option>
      </select>
    </div>

    <div class="field-group">
      <label class="fld-lbl" for="fpsSel">⏱️ Velocità (FPS):</label>
      <select id="fpsSel" onchange="onFpsChange()">
        <option value="60">60 FPS (Ultra Fluido)</option>
        <option value="30" selected>30 FPS (Standard)</option>
        <option value="24">24 FPS (Cinema)</option>
        <option value="15">15 FPS (Risparmio Batteria)</option>
        <option value="10">10 FPS (Bassa Banda)</option>
        <option value="5">5 FPS (Minimo)</option>
      </select>
    </div>

    <div class="field-group">
      <label class="fld-lbl" for="rotSel">🔄 Rotazione & Orientamento:</label>
      <select id="rotSel" onchange="updateStreamDimensions()">
        <option value="auto" selected>🔄 Auto (Rotazione Reale Telefono)</option>
        <option value="0">⬆️ 0° (Normale)</option>
        <option value="90">⤵️ 90° (Ruota Destra)</option>
        <option value="180">⬇️ 180° (Capovolto)</option>
        <option value="270">⤴️ 270° (Ruota Sinistra)</option>
      </select>
    </div>

    <div class="field-group">
      <label class="fld-lbl" for="mirrorSel">🪞 Specchiatura Video:</label>
      <select id="mirrorSel" onchange="updateStreamDimensions()">
        <option value="0" selected>Normale (Non specchiato)</option>
        <option value="1">🪞 Specchia Orizzontale</option>
      </select>
    </div>
  </div>

  <div class="btn-row">
    <button id="btnLive" class="primary" onclick="startLiveCamera(true)">🎥 Richiedi Permesso & Avvia Fotocamera Live</button>

    <button id="btnTogglePreview" class="oled-btn" onclick="toggleLocalPreview()">👁️ Spegni Anteprima</button>
    <button class="oled-btn" onclick="refreshCameraList(true)">🔄 Rileva Obiettivi</button>

    <button class="blackout-btn" onclick="enterOledBlackoutFullscreen()">
      🌑 Schermo Nero Fullscreen a Luminosità Minima (2 Tap per Uscire)
    </button>
  </div>

  <div class="btn-row" style="margin-top:4px;">
    <label for="nativeCamInput" class="btn-label oled-btn" style="text-align:center;">
      📸 Fotocamera Nativa (Fallback)
    </label>
    <input id="nativeCamInput" type="file" accept="image/*" capture="environment" onchange="onNativePhoto(event)">
    <button class="oled-btn" onclick="startInteractiveTestStream()">📡 Segnale Test</button>
  </div>
</div>

<script>
const previewCanvas = document.getElementById('previewCanvas');
const previewCtx = previewCanvas.getContext('2d', {{ alpha: false }});
// Offscreen capture canvas so we can turn off local preview rendering (`localPreviewEnabled = false`)
// while continuing to capture & transmit frames to the PC at full speed!
const txCanvas = document.createElement('canvas');
const txCtx = txCanvas.getContext('2d', {{ alpha: false, willReadFrequently: true }});

const video = document.getElementById('camVideo');
const badge = document.getElementById('statusBadge');
const wakeBadge = document.getElementById('wakeBadge');
const sensorInfoTxt = document.getElementById('sensorInfoTxt');
const txInfoTxt = document.getElementById('txInfoTxt');
const cameraSel = document.getElementById('cameraDeviceSel');

let streamingMode = 'none';
let sentFrames = 0;
let busySending = false;
let currentStream = null;
let loadedImage = null;
let animTick = 0;
let localPreviewEnabled = true;
let blackoutActive = false;
let wakeLockSentinel = null;
let noSleepAudioCtx = null;
let pumpTimerId = null;

// Sensor native dimensions reported by the phone camera
let sensorNativeW = 640;
let sensorNativeH = 480;

// 1. Check Secure Context (HTTPS) for mobile camera permissions
function checkHttpsRequirement() {{
  const isLocal = location.hostname === 'localhost' || location.hostname === '127.0.0.1';
  if (!window.isSecureContext && !isLocal && location.protocol === 'http:') {{
    document.getElementById('httpsBanner').style.display = 'block';
    badge.textContent = '🔒 Tocca il pulsante verde HTTPS qui sotto per sbloccare il permesso fotocamera';
    // Automatically redirect to HTTPS on the same port if not yet attempted in this session
    if (!sessionStorage.getItem('smartcam_https_tried')) {{
      sessionStorage.setItem('smartcam_https_tried', '1');
      setTimeout(() => {{
        switchToHttpsNow();
      }}, 450);
    }}
  }} else if (window.isSecureContext) {{
    document.getElementById('httpsBanner').style.display = 'none';
    // Automatically request camera permission & enumerate lenses on load
    setTimeout(() => {{
      startLiveCamera(false);
    }}, 250);
  }}
}}

function switchToHttpsNow() {{
  const target = 'https://' + location.host + location.pathname;
  location.href = target;
}}

// 2. Anti-Standby: Screen Wake Lock API + WebAudio Silent Oscillator Fallback
async function enableWakeLockAntiStandby() {{
  try {{
    if ('wakeLock' in navigator && !wakeLockSentinel) {{
      wakeLockSentinel = await navigator.wakeLock.request('screen');
      wakeBadge.textContent = '☀️ Anti-Standby: ATTIVO';
      wakeLockSentinel.addEventListener('release', () => {{
        wakeLockSentinel = null;
        wakeBadge.textContent = '☀️ Anti-Standby: Riattivazione...';
      }});
    }}
  }} catch (e) {{
    // Fallback below
  }}
  // Silent WebAudio keep-awake for mobile browsers without WakeLock API
  try {{
    const AudioCtx = window.AudioContext || window.webkitAudioContext;
    if (AudioCtx && !noSleepAudioCtx) {{
      noSleepAudioCtx = new AudioCtx();
      const osc = noSleepAudioCtx.createOscillator();
      const gain = noSleepAudioCtx.createGain();
      gain.gain.value = 0.00001;
      osc.connect(gain);
      gain.connect(noSleepAudioCtx.destination);
      osc.start();
      if (!wakeLockSentinel) {{
        wakeBadge.textContent = '☀️ Anti-Standby: ATTIVO (AudioKeep)';
      }}
    }} else if (noSleepAudioCtx && noSleepAudioCtx.state === 'suspended') {{
      await noSleepAudioCtx.resume();
    }}
  }} catch (e) {{}}
}}

document.addEventListener('visibilitychange', () => {{
  if (document.visibilityState === 'visible' && streamingMode !== 'none') {{
    enableWakeLockAntiStandby();
  }}
}});

// 3. Enumerate all physical cameras on the smartphone (`enumerateDevices()`)
async function refreshCameraList(preserveCurrent) {{
  if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) return;
  try {{
    const devices = await navigator.mediaDevices.enumerateDevices();
    const videoInputs = devices.filter(d => d.kind === 'videoinput');
    if (videoInputs.length === 0) return;

    const prevVal = cameraSel.value;
    cameraSel.innerHTML = '';

    videoInputs.forEach((dev, idx) => {{
      const opt = document.createElement('option');
      opt.value = dev.deviceId || ('idx_' + idx);
      const rawLabel = (dev.label || '').trim();
      let icon = '📷';
      const low = rawLabel.toLowerCase();
      if (low.includes('front') || low.includes('user') || low.includes('anteriore') || low.includes('selfie')) {{
        icon = '🤳';
      }} else if (low.includes('ultra') || low.includes('wide') || low.includes('0.5')) {{
        icon = '📐';
      }} else if (low.includes('tele') || low.includes('zoom')) {{
        icon = '🔭';
      }}
      opt.textContent = rawLabel
        ? (icon + ' [' + (idx + 1) + '] ' + rawLabel)
        : (icon + ' Fotocamera Telefono #' + (idx + 1));
      cameraSel.appendChild(opt);
    }});

    // Also keep explicit facingMode fallbacks at the end of the dropdown
    const optEnv = document.createElement('option');
    optEnv.value = 'env_default';
    optEnv.textContent = '📸 Posteriore Automatica (environment)';
    cameraSel.appendChild(optEnv);

    const optUsr = document.createElement('option');
    optUsr.value = 'user_default';
    optUsr.textContent = '🤳 Anteriore Automatica (user)';
    cameraSel.appendChild(optUsr);

    if (preserveCurrent && Array.from(cameraSel.options).some(o => o.value === prevVal)) {{
      cameraSel.value = prevVal;
    }} else if (currentStream) {{
      const track = currentStream.getVideoTracks()[0];
      const settings = track && track.getSettings ? track.getSettings() : {{}};
      if (settings.deviceId && Array.from(cameraSel.options).some(o => o.value === settings.deviceId)) {{
        cameraSel.value = settings.deviceId;
      }}
    }}
  }} catch (e) {{}}
}}

function onCameraSelectionChange() {{
  startLiveCamera(true);
}}

function getSelectedFps() {{
  return parseInt(document.getElementById('fpsSel').value, 10) || 30;
}}

async function onFpsChange() {{
  const fps = getSelectedFps();
  if (currentStream) {{
    const track = currentStream.getVideoTracks()[0];
    if (track && track.applyConstraints) {{
      try {{
        await track.applyConstraints({{ frameRate: {{ ideal: fps, max: fps }} }});
      }} catch (e) {{}}
    }}
  }}
  updateStreamDimensions();
}}

// 4. Real Phone Orientation & Native Sensor Aspect-Ratio Scaling
function getEffectiveRotationDeg() {{
  const sel = document.getElementById('rotSel').value;
  if (sel !== 'auto') {{
    return parseInt(sel, 10) || 0;
  }}
  // Note: On mobile browsers, `video.videoWidth` and `video.videoHeight` already reflect the physical
  // portrait/landscape orientation when the browser rotates the video element, OR `screen.orientation.angle`
  // reports the device rotation when locked. We expose both automatic detection and 0/90/180/270 override.
  return 0;
}}

function updateStreamDimensions() {{
  if (streamingMode === 'camera' && video.videoWidth > 0 && video.videoHeight > 0) {{
    sensorNativeW = video.videoWidth;
    sensorNativeH = video.videoHeight;
  }} else if (streamingMode === 'photo' && loadedImage) {{
    sensorNativeW = loadedImage.naturalWidth || 640;
    sensorNativeH = loadedImage.naturalHeight || 480;
  }}

  const scale = parseFloat(document.getElementById('scaleSel').value) || 0.55;
  const rot = getEffectiveRotationDeg();
  const swapAxes = (rot === 90 || rot === 270);

  const baseW = swapAxes ? sensorNativeH : sensorNativeW;
  const baseH = swapAxes ? sensorNativeW : sensorNativeH;

  // Keep dimensions even multiples of 2 while preserving exact sensor aspect ratio
  const outW = Math.max(64, (Math.round(baseW * scale) & ~1));
  const outH = Math.max(64, (Math.round(baseH * scale) & ~1));

  if (txCanvas.width !== outW || txCanvas.height !== outH) {{
    txCanvas.width = outW;
    txCanvas.height = outH;
  }}
  if (previewCanvas.width !== outW || previewCanvas.height !== outH) {{
    previewCanvas.width = outW;
    previewCanvas.height = outH;
  }}
  previewCanvas.style.aspectRatio = outW + ' / ' + outH;

  const gcd = (a, b) => (b === 0 ? a : gcd(b, a % b));
  const g = gcd(sensorNativeW, sensorNativeH) || 1;
  const arW = Math.round(sensorNativeW / g);
  const arH = Math.round(sensorNativeH / g);
  const arLabel = (arW <= 32 && arH <= 32) ? (arW + ':' + arH) : (sensorNativeW / sensorNativeH).toFixed(2) + ':1';

  sensorInfoTxt.textContent = 'Sensore: ' + sensorNativeW + '×' + sensorNativeH + ' (' + arLabel + ')';
  txInfoTxt.textContent = 'Output PC: ' + outW + '×' + outH + ' @ ' + getSelectedFps() + ' FPS';
}}

video.addEventListener('loadedmetadata', updateStreamDimensions);
video.addEventListener('resize', updateStreamDimensions);
window.addEventListener('resize', () => {{
  setTimeout(updateStreamDimensions, 150);
}});
if (screen.orientation && screen.orientation.addEventListener) {{
  screen.orientation.addEventListener('change', () => {{
    setTimeout(updateStreamDimensions, 150);
  }});
}}

// 5. Request Camera Permission & Start Live Stream
async function startLiveCamera(userInitiated) {{
  if (userInitiated) {{
    enableWakeLockAntiStandby();
  }}

  if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {{
    if (!window.isSecureContext && location.protocol === 'http:') {{
      document.getElementById('httpsBanner').style.display = 'block';
      badge.textContent = '🔒 Browser in HTTP: premi il pulsante verde HTTPS sopra per il permesso fotocamera!';
      if (userInitiated) {{
        switchToHttpsNow();
      }}
      return;
    }}
    badge.textContent = '⚠️ API Fotocamera non disponibile in questo browser.';
    return;
  }}

  const fps = getSelectedFps();
  const chosenId = cameraSel.value;
  let videoConstraints = {{
    frameRate: {{ ideal: fps }}
  }};

  if (chosenId === 'env_default' || !chosenId) {{
    videoConstraints.facingMode = {{ ideal: 'environment' }};
  }} else if (chosenId === 'user_default') {{
    videoConstraints.facingMode = {{ ideal: 'user' }};
  }} else {{
    videoConstraints.deviceId = {{ exact: chosenId }};
  }}

  try {{
    badge.textContent = '⏳ Richiesta permesso fotocamera in corso...';
    if (currentStream) {{
      currentStream.getTracks().forEach(t => t.stop());
      currentStream = null;
    }}

    try {{
      currentStream = await navigator.mediaDevices.getUserMedia({{
        video: videoConstraints,
        audio: false
      }});
    }} catch (firstErr) {{
      // Fallback if exact constraint failed on a specific lens
      currentStream = await navigator.mediaDevices.getUserMedia({{
        video: true,
        audio: false
      }});
    }}

    video.srcObject = currentStream;
    await video.play();

    // Populate the dropdown with all real phone cameras now that permission is granted!
    await refreshCameraList(true);

    sensorNativeW = video.videoWidth || 640;
    sensorNativeH = video.videoHeight || 480;
    streamingMode = 'camera';
    updateStreamDimensions();
    enableWakeLockAntiStandby();

    badge.textContent = '🟢 IN TRASMISSIONE LIVE VERSO SMARTCAM PC';
    scheduleNextFrame();
  }} catch (err) {{
    badge.textContent = '⚠️ Permesso fotocamera negato o bloccato (' + err.name + ') — Concedi il permesso nel lucchetto del browser';
  }}
}}

// 6. Toggle Local Preview ON/OFF (OLED Power Saving)
function toggleLocalPreview() {{
  localPreviewEnabled = !localPreviewEnabled;
  const btn = document.getElementById('btnTogglePreview');
  const offMsg = document.getElementById('previewOffMsg');
  if (localPreviewEnabled) {{
    previewCanvas.style.display = 'block';
    offMsg.style.display = 'none';
    btn.textContent = '👁️ Spegni Anteprima';
    btn.classList.remove('active');
  }} else {{
    previewCanvas.style.display = 'none';
    offMsg.style.display = 'block';
    btn.textContent = '👁️ Accendi Anteprima';
    btn.classList.add('active');
  }}
}}

// 7. Full-Screen Minimum-Brightness 100% Pure Black Screen Saver (Double-Tap or Swipe Back to Exit)
const blackoutEl = document.getElementById('oledBlackoutOverlay');
let lastTapTime = 0;

function enterOledBlackoutFullscreen() {{
  enableWakeLockAntiStandby();
  blackoutActive = true;
  blackoutEl.style.display = 'block';
  try {{
    history.pushState({{ smartcamOled: 1 }}, '');
  }} catch (e) {{}}
  const docEl = document.documentElement;
  if (docEl.requestFullscreen) {{
    docEl.requestFullscreen().catch(() => {{}});
  }} else if (docEl.webkitRequestFullscreen) {{
    docEl.webkitRequestFullscreen();
  }}
}}

function exitOledBlackoutFullscreen() {{
  blackoutActive = false;
  blackoutEl.style.display = 'none';
  if (document.fullscreenElement && document.exitFullscreen) {{
    document.exitFullscreen().catch(() => {{}});
  }} else if (document.webkitFullscreenElement && document.webkitExitFullscreen) {{
    document.webkitExitFullscreen();
  }}
}}

window.addEventListener('popstate', () => {{
  if (blackoutActive) {{
    exitOledBlackoutFullscreen();
  }}
}});
document.addEventListener('fullscreenchange', () => {{
  if (!document.fullscreenElement && blackoutActive) {{
    blackoutActive = false;
    blackoutEl.style.display = 'none';
  }}
}});
document.addEventListener('webkitfullscreenchange', () => {{
  if (!document.webkitFullscreenElement && blackoutActive) {{
    blackoutActive = false;
    blackoutEl.style.display = 'none';
  }}
}});

blackoutEl.addEventListener('touchend', (ev) => {{
  const now = performance.now();
  if (now - lastTapTime < 360) {{
    ev.preventDefault();
    exitOledBlackoutFullscreen();
    lastTapTime = 0;
  }} else {{
    lastTapTime = now;
  }}
}});
blackoutEl.addEventListener('dblclick', (ev) => {{
  ev.preventDefault();
  exitOledBlackoutFullscreen();
}});

function onNativePhoto(ev) {{
  const file = ev.target.files && ev.target.files[0];
  if (!file) return;
  enableWakeLockAntiStandby();
  const reader = new FileReader();
  reader.onload = function(e) {{
    const img = new Image();
    img.onload = function() {{
      loadedImage = img;
      sensorNativeW = img.naturalWidth || 640;
      sensorNativeH = img.naturalHeight || 480;
      streamingMode = 'photo';
      updateStreamDimensions();
      badge.textContent = '🟢 FOTO NATIVA IN TRASMISSIONE AL PC';
      scheduleNextFrame();
    }};
    img.src = e.target.result;
  }};
  reader.readAsDataURL(file);
}}

function startInteractiveTestStream() {{
  enableWakeLockAntiStandby();
  sensorNativeW = 640;
  sensorNativeH = 480;
  streamingMode = 'test';
  updateStreamDimensions();
  badge.textContent = '🟢 SEGNALE DI TEST ATTIVO VERSO IL PC';
  scheduleNextFrame();
}}

function drawSourceWithRotationAndMirror(sourceElem, srcW, srcH) {{
  const outW = txCanvas.width;
  const outH = txCanvas.height;
  const rot = getEffectiveRotationDeg();
  const mirror = document.getElementById('mirrorSel').value === '1';

  txCtx.save();
  txCtx.translate(outW / 2, outH / 2);
  if (mirror) {{
    txCtx.scale(-1, 1);
  }}
  if (rot !== 0) {{
    txCtx.rotate((rot * Math.PI) / 180);
  }}
  const drawW = (rot === 90 || rot === 270) ? outH : outW;
  const drawH = (rot === 90 || rot === 270) ? outW : outH;
  txCtx.drawImage(sourceElem, -drawW / 2, -drawH / 2, drawW, drawH);
  txCtx.restore();

  // Only copy to the visible screen canvas if local preview is ON and Blackout Saver is OFF
  if (localPreviewEnabled && !blackoutActive) {{
    previewCtx.drawImage(txCanvas, 0, 0);
  }}
}}

function sendCurrentTxCanvasFrame() {{
  if (busySending) return;
  busySending = true;
  const w = txCanvas.width;
  const h = txCanvas.height;

  // Send compressed JPEG blob (~15x lighter on Wi-Fi bandwidth than raw RGBA!)
  if (txCanvas.toBlob) {{
    txCanvas.toBlob(async (blob) => {{
      if (!blob) {{
        busySending = false;
        return;
      }}
      try {{
        const res = await fetch('/frame?w=' + w + '&h=' + h + '&fmt=jpeg', {{
          method: 'POST',
          headers: {{ 'Content-Type': 'image/jpeg' }},
          body: blob
        }});
        if (res.ok) {{
          sentFrames++;
          if (!blackoutActive) {{
            badge.textContent = '🟢 LIVE (' + w + '×' + h + ' @ ' + getSelectedFps() + ' FPS) — ' + sentFrames + ' frame';
          }}
        }}
      }} catch (e) {{
        if (!blackoutActive) badge.textContent = '🔴 Riconnessione al PC...';
      }} finally {{
        busySending = false;
      }}
    }}, 'image/jpeg', 0.82);
  }} else {{
    busySending = false;
  }}
}}

function scheduleNextFrame() {{
  if (pumpTimerId) clearTimeout(pumpTimerId);
  const fps = getSelectedFps();
  const intervalMs = Math.max(16, Math.round(1000 / fps));
  pumpTimerId = setTimeout(pumpFrames, intervalMs);
}}

function pumpFrames() {{
  if (streamingMode === 'none') return;
  animTick++;

  if (streamingMode === 'camera' && video.readyState >= 2) {{
    if (video.videoWidth > 0 && video.videoHeight > 0 &&
        (video.videoWidth !== sensorNativeW || video.videoHeight !== sensorNativeH)) {{
      updateStreamDimensions();
    }}
    drawSourceWithRotationAndMirror(video, sensorNativeW, sensorNativeH);
    sendCurrentTxCanvasFrame();
  }} else if (streamingMode === 'photo' && loadedImage) {{
    drawSourceWithRotationAndMirror(loadedImage, sensorNativeW, sensorNativeH);
    sendCurrentTxCanvasFrame();
  }} else if (streamingMode === 'test') {{
    const w = txCanvas.width, h = txCanvas.height;
    txCtx.fillStyle = '#050505';
    txCtx.fillRect(0, 0, w, h);
    const cx = w / 2 + Math.cos(animTick * 0.08) * (w * 0.25);
    const cy = h / 2 + Math.sin(animTick * 0.06) * (h * 0.20);
    txCtx.fillStyle = '#10b981';
    txCtx.beginPath();
    txCtx.arc(cx, cy, 36, 0, Math.PI * 2);
    txCtx.fill();
    txCtx.fillStyle = '#d1fae5';
    txCtx.font = 'bold 18px sans-serif';
    txCtx.textAlign = 'center';
    txCtx.fillText('SMARTCAM OLED LIVE', w / 2, 38);
    txCtx.font = '14px sans-serif';
    txCtx.fillText('Frame #' + sentFrames + ' (' + w + '×' + h + ')', w / 2, h - 20);
    if (localPreviewEnabled && !blackoutActive) {{
      previewCtx.drawImage(txCanvas, 0, 0);
    }}
    sendCurrentTxCanvasFrame();
  }}

  scheduleNextFrame();
}}

// Initialize standby canvas & check HTTPS permission context
previewCtx.fillStyle = '#000000';
previewCtx.fillRect(0, 0, previewCanvas.width, previewCanvas.height);
previewCtx.strokeStyle = '#1f2937';
previewCtx.strokeRect(4, 4, previewCanvas.width - 8, previewCanvas.height - 8);
previewCtx.fillStyle = '#10b981';
previewCtx.font = 'bold 16px sans-serif';
previewCtx.textAlign = 'center';
previewCtx.fillText('SMARTCAM OLED CAMERA', previewCanvas.width / 2, previewCanvas.height / 2 - 10);
previewCtx.fillStyle = '#9ca3af';
previewCtx.font = '13px sans-serif';
previewCtx.fillText('Tocca "Richiedi Permesso & Avvia Fotocamera"', previewCanvas.width / 2, previewCanvas.height / 2 + 16);

checkHttpsRequirement();
</script>
</body>
</html>"##
    )
}

/// Returns the 5 built-in selectable SmartCam plugins:
/// 1. `phone_camera_source`: 📱 Fotocamera Smartphone (Wi-Fi / USB / IP Cam)
/// 2. `multi_person_tracker`: 👥 Tracking Persone Multiple (Multi-Volto & Busto)
/// 3. `webrtc_rtsp_source`: 🌐 Flusso Remoto WebRTC / RTSP (OBS / IP Cam)
/// 4. `studio_color_enhancer`: 🎨 Ottimizzatore Luce & Colore Studio (Auto-LUT & Denoise)
/// 5. `virtual_teleprompter_overlay`: 📜 Teleprompter / Overlay Note in Sovrimpressione (Solo Anteprima o Stream)
pub fn builtin_plugins() -> Vec<PluginMetadata> {
    let lan_ip = detect_local_lan_ip();
    let default_phone_url = format!("http://{}:{}", lan_ip, DEFAULT_PHONE_SERVER_PORT);
    vec![
        PluginMetadata {
            id: PLUGIN_ID_PHONE_CAMERA.to_string(),
            name: "📱 Fotocamera Smartphone (Wi-Fi / USB / IP Cam)".to_string(),
            version: "2.0.0".to_string(),
            author: "Rubens Rainelli".to_string(),
            description: "Server HTTP integrato + client MJPEG con porta configurabile e sblocco Firewall: apri l'URL mostrato a schermo sul browser del telefono per trasmettere la fotocamera live.".to_string(),
            category: PluginCategory::CameraInput,
            enabled: false,
            enabled_by_default: false,
            config_endpoint: default_phone_url,
            status_label: "Disattivato (Usa Webcam PC standard)".to_string(),
            is_external_dll: false,
            dll_path: None,
        },
        PluginMetadata {
            id: PLUGIN_ID_MULTI_PERSON.to_string(),
            name: "👥 Tracking Persone Multiple (Multi-Volto & Busto)".to_string(),
            version: "1.0.0".to_string(),
            author: "Rubens Rainelli".to_string(),
            description: "Rileva e mantiene nitide contemporaneamente fino a 4 persone nell'inquadratura, sfocando o sostituendo lo sfondo dietro a tutti i soggetti.".to_string(),
            category: PluginCategory::Segmentation,
            enabled: false,
            enabled_by_default: false,
            config_endpoint: "max_persons=4".to_string(),
            status_label: "Disattivato (Modalità Singolo Soggetto)".to_string(),
            is_external_dll: false,
            dll_path: None,
        },
        PluginMetadata {
            id: PLUGIN_ID_WEBRTC_RTSP.to_string(),
            name: "🌐 Flusso Remoto WebRTC / RTSP (OBS / IP Cam)".to_string(),
            version: "1.2.0".to_string(),
            author: "Rubens Rainelli".to_string(),
            description: "Riceve flussi video di rete HTTP/MJPEG o snapshot a bassa latenza da telecamere IP di rete o server LAN.".to_string(),
            category: PluginCategory::CameraInput,
            enabled: false,
            enabled_by_default: false,
            config_endpoint: "http://192.168.1.120:8080/shot.jpg".to_string(),
            status_label: "Disattivato".to_string(),
            is_external_dll: false,
            dll_path: None,
        },
        PluginMetadata {
            id: PLUGIN_ID_STUDIO_COLOR.to_string(),
            name: "🎨 Ottimizzatore Luce & Colore Studio (Auto-LUT & Denoise)".to_string(),
            version: "1.2.0".to_string(),
            author: "Rubens Rainelli".to_string(),
            description: "Auto-esposizione adattiva, protezione selettiva dell'incarnato, riduzione rumore nelle ombre e LUT calda da studio broadcast.".to_string(),
            category: PluginCategory::VideoFilter,
            enabled: false,
            enabled_by_default: false,
            config_endpoint: "warm_broadcast_lut".to_string(),
            status_label: "Disattivato".to_string(),
            is_external_dll: false,
            dll_path: None,
        },
        PluginMetadata {
            id: PLUGIN_ID_TELEPROMPTER.to_string(),
            name: "📜 Teleprompter / Overlay Note in Sovrimpressione (Solo Anteprima o Stream)".to_string(),
            version: "1.2.0".to_string(),
            author: "Rubens Rainelli".to_string(),
            description: "Mostra nella parte alta dell'inquadratura una barra guida semitrasparente con testo scorrevole leggibile per mantenere lo sguardo fisso verso l'obiettivo.".to_string(),
            category: PluginCategory::VideoFilter,
            enabled: false,
            enabled_by_default: false,
            config_endpoint: "GUARDA QUI VERSO L OBIETTIVO - SMARTCAM TELEPROMPTER ATTIVO".to_string(),
            status_label: "Disattivato".to_string(),
            is_external_dll: false,
            dll_path: None,
        },
    ]
}

/// Central manager for built-in and third-party dynamic plugins (`plugins/*.dll`).
pub struct PluginRegistry {
    plugins: Vec<PluginMetadata>,
    external_handles: Vec<ExternalDllPlugin>,
    phone_frame_counter: u64,
    phone_port: u16,
    phone_shared_frame: Arc<Mutex<PhoneFrameSharedState>>,
    phone_received_frames: Arc<AtomicU64>,
    phone_tcp_connections: Arc<AtomicU64>,
    phone_last_client_log: Arc<Mutex<String>>,
    phone_remote_pull_url: Arc<Mutex<String>>,
    phone_server: Option<PhoneCameraServer>,
    repo_root: PathBuf,
}

impl PluginRegistry {
    /// Initializes the plugin registry with built-in plugins, scans `<repo_root>/plugins/` for
    /// external `.dll` plugins, restores persisted user plugin states, and starts the built-in
    /// Smartphone HTTP server so the configured port is always ready on the LAN.
    pub fn new(repo_root: &Path) -> Self {
        let shared_frame = Arc::new(Mutex::new(PhoneFrameSharedState::default()));
        let received_frames = Arc::new(AtomicU64::new(0));
        let tcp_connections = Arc::new(AtomicU64::new(0));
        let last_client_log = Arc::new(Mutex::new(
            "In attesa di pacchetti dal telefono...".to_string(),
        ));
        let remote_pull_url = Arc::new(Mutex::new(String::new()));

        let mut registry = Self {
            plugins: builtin_plugins(),
            external_handles: Vec::new(),
            phone_frame_counter: 0,
            phone_port: DEFAULT_PHONE_SERVER_PORT,
            phone_shared_frame: shared_frame,
            phone_received_frames: received_frames,
            phone_tcp_connections: tcp_connections,
            phone_last_client_log: last_client_log,
            phone_remote_pull_url: remote_pull_url,
            phone_server: None,
            repo_root: repo_root.to_path_buf(),
        };

        let plugins_dir = repo_root.join("plugins");
        let _ = registry.scan_external_plugins_dir(&plugins_dir);
        let _ = registry.load_state(repo_root);
        registry.ensure_phone_server_started();
        registry.refresh_status_labels();
        registry
    }

    fn ensure_phone_server_started(&mut self) {
        let current_endpoint = self.phone_camera_endpoint();
        if let Ok(mut g) = self.phone_remote_pull_url.lock() {
            *g = current_endpoint;
        }

        let need_restart = match &self.phone_server {
            Some(srv) => {
                !srv.is_running.load(Ordering::Relaxed)
                    || srv.bound_port.load(Ordering::Relaxed) != self.phone_port
            }
            None => true,
        };

        if need_restart {
            if let Some(mut old_srv) = self.phone_server.take() {
                old_srv.stop();
            }
            self.phone_server = Some(PhoneCameraServer::new(
                self.phone_port,
                Arc::clone(&self.phone_shared_frame),
                Arc::clone(&self.phone_received_frames),
                Arc::clone(&self.phone_tcp_connections),
                Arc::clone(&self.phone_last_client_log),
                Arc::clone(&self.phone_remote_pull_url),
            ));
        }
    }

    /// Returns the configured TCP port for the built-in Smartphone Camera HTTP Server (default `8080`).
    pub fn phone_server_port(&self) -> u16 {
        self.phone_port
    }

    /// Sets a custom TCP port (`1024..=65535`) for the Smartphone Camera HTTP Server, immediately
    /// restarts the listener on the new port, updates the pairing URL, and returns the active port.
    pub fn set_phone_server_port(&mut self, port: u16) -> u16 {
        let clamped = port.clamp(1024, 65535);
        self.phone_port = clamped;
        let new_url = format!("http://{}:{}", self.local_lan_ip(), clamped);

        for p in &mut self.plugins {
            if p.id == PLUGIN_ID_PHONE_CAMERA {
                p.config_endpoint = new_url.clone();
                break;
            }
        }
        if let Ok(mut g) = self.phone_remote_pull_url.lock() {
            *g = new_url;
        }

        self.ensure_phone_server_started();
        self.refresh_status_labels();
        let _ = self.save_state(&self.repo_root.clone());
        self.phone_port
    }

    /// Detects and returns the PC's primary LAN IPv4 address (e.g. `"192.168.1.55"`).
    pub fn local_lan_ip(&self) -> String {
        if let Some(direct_ip) = detect_direct_hotspot_or_usb_ip() {
            return direct_ip;
        }
        detect_local_lan_ip()
    }

    /// Returns the full HTTP URL (`http://<LAN_IP>:<PORT>`) to open on the smartphone browser.
    pub fn phone_server_url(&self) -> String {
        format!(
            "http://{}:{}",
            self.local_lan_ip(),
            self.phone_server_port()
        )
    }

    /// Returns `true` if the built-in Smartphone HTTP Server is actively listening on `phone_server_port()`.
    pub fn is_phone_server_running(&self) -> bool {
        self.phone_server
            .as_ref()
            .map(|s| s.is_running.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Returns the total number of video frames received from the smartphone since startup.
    pub fn phone_connected_frames(&self) -> u64 {
        self.phone_received_frames.load(Ordering::Relaxed)
    }

    /// Returns the total number of TCP connections accepted by the Smartphone HTTP Server.
    pub fn phone_tcp_connections(&self) -> u64 {
        self.phone_tcp_connections.load(Ordering::Relaxed)
    }

    /// Returns the latest live connection diagnostic log from the Smartphone HTTP Server.
    pub fn phone_last_client_log(&self) -> String {
        self.phone_last_client_log
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "Pronto".to_string())
    }

    /// Returns the parsed network endpoint for the Smartphone Camera plugin (`phone_camera_source`).
    pub fn parsed_phone_endpoint(&self) -> ParsedPhoneEndpoint {
        ParsedPhoneEndpoint::parse(&self.phone_camera_endpoint())
    }

    /// Returns the current connection state (`Disconnected`, `PairingStandby`, `StreamingActive`)
    /// for the Smartphone Camera plugin (`phone_camera_source`), ported from `PhoneCameraPlugin::GetConnectionState`.
    pub fn phone_connection_state(&self) -> PhoneConnectionState {
        if !self.is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA) {
            return PhoneConnectionState::Disconnected;
        }
        if let Ok(guard) = self.phone_shared_frame.lock() {
            if let Some(ts) = guard.last_received {
                if ts.elapsed() < Duration::from_secs(10)
                    && guard.width > 0
                    && guard.height > 0
                    && !guard.bgra.is_empty()
                {
                    return PhoneConnectionState::StreamingActive;
                }
            }
        }
        PhoneConnectionState::PairingStandby
    }

    /// Pushes a decoded `BGRA32` smartphone frame into the plugin's shared frame buffer
    /// and switches connection state to `StreamingActive`, ported from `PhoneCameraPlugin::PushDecodedPhoneFrame`.
    pub fn push_decoded_phone_frame(
        &mut self,
        bgra_data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> bool {
        let row_bytes = (width as usize) * 4;
        if width == 0
            || height == 0
            || (stride as usize) < row_bytes
            || bgra_data.len() < (stride as usize) * (height as usize - 1) + row_bytes
        {
            return false;
        }
        if let Ok(mut guard) = self.phone_shared_frame.lock() {
            let needed = row_bytes * (height as usize);
            if guard.bgra.len() != needed {
                guard.bgra.resize(needed, 255);
            }
            for y in 0..(height as usize) {
                let src_off = y * (stride as usize);
                let dst_off = y * row_bytes;
                guard.bgra[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&bgra_data[src_off..src_off + row_bytes]);
            }
            guard.width = width;
            guard.height = height;
            guard.last_received = Some(Instant::now());
            guard.source_tag = "Direct Push BGRA32".to_string();
        } else {
            return false;
        }
        self.phone_received_frames.fetch_add(1, Ordering::Relaxed);
        self.refresh_status_labels();
        true
    }

    /// Requests Windows Firewall UAC elevation (`ShellExecuteW` `"runas"` on `powershell.exe`) to:
    /// 1. Convert any `Public` Wi-Fi profile (such as `TIM-*`) to `Private` so Windows 11 allows LAN devices.
    /// 2. Open `phone_server_port()` on all profiles (`Private,Public,Domain`) with `edge=yes`.
    pub fn unlock_windows_firewall_for_phone(&mut self) -> Result<String, String> {
        let port = self.phone_server_port();
        self.ensure_phone_server_started();

        let ps_cmd = format!(
            "-NoProfile -WindowStyle Hidden -Command \"Get-NetConnectionProfile | Where-Object {{ $_.NetworkCategory -eq 'Public' }} | Set-NetConnectionProfile -NetworkCategory Private -ErrorAction SilentlyContinue; netsh advfirewall firewall delete rule name='SmartCam Phone Camera Server (TCP {0})' >$null 2>&1; netsh advfirewall firewall add rule name='SmartCam Phone Camera Server (TCP {0})' dir=in action=allow protocol=TCP localport={0} profile=any edge=yes\"",
            port
        );

        unsafe {
            let op_w = to_wstring("runas");
            let file_w = to_wstring("powershell.exe");
            let args_w = to_wstring(&ps_cmd);
            let hinst = ShellExecuteW(
                std::ptr::null_mut(),
                op_w.as_ptr(),
                file_w.as_ptr(),
                args_w.as_ptr(),
                std::ptr::null(),
                SW_HIDE,
            );
            if (hinst as usize) > 32 {
                self.refresh_status_labels();
                Ok(format!(
                    "Firewall TCP {} sbloccato e Rete Wi-Fi impostata su Privata! Apri {} sul telefono.",
                    port,
                    self.phone_server_url()
                ))
            } else {
                Err(format!(
                    "Autorizzazione UAC Firewall annullata o non riuscita (codice {}).",
                    hinst as usize
                ))
            }
        }
    }

    /// Activates a direct USB Bridge (`adb reverse` or USB Tethering IP `192.168.42.x`) or opens
    /// Windows Mobile Hotspot (`192.168.137.1`) so the smartphone connects directly to the PC
    /// even when the home router (e.g. TIM Wi-Fi) blocks Wi-Fi client-to-client communication.
    pub fn activate_phone_usb_or_hotspot_bridge(&mut self) -> String {
        let port = self.phone_server_port();
        self.ensure_phone_server_started();

        // 1. Try `adb reverse tcp:<port> tcp:<port>` if Android ADB is available over USB
        if let Ok(out) = std::process::Command::new("adb")
            .args([
                "reverse",
                &format!("tcp:{}", port),
                &format!("tcp:{}", port),
            ])
            .output()
        {
            if out.status.success() {
                let usb_url = format!("http://127.0.0.1:{}", port);
                let _ = self.set_plugin_endpoint(PLUGIN_ID_PHONE_CAMERA, &usb_url);
                if let Ok(mut lg) = self.phone_last_client_log.lock() {
                    *lg = format!("🔌 Ponte USB ADB attivo → Apri {} sul telefono", usb_url);
                }
                return format!(
                    "🔌 Ponte USB ADB attivato! Apri {} sul browser del telefono collegato via cavo USB.",
                    usb_url
                );
            }
        }

        // 2. Check if USB Tethering (`192.168.42.x` / `172.20.10.x`) or Windows Mobile Hotspot (`192.168.137.1`) is active
        if let Some(direct_ip) = detect_direct_hotspot_or_usb_ip() {
            let direct_url = format!("http://{}:{}", direct_ip, port);
            let _ = self.set_plugin_endpoint(PLUGIN_ID_PHONE_CAMERA, &direct_url);
            if let Ok(mut lg) = self.phone_last_client_log.lock() {
                *lg = format!("📶 Ponte Diretto attivo su {}", direct_url);
            }
            return format!(
                "📶 Connessione diretta rilevata! Apri {} sul telefono (aggira l'isolamento del modem).",
                direct_url
            );
        }

        // 3. Open Windows 11 Mobile Hotspot settings so the user can toggle Hotspot ON with 1 click
        unsafe {
            let op_w = to_wstring("open");
            let uri_w = to_wstring("ms-settings:network-mobilehotspot");
            let _ = ShellExecuteW(
                std::ptr::null_mut(),
                op_w.as_ptr(),
                uri_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            );
        }
        if let Ok(mut lg) = self.phone_last_client_log.lock() {
            *lg = format!(
                "📶 Attiva Hotspot Mobile Windows o Tethering USB → poi apri http://192.168.137.1:{}",
                port
            );
        }
        format!(
            "📶 Impostazioni Hotspot Windows aperte: attivalo e connetti il telefono (URL: http://192.168.137.1:{}) oppure collega il cavo con Tethering USB!",
            port
        )
    }

    /// Scans the `plugins/` directory for external `.dll` libraries exporting `smartcam_plugin_query_v1`.
    pub fn scan_external_plugins_dir(&mut self, plugins_dir: &Path) -> usize {
        if !plugins_dir.exists() || !plugins_dir.is_dir() {
            return 0;
        }

        let entries = match fs::read_dir(plugins_dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        let mut loaded_count = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_dll = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("dll"))
                .unwrap_or(false);
            if !is_dll {
                continue;
            }

            if self.try_load_external_dll(&path) {
                loaded_count += 1;
            }
        }

        self.refresh_status_labels();
        loaded_count
    }

    fn try_load_external_dll(&mut self, dll_path: &Path) -> bool {
        use std::os::windows::ffi::OsStrExt;
        let wide_path: Vec<u16> = dll_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let module = LoadLibraryW(wide_path.as_ptr());
            if module.is_null() {
                return false;
            }

            let proc_addr = GetProcAddress(module, SMARTCAM_PLUGIN_QUERY_SYMBOL.as_ptr());
            if proc_addr.is_null() {
                FreeLibrary(module);
                return false;
            }

            let query_fn: SmartCamPluginQueryFn = std::mem::transmute(proc_addr);
            let desc_ptr = query_fn();
            if desc_ptr.is_null() {
                FreeLibrary(module);
                return false;
            }

            let desc = &*desc_ptr;
            if desc.abi_version != SMARTCAM_PLUGIN_ABI_VERSION_V1 {
                FreeLibrary(module);
                return false;
            }

            let fallback_id = dll_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("external_plugin");
            let id = cstr_to_string(desc.id, fallback_id);
            if self.plugins.iter().any(|p| p.id == id) {
                FreeLibrary(module);
                return false;
            }

            let name = cstr_to_string(desc.name, &id);
            let version = cstr_to_string(desc.version, "1.0.0");
            let author = cstr_to_string(desc.author, "Third-Party Developer");
            let description = cstr_to_string(desc.description, "Plugin esterno SmartCam (.dll)");
            let category_str = cstr_to_string(desc.category, "video_filter");
            let category = PluginCategory::from_str(&category_str);
            let enabled_by_default = desc.enabled_by_default != 0;

            let instance = if let Some(init_fn) = desc.init {
                init_fn()
            } else {
                std::ptr::null_mut()
            };

            self.plugins.push(PluginMetadata {
                id: id.clone(),
                name,
                version,
                author,
                description,
                category,
                enabled: enabled_by_default,
                enabled_by_default,
                config_endpoint: String::new(),
                status_label: if enabled_by_default {
                    "Attivo (DLL Esterna)".to_string()
                } else {
                    "Disattivato (DLL Esterna)".to_string()
                },
                is_external_dll: true,
                dll_path: Some(dll_path.to_path_buf()),
            });

            self.external_handles.push(ExternalDllPlugin {
                id,
                module_handle: module,
                descriptor: desc_ptr,
                instance,
                is_opened: false,
            });

            true
        }
    }

    /// Returns an immutable slice of all registered plugins (built-in + external DLLs).
    pub fn plugins(&self) -> &[PluginMetadata] {
        &self.plugins
    }

    /// Returns a mutable slice of all registered plugins.
    pub fn plugins_mut(&mut self) -> &mut [PluginMetadata] {
        &mut self.plugins
    }

    /// Looks up a plugin by its unique ID (or short alias).
    pub fn get_plugin(&self, id: &str) -> Option<&PluginMetadata> {
        let cid = normalize_plugin_id(id);
        self.plugins.iter().find(|p| p.id == cid)
    }

    /// Returns true if the given plugin ID (or short alias) is currently enabled by the user.
    pub fn is_plugin_enabled(&self, id: &str) -> bool {
        let cid = normalize_plugin_id(id);
        self.plugins
            .iter()
            .find(|p| p.id == cid)
            .map(|p| p.enabled)
            .unwrap_or(false)
    }

    /// Toggles the enabled state of a plugin by ID and returns the new state (`Some(true/false)`).
    /// If a `CameraInput` plugin is enabled, any other `CameraInput` plugin is automatically disabled
    /// so there is never an input conflict.
    pub fn toggle_plugin(&mut self, id: &str) -> Option<bool> {
        let cid = normalize_plugin_id(id);
        let idx = self.plugins.iter().position(|p| p.id == cid)?;
        let new_state = !self.plugins[idx].enabled;
        self.set_plugin_enabled(&cid, new_state);
        Some(new_state)
    }

    /// Explicitly sets the `enabled` state of a plugin by ID.
    pub fn set_plugin_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let cid = normalize_plugin_id(id);
        let idx = match self.plugins.iter().position(|p| p.id == cid) {
            Some(i) => i,
            None => return false,
        };

        let category = self.plugins[idx].category;
        if enabled && category == PluginCategory::CameraInput {
            for (i, p) in self.plugins.iter_mut().enumerate() {
                if i != idx && p.category == PluginCategory::CameraInput {
                    p.enabled = false;
                }
            }
        }

        self.plugins[idx].enabled = enabled;
        if cid == PLUGIN_ID_PHONE_CAMERA && enabled {
            self.ensure_phone_server_started();
        }
        self.sync_external_plugin_state(&cid, enabled);
        self.refresh_status_labels();
        true
    }

    fn sync_external_plugin_state(&mut self, id: &str, enabled: bool) {
        let endpoint = self
            .plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.config_endpoint.clone())
            .unwrap_or_default();

        for ext in &mut self.external_handles {
            if ext.id == id && !ext.descriptor.is_null() {
                unsafe {
                    let desc = &*ext.descriptor;
                    if enabled && !ext.is_opened {
                        if let Some(open_fn) = desc.open {
                            let c_uri = CString::new(endpoint.as_str()).unwrap_or_default();
                            ext.is_opened =
                                open_fn(ext.instance, 1280, 720, 30, c_uri.as_ptr()) != 0;
                        } else {
                            ext.is_opened = true;
                        }
                    } else if !enabled && ext.is_opened {
                        if let Some(close_fn) = desc.close {
                            close_fn(ext.instance);
                        }
                        ext.is_opened = false;
                    }
                }
            }
        }
    }

    /// Replaces the set of enabled plugins from a list of IDs.
    pub fn set_enabled_plugin_ids(&mut self, ids: &[String]) {
        let normalized: Vec<String> = ids.iter().map(|s| normalize_plugin_id(s)).collect();
        for p in &mut self.plugins {
            p.enabled = normalized.iter().any(|item| item == &p.id);
        }
        if self.is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA) {
            self.ensure_phone_server_started();
        }
        self.refresh_status_labels();
    }

    /// Returns the list of currently enabled plugin IDs.
    pub fn enabled_plugin_ids(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.id.clone())
            .collect()
    }

    /// Returns the number of currently active plugins.
    pub fn enabled_count(&self) -> usize {
        self.plugins.iter().filter(|p| p.enabled).count()
    }

    /// Configures the URL/IP endpoint (or TCP port) for the Smartphone Camera plugin (`phone_camera_source`).
    pub fn set_phone_camera_endpoint(&mut self, endpoint: &str) -> bool {
        let trimmed = endpoint.trim();
        if let Ok(port_only) = trimmed.trim_start_matches(':').parse::<u16>() {
            self.set_phone_server_port(port_only);
            return true;
        }
        if let Some((host, port, _)) = parse_http_endpoint(trimmed) {
            let lan_ip = self.local_lan_ip();
            if (host == lan_ip || host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost"))
                && port != self.phone_port
            {
                self.set_phone_server_port(port);
            }
        }
        let ok = self.set_plugin_endpoint(PLUGIN_ID_PHONE_CAMERA, trimmed);
        if let Ok(mut g) = self.phone_remote_pull_url.lock() {
            *g = trimmed.to_string();
        }
        ok
    }

    /// Returns the configured URL/IP endpoint for the Smartphone Camera plugin.
    pub fn phone_camera_endpoint(&self) -> String {
        self.get_plugin(PLUGIN_ID_PHONE_CAMERA)
            .map(|p| p.config_endpoint.clone())
            .unwrap_or_else(|| self.phone_server_url())
    }

    /// Updates the configuration endpoint or parameter string of any plugin by ID.
    pub fn set_plugin_endpoint(&mut self, id: &str, endpoint: &str) -> bool {
        let cid = normalize_plugin_id(id);
        let clean = endpoint.trim();
        if clean.is_empty() {
            return false;
        }
        let mut found = false;
        for p in &mut self.plugins {
            if p.id == cid {
                p.config_endpoint = clean.to_string();
                found = true;
                break;
            }
        }
        if found {
            if cid == PLUGIN_ID_PHONE_CAMERA {
                if let Ok(mut g) = self.phone_remote_pull_url.lock() {
                    *g = clean.to_string();
                }
            }
            for ext in &mut self.external_handles {
                if ext.id == cid && !ext.descriptor.is_null() {
                    unsafe {
                        let desc = &*ext.descriptor;
                        if let Some(set_param) = desc.set_parameter {
                            let k = CString::new("endpoint_uri").unwrap_or_default();
                            let v = CString::new(clean).unwrap_or_default();
                            set_param(ext.instance, k.as_ptr(), v.as_ptr());
                        }
                    }
                }
            }
            self.refresh_status_labels();
        }
        found
    }

    /// Updates human-readable Italian status labels for all plugins.
    pub fn refresh_status_labels(&mut self) {
        let lan_url = self.phone_server_url();
        let srv_running = self.is_phone_server_running();
        let rx_frames = self.phone_connected_frames();
        let port = self.phone_port;

        for p in &mut self.plugins {
            if p.id == PLUGIN_ID_PHONE_CAMERA {
                if p.enabled {
                    let srv_tag = if rx_frames > 0 {
                        format!("Connesso: {} frame ricevuti", rx_frames)
                    } else if srv_running {
                        format!("Server HTTP attivo su {}", lan_url)
                    } else {
                        format!("Porta TCP {} occupata", port)
                    };
                    p.status_label = format!("🟢 ATTIVO ({}) → Apri {}", srv_tag, lan_url);
                } else {
                    p.status_label = format!(
                        "⚪ Disattivato (Server pronto su {} — Porta {})",
                        lan_url, port
                    );
                }
            } else if p.id == PLUGIN_ID_MULTI_PERSON {
                if p.enabled {
                    p.status_label =
                        "🟢 ATTIVO (Rilevamento fino a 4 persone contemporanee)".to_string();
                } else {
                    p.status_label = "⚪ Disattivato (Modalità Singolo Soggetto)".to_string();
                }
            } else if p.id == PLUGIN_ID_WEBRTC_RTSP {
                if p.enabled {
                    p.status_label = format!("🟢 ATTIVO → {}", p.config_endpoint);
                } else {
                    p.status_label = "⚪ Disattivato".to_string();
                }
            } else if p.enabled {
                p.status_label = if p.is_external_dll {
                    "🟢 ATTIVO (Plugin DLL Esterno)".to_string()
                } else {
                    "🟢 ATTIVO nella pipeline live".to_string()
                };
            } else {
                p.status_label = if p.is_external_dll {
                    "⚪ Disattivato (Plugin DLL Esterno)".to_string()
                } else {
                    "⚪ Disattivato".to_string()
                };
            }
        }
    }

    /// Saves the active plugin state, endpoints, and `phone_server_port` to `config/plugins_state.conf`.
    pub fn save_state(&self, repo_root: &Path) -> bool {
        let cfg_dir = repo_root.join("config");
        let _ = fs::create_dir_all(&cfg_dir);
        let state_file = cfg_dir.join("plugins_state.conf");

        let mut content = format!(
            "# SmartCam Plugin Registry State (PEFL v1.0)\n\
             # Formato: plugin_id|enabled(0/1)|config_endpoint\n\
             phone_server_port|1|{}\n",
            self.phone_port
        );
        for p in &self.plugins {
            let en = if p.enabled { "1" } else { "0" };
            content.push_str(&format!("{}|{}|{}\n", p.id, en, p.config_endpoint));
        }
        fs::write(state_file, content).is_ok()
    }

    /// Loads persisted plugin activation states, endpoints, and `phone_server_port` from `config/plugins_state.conf`.
    pub fn load_state(&mut self, repo_root: &Path) -> bool {
        let state_file = repo_root.join("config").join("plugins_state.conf");
        let raw = match fs::read_to_string(state_file) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let lan_ip = self.local_lan_ip();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.splitn(3, '|').collect();
            if parts.len() >= 2 {
                let id = parts[0].trim();
                if id == "phone_server_port" && parts.len() == 3 {
                    if let Ok(port) = parts[2].trim().parse::<u16>() {
                        self.phone_port = port.clamp(1024, 65535);
                    }
                    continue;
                }
                let en = parts[1].trim() == "1" || parts[1].trim().eq_ignore_ascii_case("true");
                if let Some(p) = self.plugins.iter_mut().find(|item| item.id == id) {
                    p.enabled = en;
                    if parts.len() == 3 && !parts[2].trim().is_empty() {
                        let saved_ep = parts[2].trim();
                        // Replace old placeholder 192.168.1.100:8080/video with real LAN server URL
                        if id == PLUGIN_ID_PHONE_CAMERA
                            && saved_ep == "http://192.168.1.100:8080/video"
                        {
                            p.config_endpoint = format!("http://{}:{}", lan_ip, self.phone_port);
                        } else {
                            p.config_endpoint = saved_ep.to_string();
                        }
                    }
                }
            }
        }

        // Sync phone_camera_source endpoint port if pointing to local LAN IP
        let current_lan_url = format!("http://{}:{}", lan_ip, self.phone_port);
        for p in &mut self.plugins {
            if p.id == PLUGIN_ID_PHONE_CAMERA
                && p.config_endpoint
                    .starts_with(&format!("http://{}:", lan_ip))
            {
                p.config_endpoint = current_lan_url.clone();
            }
        }

        self.refresh_status_labels();
        true
    }

    /// Returns a concise Italian summary string of active plugins for the GUI header/footer.
    pub fn summary_line_it(&self) -> String {
        let active: Vec<&str> = self
            .plugins
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.name.as_str())
            .collect();
        if active.is_empty() {
            "Nessun plugin attivo (Clicca per attivare 📱 Fotocamera Smartphone, 👥 Multi-Persona o Filtri Studio)"
                .to_string()
        } else {
            format!("{} plugin attivi: {}", active.len(), active.join(" • "))
        }
    }

    /// If a `CameraInput` plugin (such as `phone_camera_source`, `webrtc_rtsp_source`, or an external camera DLL) is enabled,
    /// produces the next `BGRA32` frame at its true native dimensions `(actual_w, actual_h)` (never stretching/squishing!)
    /// and returns `Some((actual_w, actual_h))`.
    pub fn read_active_camera_plugin_frame(
        &mut self,
        out_bgra: &mut Vec<u8>,
        fallback_w: u32,
        fallback_h: u32,
    ) -> Option<(u32, u32)> {
        let width = fallback_w.max(64);
        let height = fallback_h.max(64);
        let needed_fallback = (width as usize) * (height as usize) * 4;
        if out_bgra.len() < needed_fallback {
            out_bgra.resize(needed_fallback, 255);
        }

        // 1. Built-in Smartphone Camera Plugin (`phone_camera_source`)
        if self.is_plugin_enabled(PLUGIN_ID_PHONE_CAMERA) {
            self.ensure_phone_server_started();
            self.phone_frame_counter = self.phone_frame_counter.wrapping_add(1);

            // Check if we have a real frame received from the phone within the last 10 seconds
            if let Ok(guard) = self.phone_shared_frame.lock() {
                if let Some(ts) = guard.last_received {
                    let fw = guard.width;
                    let fh = guard.height;
                    let frame_bytes = (fw as usize) * (fh as usize) * 4;
                    if ts.elapsed() < Duration::from_secs(10)
                        && fw > 0
                        && fh > 0
                        && guard.bgra.len() == frame_bytes
                    {
                        if out_bgra.len() < frame_bytes {
                            out_bgra.resize(frame_bytes, 255);
                        }
                        out_bgra[..frame_bytes].copy_from_slice(&guard.bgra[..frame_bytes]);
                        return Some((fw, fh));
                    }
                }
            }

            let server_url = self.phone_server_url().to_ascii_uppercase();
            let endpoint = self.phone_camera_endpoint();
            let is_usb = endpoint.contains("127.0.0.1")
                || endpoint.contains("localhost")
                || endpoint.starts_with("usb://");
            render_smartphone_pairing_frame_bgra(
                &mut out_bgra[..needed_fallback],
                width,
                height,
                self.phone_frame_counter,
                is_usb,
                "SMARTCAM TELEFONO SERVER ATTIVO",
                "APRI SUL BROWSER DEL TELEFONO:",
                &server_url,
                "SE NON CARICA CLICCA SBLOCCA FIREWALL",
            );
            return Some((width, height));
        }

        // 2. Built-in WebRTC / RTSP / HTTP Stream Plugin (`webrtc_rtsp_source`)
        if self.is_plugin_enabled(PLUGIN_ID_WEBRTC_RTSP) {
            self.phone_frame_counter = self.phone_frame_counter.wrapping_add(1);
            let ep = self
                .get_plugin(PLUGIN_ID_WEBRTC_RTSP)
                .map(|p| p.config_endpoint.clone())
                .unwrap_or_else(|| "http://192.168.1.120:8080/shot.jpg".to_string());

            if let Ok(mut g) = self.phone_remote_pull_url.lock() {
                *g = ep.clone();
            }

            if let Ok(guard) = self.phone_shared_frame.lock() {
                if let Some(ts) = guard.last_received {
                    let fw = guard.width;
                    let fh = guard.height;
                    let frame_bytes = (fw as usize) * (fh as usize) * 4;
                    if ts.elapsed() < Duration::from_secs(5)
                        && fw > 0
                        && fh > 0
                        && guard.bgra.len() == frame_bytes
                    {
                        if out_bgra.len() < frame_bytes {
                            out_bgra.resize(frame_bytes, 255);
                        }
                        out_bgra[..frame_bytes].copy_from_slice(&guard.bgra[..frame_bytes]);
                        return Some((fw, fh));
                    }
                }
            }

            let ep_upper = ep.to_ascii_uppercase();
            render_smartphone_pairing_frame_bgra(
                &mut out_bgra[..needed_fallback],
                width,
                height,
                self.phone_frame_counter,
                false,
                "FLUSSO REMOTO WEBRTC / RTSP / HTTP",
                "IN CONFRONTO / ASCOLTO SU ENDPOINT:",
                &ep_upper,
                "INSERISCI URL MJPEG O SNAPSHOT IP CAM",
            );
            return Some((width, height));
        }

        // 3. External `CameraInput` DLL plugins
        let active_ext_id = self
            .plugins
            .iter()
            .find(|p| p.enabled && p.is_external_dll && p.category == PluginCategory::CameraInput)
            .map(|p| p.id.clone());

        if let Some(ext_id) = active_ext_id {
            for ext in &mut self.external_handles {
                if ext.id == ext_id && !ext.descriptor.is_null() {
                    unsafe {
                        let desc = &*ext.descriptor;
                        if let Some(read_fn) = desc.read_frame {
                            let ok = read_fn(
                                ext.instance,
                                out_bgra.as_mut_ptr(),
                                width,
                                height,
                                width * 4,
                                33,
                            );
                            if ok != 0 {
                                return Some((width, height));
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Executes all enabled `VideoFilter` and `Segmentation` plugins in-place on `inout_bgra` and `inout_mask`.
    pub fn apply_active_video_filter_plugins(
        &mut self,
        inout_bgra: &mut [u8],
        mut inout_mask: Option<&mut [u8]>,
        width: u32,
        height: u32,
        is_preview_pass: bool,
    ) {
        let needed = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || inout_bgra.len() < needed {
            return;
        }

        // 1. Built-in `multi_person_tracker`: Multi-Person Foreground Mask Refinement (up to 4 people)
        if self.is_plugin_enabled(PLUGIN_ID_MULTI_PERSON) {
            if let Some(mask) = inout_mask.as_deref_mut() {
                if mask.len() >= (width as usize) * (height as usize) {
                    apply_multi_person_mask_refinement(inout_bgra, mask, width, height);
                }
            }
        }

        // 2. Built-in `studio_color_enhancer`: Auto-Exposure Leveling, Skin-Tone Protection & Shadow Denoise
        if self.is_plugin_enabled(PLUGIN_ID_STUDIO_COLOR) {
            apply_studio_color_enhancer_bgra(inout_bgra, width, height);
        }

        // 3. Built-in `virtual_teleprompter_overlay`: Studio Eye-Contact Teleprompter with Readable Scrolling Text
        if self.is_plugin_enabled(PLUGIN_ID_TELEPROMPTER) {
            let note_cfg = self
                .get_plugin(PLUGIN_ID_TELEPROMPTER)
                .map(|p| p.config_endpoint.clone())
                .unwrap_or_default();
            let also_on_stream = note_cfg.to_ascii_lowercase().contains("stream");
            if is_preview_pass || also_on_stream {
                self.phone_frame_counter = self.phone_frame_counter.wrapping_add(1);
                apply_teleprompter_overlay_bgra(
                    inout_bgra,
                    width,
                    height,
                    &note_cfg,
                    self.phone_frame_counter,
                );
            }
        }

        // 4. External `.dll` VideoFilter / Segmentation plugins
        let enabled_ext_ids: Vec<String> = self
            .plugins
            .iter()
            .filter(|p| {
                p.enabled
                    && p.is_external_dll
                    && (p.category == PluginCategory::VideoFilter
                        || p.category == PluginCategory::Segmentation)
            })
            .map(|p| p.id.clone())
            .collect();

        for ext_id in enabled_ext_ids {
            for ext in &mut self.external_handles {
                if ext.id == ext_id && !ext.descriptor.is_null() {
                    unsafe {
                        let desc = &*ext.descriptor;
                        if let Some(process_fn) = desc.process_frame {
                            let mask_ptr = match inout_mask.as_deref_mut() {
                                Some(m) if m.len() >= (width as usize) * (height as usize) => {
                                    m.as_mut_ptr()
                                }
                                _ => std::ptr::null_mut(),
                            };
                            let _ = process_fn(
                                ext.instance,
                                inout_bgra.as_mut_ptr(),
                                mask_ptr,
                                width,
                                height,
                                width * 4,
                            );
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Image Scaling & Crisp 5x7 Bitmap Font Renderer for Video Overlays
// ============================================================================

fn scale_bgra_nearest(src: &[u8], src_w: u32, src_h: u32, dst: &mut [u8], dst_w: u32, dst_h: u32) {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return;
    }
    if src_w == dst_w && src_h == dst_h && src.len() >= dst.len() {
        dst.copy_from_slice(&src[..dst.len()]);
        return;
    }
    let src_stride = (src_w as usize) * 4;
    let dst_stride = (dst_w as usize) * 4;
    for y in 0..dst_h {
        let sy = ((y as u64 * src_h as u64) / (dst_h as u64)).min((src_h - 1) as u64) as usize;
        let src_row = sy * src_stride;
        let dst_row = (y as usize) * dst_stride;
        for x in 0..dst_w {
            let sx = ((x as u64 * src_w as u64) / (dst_w as u64)).min((src_w - 1) as u64) as usize;
            let s_idx = src_row + sx * 4;
            let d_idx = dst_row + (x as usize) * 4;
            dst[d_idx] = src[s_idx];
            dst[d_idx + 1] = src[s_idx + 1];
            dst[d_idx + 2] = src[s_idx + 2];
            dst[d_idx + 3] = 255;
        }
    }
}

/// Returns a 5x7 bitmap glyph (7 rows of 5-bit masks) for uppercase ASCII characters, digits, and URL punctuation.
fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        '/' => [
            0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        _ => [0b00000; 7],
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_bitmap_text_bgra(
    buf: &mut [u8],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    text: &str,
    scale: u32,
    b: u8,
    g: u8,
    r: u8,
) {
    let sc = scale.max(1);
    let char_advance = 6 * sc;
    let mut cur_x = x;
    for ch in text.chars() {
        if cur_x + 5 * sc >= width {
            break;
        }
        let rows = glyph_5x7(ch);
        for (ry, row_bits) in rows.iter().enumerate() {
            for rx in 0..5u32 {
                if (row_bits & (1 << (4 - rx))) != 0 {
                    fill_rect_bgra(
                        buf,
                        width,
                        height,
                        cur_x + rx * sc,
                        y + (ry as u32) * sc,
                        sc,
                        sc,
                        b,
                        g,
                        r,
                    );
                }
            }
        }
        cur_x += char_advance;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_centered_text_bgra(
    buf: &mut [u8],
    width: u32,
    height: u32,
    center_x: u32,
    y: u32,
    text: &str,
    scale: u32,
    b: u8,
    g: u8,
    r: u8,
) {
    let sc = scale.max(1);
    let text_w = (text.chars().count() as u32) * 6 * sc;
    let start_x = center_x.saturating_sub(text_w / 2);
    draw_bitmap_text_bgra(buf, width, height, start_x, y, text, sc, b, g, r);
}

/// Renders the HD Smartphone Pairing / Standby Card in `BGRA32` with crisp 5x7 bitmap instructions
/// showing the exact `http://<IP_LAN>:<PORT>` URL to open on the smartphone.
#[allow(clippy::too_many_arguments)]
fn render_smartphone_pairing_frame_bgra(
    out_bgra: &mut [u8],
    width: u32,
    height: u32,
    frame_counter: u64,
    is_usb_mode: bool,
    title_line: &str,
    subtitle_line: &str,
    url_line: &str,
    footer_line: &str,
) {
    let stride = (width as usize) * 4;
    for y in 0..height {
        let row_offset = (y as usize) * stride;
        let grad_b = (28 + (y * 30) / height.max(1)) as u8;
        let grad_g = (18 + (y * 22) / height.max(1)) as u8;
        let grad_r = (10 + (y * 12) / height.max(1)) as u8;
        for x in 0..width {
            let idx = row_offset + (x as usize) * 4;
            out_bgra[idx] = grad_b;
            out_bgra[idx + 1] = grad_g;
            out_bgra[idx + 2] = grad_r;
            out_bgra[idx + 3] = 255;
        }
    }

    let card_w = ((width * 4) / 5).max(260).min(width.saturating_sub(16));
    let card_h = ((height * 3) / 4).max(160).min(height.saturating_sub(16));
    let card_x = (width.saturating_sub(card_w)) / 2;
    let card_y = (height.saturating_sub(card_h)) / 2;

    fill_rect_bgra(
        out_bgra, width, height, card_x, card_y, card_w, card_h, 42, 28, 16,
    );
    border_rect_bgra(
        out_bgra, width, height, card_x, card_y, card_w, card_h, 3, 240, 195, 45,
    );

    let center_x = card_x + card_w / 2;
    let base_scale = if width >= 960 {
        3
    } else if width >= 540 {
        2
    } else {
        1
    };
    let url_scale = (base_scale + 1).min(4);

    let y1 = card_y + card_h / 7;
    let y2 = y1 + 12 * base_scale;
    let y3 = y2 + 12 * base_scale;
    let bar_y = y3 + 14 * url_scale;
    let y4 = bar_y + 28;

    draw_centered_text_bgra(
        out_bgra, width, height, center_x, y1, title_line, base_scale, 90, 220, 255,
    );
    draw_centered_text_bgra(
        out_bgra,
        width,
        height,
        center_x,
        y2,
        subtitle_line,
        base_scale,
        215,
        230,
        245,
    );

    // Highlight box around URL
    let url_box_w = ((url_line.len() as u32) * 6 * url_scale + 24).min(card_w.saturating_sub(24));
    let url_box_h = 7 * url_scale + 14;
    let url_box_x = center_x.saturating_sub(url_box_w / 2);
    fill_rect_bgra(
        out_bgra,
        width,
        height,
        url_box_x,
        y3.saturating_sub(6),
        url_box_w,
        url_box_h,
        20,
        14,
        8,
    );
    border_rect_bgra(
        out_bgra,
        width,
        height,
        url_box_x,
        y3.saturating_sub(6),
        url_box_w,
        url_box_h,
        2,
        110,
        245,
        90,
    );
    draw_centered_text_bgra(
        out_bgra, width, height, center_x, y3, url_line, url_scale, 120, 255, 110,
    );

    // Animated radar scan bar
    let bar_w = (card_w * 3) / 5;
    let bar_x = center_x.saturating_sub(bar_w / 2);
    fill_rect_bgra(out_bgra, width, height, bar_x, bar_y, bar_w, 14, 24, 16, 10);

    let pulse_w = (bar_w / 4).max(16);
    let span = (bar_w.saturating_sub(pulse_w)).max(1);
    let offset = ((frame_counter * 6) as u32) % span;
    let (pb, pg, pr) = if is_usb_mode {
        (255, 190, 50)
    } else {
        (120, 240, 60)
    };
    fill_rect_bgra(
        out_bgra,
        width,
        height,
        bar_x + offset,
        bar_y,
        pulse_w,
        14,
        pb,
        pg,
        pr,
    );

    if y4 + 8 * base_scale < card_y + card_h {
        draw_centered_text_bgra(
            out_bgra,
            width,
            height,
            center_x,
            y4,
            footer_line,
            base_scale.saturating_sub(1).max(1),
            175,
            205,
            235,
        );
    }

    // Draw scannable ISO/IEC 18004 QR Code on the right side of the card if space permits
    let qr_url_lower = url_line.trim().to_ascii_lowercase();
    if qr_url_lower.starts_with("http://") || qr_url_lower.starts_with("https://") {
        let (qr_n, qr_mods) = generate_qr_code_matrix(&qr_url_lower);
        let quiet = 2u32;
        let total_mods = (qr_n as u32) + quiet * 2;
        let mod_px = ((card_h / (total_mods + 8)).max(2)).min(5);
        let qr_box_px = total_mods * mod_px;
        if card_w >= qr_box_px + 220 && card_h >= qr_box_px + 20 {
            let qx = (card_x + card_w).saturating_sub(qr_box_px + 14);
            let qy = (card_y + card_h).saturating_sub(qr_box_px + 12);
            fill_rect_bgra(
                out_bgra, width, height, qx, qy, qr_box_px, qr_box_px, 255, 255, 255,
            );
            for r in 0..qr_n {
                for c in 0..qr_n {
                    if qr_mods[r * qr_n + c] {
                        fill_rect_bgra(
                            out_bgra,
                            width,
                            height,
                            qx + (c as u32 + quiet) * mod_px,
                            qy + (r as u32 + quiet) * mod_px,
                            mod_px,
                            mod_px,
                            0,
                            0,
                            0,
                        );
                    }
                }
            }
        }
    }
}

/// Generates a scannable ISO/IEC 18004 QR Code matrix (`(size, modules)`) for `text`
/// using Byte Mode, ECC Level L, and Mask Pattern 0 (Version 2 `25x25` up to 32 bytes,
/// or Version 3 `29x29` up to 53 bytes).
pub fn generate_qr_code_matrix(text: &str) -> (usize, Vec<bool>) {
    let raw_bytes = text.as_bytes();
    let (version, size, data_cap, ecc_len) = if raw_bytes.len() <= 32 {
        (2usize, 25usize, 34usize, 10usize)
    } else {
        (3usize, 29usize, 55usize, 15usize)
    };
    let payload = if raw_bytes.len() > data_cap - 2 {
        &raw_bytes[..data_cap - 2]
    } else {
        raw_bytes
    };

    // 1. Encode Byte-mode bitstream: mode=0100 (4 bits) + len (8 bits) + payload + 0000 terminator + pad 0xEC/0x11
    let mut bits: Vec<u8> = Vec::with_capacity(data_cap * 8);
    for b in [0u8, 1, 0, 0] {
        bits.push(b);
    }
    let len_u8 = payload.len() as u8;
    for i in (0..8).rev() {
        bits.push((len_u8 >> i) & 1);
    }
    for &byte in payload {
        for i in (0..8).rev() {
            bits.push((byte >> i) & 1);
        }
    }
    for _ in 0..4 {
        if bits.len() < data_cap * 8 {
            bits.push(0);
        }
    }
    while bits.len() % 8 != 0 {
        bits.push(0);
    }
    let mut data_cw = Vec::with_capacity(data_cap);
    for chunk in bits.chunks_exact(8) {
        let mut v = 0u8;
        for &bit in chunk {
            v = (v << 1) | bit;
        }
        data_cw.push(v);
    }
    let pad_bytes = [0xECu8, 0x11u8];
    let mut pad_idx = 0usize;
    while data_cw.len() < data_cap {
        data_cw.push(pad_bytes[pad_idx & 1]);
        pad_idx += 1;
    }

    // 2. Reed-Solomon ECC over GF(2^8) with primitive polynomial 0x11D
    let ecc_cw = qr_rs_compute_ecc(&data_cw, ecc_len);
    let mut all_cw = data_cw;
    all_cw.extend_from_slice(&ecc_cw);

    // 3. Build function patterns & reserve matrix
    let mut modules = vec![false; size * size];
    let mut is_func = vec![false; size * size];

    let set_func = |r: usize, c: usize, dark: bool, mods: &mut [bool], func: &mut [bool]| {
        if r < size && c < size {
            mods[r * size + c] = dark;
            func[r * size + c] = true;
        }
    };

    // Finder patterns (7x7) + separators at (0,0), (0, size-7), (size-7, 0)
    let finder_origins = [
        (0isize, 0isize),
        (0isize, (size - 7) as isize),
        ((size - 7) as isize, 0isize),
    ];
    for &(fr, fc) in &finder_origins {
        for dr in -1isize..=7 {
            for dc in -1isize..=7 {
                let rr = fr + dr;
                let cc = fc + dc;
                if rr >= 0 && rr < size as isize && cc >= 0 && cc < size as isize {
                    let dark = if dr >= 0 && dr <= 6 && dc >= 0 && dc <= 6 {
                        dr == 0
                            || dr == 6
                            || dc == 0
                            || dc == 6
                            || (dr >= 2 && dr <= 4 && dc >= 2 && dc <= 4)
                    } else {
                        false
                    };
                    set_func(rr as usize, cc as usize, dark, &mut modules, &mut is_func);
                }
            }
        }
    }

    // Timing patterns along row 6 and col 6
    for i in 8..(size - 8) {
        let dark = (i % 2) == 0;
        set_func(6, i, dark, &mut modules, &mut is_func);
        set_func(i, 6, dark, &mut modules, &mut is_func);
    }

    // Alignment pattern (5x5 centered at size - 7)
    let ac = size - 7;
    for dr in -2isize..=2 {
        for dc in -2isize..=2 {
            let rr = (ac as isize + dr) as usize;
            let cc = (ac as isize + dc) as usize;
            let dist = dr.abs().max(dc.abs());
            let dark = dist == 2 || dist == 0;
            set_func(rr, cc, dark, &mut modules, &mut is_func);
        }
    }

    // Reserve format information areas + Dark Module at (4*version + 9, 8)
    for i in 0..9 {
        if i != 6 {
            is_func[8 * size + i] = true;
            is_func[i * size + 8] = true;
        }
    }
    for i in 0..8 {
        is_func[8 * size + (size - 1 - i)] = true;
        is_func[(size - 1 - i) * size + 8] = true;
    }
    set_func(4 * version + 9, 8, true, &mut modules, &mut is_func);

    // 4. Zig-zag upward/downward 2-column data bit placement + Mask 0 (`(r + c) % 2 == 0`)
    let mut bit_idx = 0usize;
    let total_bits = all_cw.len() * 8;
    let mut right_col = (size - 1) as isize;
    let mut upward = true;
    while right_col >= 1 {
        if right_col == 6 {
            right_col -= 1; // Skip vertical timing pattern column
        }
        for step in 0..size {
            let r = if upward { size - 1 - step } else { step };
            for c_off in 0..2 {
                let c = (right_col - c_off) as usize;
                let idx = r * size + c;
                if !is_func[idx] {
                    let mut dark = false;
                    if bit_idx < total_bits {
                        let byte = all_cw[bit_idx >> 3];
                        let shift = 7 - (bit_idx & 7);
                        dark = ((byte >> shift) & 1) != 0;
                        bit_idx += 1;
                    }
                    // Apply Mask Pattern 0: invert if (r + c) % 2 == 0
                    if (r + c) % 2 == 0 {
                        dark = !dark;
                    }
                    modules[idx] = dark;
                }
            }
        }
        upward = !upward;
        right_col -= 2;
    }

    // 5. Write Format Information for ECC Level L (01) + Mask 0 (000) -> BCH(15,5) masked = 0x77C4
    let fmt_bits: u16 = 0x77C4;
    // Copy 1: around top-left finder pattern
    let tl_coords_r = [8usize, 8, 8, 8, 8, 8, 8, 8, 7, 5, 4, 3, 2, 1, 0];
    let tl_coords_c = [0usize, 1, 2, 3, 4, 5, 7, 8, 8, 8, 8, 8, 8, 8, 8];
    for i in 0..15 {
        let bit_dark = ((fmt_bits >> (14 - i)) & 1) != 0;
        modules[tl_coords_r[i] * size + tl_coords_c[i]] = bit_dark;
    }
    // Copy 2: bottom-left vertical & top-right horizontal
    for i in 0..7 {
        let bit_dark = ((fmt_bits >> (14 - i)) & 1) != 0;
        modules[(size - 1 - i) * size + 8] = bit_dark;
    }
    for i in 7..15 {
        let bit_dark = ((fmt_bits >> (14 - i)) & 1) != 0;
        modules[8 * size + (size - 15 + i)] = bit_dark;
    }

    (size, modules)
}

fn qr_gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if (b & 1) != 0 {
            p ^= a;
        }
        let hi = (a & 0x80) != 0;
        a <<= 1;
        if hi {
            a ^= 0x1D;
        }
        b >>= 1;
    }
    p
}

fn qr_rs_compute_ecc(data: &[u8], ecc_len: usize) -> Vec<u8> {
    // Build generator polynomial g(x) = (x - 2^0)(x - 2^1)...(x - 2^(ecc_len-1))
    let mut gen = vec![1u8];
    let mut root = 1u8;
    for _ in 0..ecc_len {
        let mut next = vec![0u8; gen.len() + 1];
        for (i, &c) in gen.iter().enumerate() {
            next[i] ^= c;
            next[i + 1] ^= qr_gf_mul(c, root);
        }
        gen = next;
        root = qr_gf_mul(root, 2);
    }

    let mut rem = vec![0u8; ecc_len];
    for &byte in data {
        let factor = byte ^ rem[0];
        rem.remove(0);
        rem.push(0);
        for i in 0..ecc_len {
            rem[i] ^= qr_gf_mul(gen[i + 1], factor);
        }
    }
    rem
}

fn fill_rect_bgra(
    buf: &mut [u8],
    width: u32,
    height: u32,
    rx: u32,
    ry: u32,
    rw: u32,
    rh: u32,
    b: u8,
    g: u8,
    r: u8,
) {
    let x_end = (rx + rw).min(width);
    let y_end = (ry + rh).min(height);
    let stride = (width as usize) * 4;
    for y in ry..y_end {
        let row = (y as usize) * stride;
        for x in rx..x_end {
            let idx = row + (x as usize) * 4;
            buf[idx] = b;
            buf[idx + 1] = g;
            buf[idx + 2] = r;
            buf[idx + 3] = 255;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn border_rect_bgra(
    buf: &mut [u8],
    width: u32,
    height: u32,
    rx: u32,
    ry: u32,
    rw: u32,
    rh: u32,
    t: u32,
    b: u8,
    g: u8,
    r: u8,
) {
    fill_rect_bgra(buf, width, height, rx, ry, rw, t, b, g, r);
    if rh > t {
        fill_rect_bgra(buf, width, height, rx, ry + rh - t, rw, t, b, g, r);
    }
    fill_rect_bgra(buf, width, height, rx, ry, t, rh, b, g, r);
    if rw > t {
        fill_rect_bgra(buf, width, height, rx + rw - t, ry, t, rh, b, g, r);
    }
}

// ============================================================================
// Multi-Person Foreground Mask Refinement (`multi_person_tracker`)
// ============================================================================

/// Detects secondary faces and upper-body silhouettes across all horizontal zones of the frame
/// (up to 4 simultaneous people) and reinforces their foreground segmentation mask (`inout_mask`).
fn apply_multi_person_mask_refinement(bgra: &[u8], inout_mask: &mut [u8], width: u32, height: u32) {
    if width < 32 || height < 32 {
        return;
    }
    let w = width as usize;
    let h = height as usize;

    // Divide frame horizontally into 4 person zones and scan for skin-tone / face clusters
    let zones = 4usize;
    let zone_w = (w / zones).max(1);

    for z in 0..zones {
        let x0 = z * zone_w;
        let x1 = if z + 1 == zones { w } else { (z + 1) * zone_w };

        let mut skin_sum_x = 0usize;
        let mut skin_sum_y = 0usize;
        let mut skin_count = 0usize;

        for y in (h / 10)..(h * 7 / 10) {
            if y % 4 != 0 {
                continue;
            }
            let row_px = y * w;
            for x in (x0..x1).step_by(4) {
                let p = row_px + x;
                let b = bgra[p * 4] as i32;
                let g = bgra[p * 4 + 1] as i32;
                let r = bgra[p * 4 + 2] as i32;

                // YCbCr + RGB skin-cluster detector
                let is_skin = r > 85
                    && g > 38
                    && b > 20
                    && r > g
                    && r > b
                    && (r - g) >= 12
                    && (r - b) >= 22
                    && (r - b) <= 145;
                if is_skin {
                    skin_sum_x += x;
                    skin_sum_y += y;
                    skin_count += 1;
                }
            }
        }

        // If a secondary person's face cluster is present in this zone, reinforce their head + torso mask
        let min_hits = ((zone_w * h) / (16 * 55)).max(12);
        if skin_count >= min_hits {
            let cx = (skin_sum_x / skin_count) as i32;
            let cy = (skin_sum_y / skin_count) as i32;
            let head_rx = (zone_w as i32 * 2 / 5).max(28);
            let head_ry = (h as i32 / 5).max(36);
            let torso_rx = (zone_w as i32 * 3 / 5).max(44);
            let torso_ry = (h as i32 * 2 / 5).max(64);
            let torso_cy = (cy + head_ry + torso_ry / 2).min(h as i32 - 1);

            let y_start = (cy - head_ry).max(0) as usize;
            let y_end = (torso_cy + torso_ry).min(h as i32) as usize;
            let x_start = (cx - torso_rx).max(0) as usize;
            let x_end = (cx + torso_rx).min(w as i32) as usize;

            for y in y_start..y_end {
                let row_px = y * w;
                let dy_head = (y as i32) - cy;
                let dy_torso = (y as i32) - torso_cy;
                for x in x_start..x_end {
                    let dx = (x as i32) - cx;
                    let in_head = (dx * dx) * 100 / (head_rx * head_rx).max(1)
                        + (dy_head * dy_head) * 100 / (head_ry * head_ry).max(1);
                    let in_torso = (dx * dx) * 100 / (torso_rx * torso_rx).max(1)
                        + (dy_torso * dy_torso) * 100 / (torso_ry * torso_ry).max(1);
                    let d_norm = in_head.min(in_torso);
                    if d_norm <= 100 {
                        let boost = if d_norm <= 65 {
                            255u8
                        } else {
                            ((100 - d_norm) * 255 / 35).clamp(0, 255) as u8
                        };
                        let idx = row_px + x;
                        if boost > inout_mask[idx] {
                            inout_mask[idx] =
                                ((inout_mask[idx] as u16 + boost as u16 * 2) / 3) as u8;
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Studio Color Enhancer & Virtual Teleprompter Overlay Filters
// ============================================================================

/// Applies the built-in `studio_color_enhancer`:
/// 1. Computes frame average luminance for automatic low-light exposure lift.
/// 2. Protects natural skin tones against clipping/oversaturation.
/// 3. Applies shadow noise reduction + warm broadcast S-curve LUT.
fn apply_studio_color_enhancer_bgra(inout_bgra: &mut [u8], width: u32, height: u32) {
    let total_pixels = (width as usize) * (height as usize);
    if total_pixels == 0 {
        return;
    }

    // 1. Sample average frame luminance to compute adaptive low-light gain
    let mut luma_sum: u64 = 0;
    let mut sample_cnt: u64 = 0;
    for i in (0..total_pixels).step_by(8) {
        let idx = i * 4;
        let b = inout_bgra[idx] as u64;
        let g = inout_bgra[idx + 1] as u64;
        let r = inout_bgra[idx + 2] as u64;
        luma_sum += (r * 77 + g * 150 + b * 29) >> 8;
        sample_cnt += 1;
    }
    let avg_luma = (luma_sum / sample_cnt.max(1)) as i32;
    // Lift dark rooms smoothly toward target studio luma ~122
    let exposure_lift = ((122 - avg_luma) / 4).clamp(-8, 22);

    // 2. Apply shadow denoise + skin-aware broadcast LUT
    for i in 0..total_pixels {
        let idx = i * 4;
        let mut b = inout_bgra[idx] as i32;
        let mut g = inout_bgra[idx + 1] as i32;
        let mut r = inout_bgra[idx + 2] as i32;

        let luma = (r * 77 + g * 150 + b * 29) >> 8;

        // Fast horizontal chroma denoise in dark shadows (`luma < 56`)
        if luma < 56 && i > 0 && i + 1 < total_pixels {
            let prev = (i - 1) * 4;
            let next = (i + 1) * 4;
            b = (b * 2 + inout_bgra[prev] as i32 + inout_bgra[next] as i32) >> 2;
            g = (g * 2 + inout_bgra[prev + 1] as i32 + inout_bgra[next + 1] as i32) >> 2;
            r = (r * 2 + inout_bgra[prev + 2] as i32 + inout_bgra[next + 2] as i32) >> 2;
        }

        let is_skin = r > g && g > b && (r - b) > 18 && r > 70 && r < 235;
        let contrast_gain = if is_skin { 104 } else { 109 };

        let r_out = (((r + exposure_lift - 128) * contrast_gain) / 100 + 133).clamp(0, 255) as u8;
        let g_out =
            (((g + exposure_lift - 128) * (contrast_gain - 2)) / 100 + 129).clamp(0, 255) as u8;
        let b_out =
            (((b + exposure_lift - 128) * (contrast_gain - 5)) / 100 + 124).clamp(0, 255) as u8;

        inout_bgra[idx] = b_out;
        inout_bgra[idx + 1] = g_out;
        inout_bgra[idx + 2] = r_out;
    }
}

/// Applies the built-in `virtual_teleprompter_overlay` semi-transparent top guide bar
/// with a scrolling 5x7 bitmap prompt and center eye-contact reticle.
fn apply_teleprompter_overlay_bgra(
    inout_bgra: &mut [u8],
    width: u32,
    height: u32,
    raw_note: &str,
    frame_counter: u64,
) {
    let bar_h = (height / 7).clamp(32, 92);
    let stride = (width as usize) * 4;
    for y in 0..bar_h {
        let row = (y as usize) * stride;
        for x in 0..width {
            let idx = row + (x as usize) * 4;
            // 70% dark translucent glass bar at the top of the frame
            inout_bgra[idx] = ((inout_bgra[idx] as u32 * 30 + 24 * 70) / 100) as u8;
            inout_bgra[idx + 1] = ((inout_bgra[idx + 1] as u32 * 30 + 18 * 70) / 100) as u8;
            inout_bgra[idx + 2] = ((inout_bgra[idx + 2] as u32 * 30 + 12 * 70) / 100) as u8;
        }
    }

    let clean_note = if raw_note.trim().is_empty()
        || raw_note.trim().eq_ignore_ascii_case("preview_only")
        || raw_note.trim().eq_ignore_ascii_case("stream")
    {
        "GUARDA QUI VERSO L OBIETTIVO - SMARTCAM TELEPROMPTER ATTIVO"
    } else {
        raw_note.trim()
    };

    let text_scale = if width >= 960 { 2 } else { 1 };
    let text_y = (bar_h.saturating_sub(7 * text_scale + 6)) / 2;

    // Smooth horizontal ticker if text is long, otherwise centered near the top camera lens
    let padded = format!("{}   -   ", clean_note.to_ascii_uppercase());
    let chars: Vec<char> = padded.chars().collect();
    if !chars.is_empty() {
        let max_vis_chars = ((width.saturating_sub(32)) / (6 * text_scale)).max(12) as usize;
        let scroll_idx = ((frame_counter / 6) as usize) % chars.len();
        let mut window_str = String::with_capacity(max_vis_chars);
        for i in 0..max_vis_chars.min(chars.len()) {
            window_str.push(chars[(scroll_idx + i) % chars.len()]);
        }
        draw_centered_text_bgra(
            inout_bgra,
            width,
            height,
            width / 2,
            text_y,
            &window_str,
            text_scale,
            140,
            245,
            255,
        );
    }

    // Center eye-contact reticle marker
    let marker_w = (width / 9).clamp(44, 150);
    let marker_x = (width.saturating_sub(marker_w)) / 2;
    let marker_y = bar_h.saturating_sub(4);
    fill_rect_bgra(
        inout_bgra, width, height, marker_x, marker_y, marker_w, 3, 60, 215, 255,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, ClientConnection, RootCertStore};

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
        fn Process32FirstW(h_snapshot: *mut c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(h_snapshot: *mut c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(h_object: *mut c_void) -> i32;
    }

    fn count_child_powershell_processes() -> usize {
        const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
        unsafe {
            let my_pid = GetCurrentProcessId();
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || (snap as isize) == -1 {
                return 0;
            }
            let mut entry: ProcessEntry32W = std::mem::zeroed();
            entry.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
            let mut count = 0usize;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    if entry.th32_parent_process_id == my_pid {
                        let len = entry
                            .sz_exe_file
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.sz_exe_file.len());
                        let name = String::from_utf16_lossy(&entry.sz_exe_file[..len])
                            .to_ascii_lowercase();
                        if name.contains("powershell") || name.contains("pwsh") {
                            count += 1;
                        }
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            count
        }
    }

    #[test]
    fn test_native_rust_tls_server_handshake_and_zero_powershell() {
        let tls_mat = get_or_init_phone_tls_material()
            .expect("rcgen + rustls must generate self-signed X.509 certificate in-process");
        assert!(!tls_mat.cert_der.is_empty());

        // Pick a free test port for the PhoneCameraServer
        let mut test_port = 18440u16;
        for candidate in 18440..18540 {
            if TcpListener::bind(("127.0.0.1", candidate)).is_ok()
                && TcpListener::bind(("127.0.0.1", candidate + 1000)).is_ok()
            {
                test_port = candidate;
                break;
            }
        }

        let shared_frame = Arc::new(Mutex::new(PhoneFrameSharedState::default()));
        let received_frames = Arc::new(AtomicU64::new(0));
        let tcp_connections = Arc::new(AtomicU64::new(0));
        let last_client_log = Arc::new(Mutex::new(String::new()));
        let remote_pull_url = Arc::new(Mutex::new(String::new()));

        let mut server = PhoneCameraServer::new(
            test_port,
            Arc::clone(&shared_frame),
            Arc::clone(&received_frames),
            Arc::clone(&tcp_connections),
            Arc::clone(&last_client_log),
            Arc::clone(&remote_pull_url),
        );
        assert!(server.is_running.load(Ordering::SeqCst));

        // Verify zero powershell.exe child processes were spawned by the server
        assert_eq!(
            count_child_powershell_processes(),
            0,
            "PhoneCameraServer must not spawn any powershell.exe subprocesses"
        );

        // Configure a rustls ClientConfig trusting ONLY the rcgen self-signed certificate
        let mut root_store = RootCertStore::empty();
        root_store
            .add(CertificateDer::from(tls_mat.cert_der.clone()))
            .expect("Generated rcgen certificate must be valid DER");

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let client_config = Arc::new(
            ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("TLS protocol versions")
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );

        // 1. Test HTTPS GET /status on the single-port HTTP/HTTPS multiplexed listener (`127.0.0.1:<test_port>`)
        {
            let server_name = ServerName::try_from("localhost").expect("valid DNS ServerName");
            let client_conn =
                ClientConnection::new(Arc::clone(&client_config), server_name).unwrap();
            let tcp =
                TcpStream::connect(("127.0.0.1", test_port)).expect("connect to multiplexed port");
            tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();

            let mut tls_stream = StreamOwned::new(client_conn, tcp);
            let req = format!(
                "GET /status HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
                test_port
            );
            tls_stream
                .write_all(req.as_bytes())
                .expect("HTTPS write GET /status");
            tls_stream.flush().unwrap();

            let mut resp_bytes = Vec::new();
            let _ = tls_stream.read_to_end(&mut resp_bytes);
            let resp_str = String::from_utf8_lossy(&resp_bytes);
            assert!(
                resp_str.starts_with("HTTP/1.1 200 OK"),
                "Expected 200 OK over HTTPS, got: {}",
                resp_str
            );
            assert!(
                resp_str.contains("\"ok\":true"),
                "Expected JSON body with \"ok\":true, got: {}",
                resp_str
            );
        }

        // 2. Test HTTPS POST /frame on the loopback TLS port (`127.0.0.1:<test_port + 1000>`) using IP SAN 127.0.0.1
        {
            let ip_server_name =
                ServerName::IpAddress(rustls::pki_types::IpAddr::try_from("127.0.0.1").unwrap());
            let client_conn =
                ClientConnection::new(Arc::clone(&client_config), ip_server_name).unwrap();
            let tcp = TcpStream::connect(("127.0.0.1", test_port + 1000))
                .expect("connect to TLS loopback port");
            tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();

            let mut tls_stream = StreamOwned::new(client_conn, tcp);
            let raw_rgba = vec![128u8; 16 * 16 * 4];
            let hdr = format!(
                "POST /frame?w=16&h=16&fmt=rgba HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                test_port,
                raw_rgba.len()
            );
            tls_stream.write_all(hdr.as_bytes()).unwrap();
            tls_stream.write_all(&raw_rgba).unwrap();
            tls_stream.flush().unwrap();

            let mut resp_bytes = Vec::new();
            let _ = tls_stream.read_to_end(&mut resp_bytes);
            let resp_str = String::from_utf8_lossy(&resp_bytes);
            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"ok\":true"));
            assert_eq!(received_frames.load(Ordering::SeqCst), 1);
        }

        assert_eq!(count_child_powershell_processes(), 0);
        server.stop();
    }

    #[test]
    fn test_parsed_phone_endpoint_and_mjpeg_boundary_extraction() {
        let ep_http = ParsedPhoneEndpoint::parse("http://192.168.1.55:8080/video");
        assert_eq!(ep_http.scheme, "http");
        assert_eq!(ep_http.host, "192.168.1.55");
        assert_eq!(ep_http.port, 8080);
        assert_eq!(ep_http.path, "/video");
        assert!(!ep_http.is_usb);

        let ep_rtsp = ParsedPhoneEndpoint::parse("rtsp://192.168.1.120/live");
        assert_eq!(ep_rtsp.scheme, "rtsp");
        assert_eq!(ep_rtsp.host, "192.168.1.120");
        assert_eq!(ep_rtsp.port, 8554);
        assert_eq!(ep_rtsp.path, "/live");
        assert!(!ep_rtsp.is_usb);

        let ep_usb = ParsedPhoneEndpoint::parse("http://127.0.0.1:8080/shot.jpg");
        assert!(ep_usb.is_usb);

        // Test multipart MJPEG stream extraction with explicit Content-Length
        let fake_jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0xFF, 0xD9,
        ];
        let mut mjpeg_stream = Vec::new();
        mjpeg_stream.extend_from_slice(
            format!(
                "HTTP/1.0 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=myboundary\r\n\r\n\
                 --myboundary\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                fake_jpeg.len()
            )
            .as_bytes(),
        );
        mjpeg_stream.extend_from_slice(&fake_jpeg);
        mjpeg_stream.extend_from_slice(b"\r\n--myboundary\r\n");

        let (extracted, consumed) =
            extract_mjpeg_frame_from_buffer(&mjpeg_stream).expect("must extract MJPEG frame");
        assert_eq!(extracted, fake_jpeg);
        assert!(consumed >= fake_jpeg.len());
    }

    #[test]
    fn test_plugin_registry_push_frame_and_filters() {
        let (qr_size, qr_mods) = generate_qr_code_matrix("https://192.168.1.55:8080");
        assert_eq!(qr_size, 25);
        assert_eq!(qr_mods.len(), 25 * 25);

        let mut bgra = vec![40u8; 64 * 64 * 4];
        for px in bgra.chunks_exact_mut(4) {
            px[3] = 255;
        }
        let mut mask = vec![0u8; 64 * 64];
        apply_studio_color_enhancer_bgra(&mut bgra, 64, 64);
        assert!(
            bgra[2] > 40,
            "low-light exposure lift must brighten dark frame"
        );
        apply_multi_person_mask_refinement(&bgra, &mut mask, 64, 64);
        apply_teleprompter_overlay_bgra(&mut bgra, 64, 64, "TEST TELEPROMPTER", 1);
        assert_eq!(bgra[3], 255);
    }
}
