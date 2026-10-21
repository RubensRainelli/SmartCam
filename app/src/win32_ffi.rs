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

#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code)]

use std::ffi::c_void;

pub type BOOL = i32;
pub type BYTE = u8;
pub type WORD = u16;
pub type DWORD = u32;
pub type UINT = u32;
pub type INT = i32;
pub type LONG = i32;
pub type LONGLONG = i64;
pub type ULONGLONG = u64;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type HRESULT = i32;

pub type HANDLE = *mut c_void;
pub type HINSTANCE = *mut c_void;
pub type HMODULE = *mut c_void;
pub type HWND = *mut c_void;
pub type HDC = *mut c_void;
pub type HGDIOBJ = *mut c_void;
pub type HBRUSH = *mut c_void;
pub type HPEN = *mut c_void;
pub type HFONT = *mut c_void;
pub type HICON = *mut c_void;
pub type HCURSOR = *mut c_void;
pub type HMENU = *mut c_void;
pub type HBITMAP = *mut c_void;
pub type HKEY = *mut c_void;
pub type FARPROC = *const c_void;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const NULL: *mut c_void = std::ptr::null_mut();
pub const TRUE: BOOL = 1;
pub const FALSE: BOOL = 0;

pub const PAGE_READWRITE: DWORD = 0x04;
pub const FILE_MAP_ALL_ACCESS: DWORD = 0x000F001F;
pub const FILE_MAP_READ: DWORD = 0x0004;
pub const ERROR_ALREADY_EXISTS: DWORD = 183;

// Registry constants
pub const HKEY_CURRENT_USER: HKEY = 0x80000001usize as HKEY;
pub const HKEY_LOCAL_MACHINE: HKEY = 0x80000002usize as HKEY;
pub const KEY_READ: DWORD = 0x20019;
pub const KEY_WRITE: DWORD = 0x20006;
pub const KEY_WOW64_64KEY: DWORD = 0x0100;
pub const REG_SZ: DWORD = 1;
pub const REG_BINARY: DWORD = 3;
pub const REG_DWORD: DWORD = 4;

// Window Messages
pub const WM_CREATE: UINT = 0x0001;
pub const WM_DESTROY: UINT = 0x0002;
pub const WM_SIZE: UINT = 0x0005;
pub const WM_PAINT: UINT = 0x000F;
pub const WM_CLOSE: UINT = 0x0010;
pub const WM_QUIT: UINT = 0x0012;
pub const WM_ERASEBKGND: UINT = 0x0014;
pub const WM_SETFONT: UINT = 0x0030;
pub const WM_GETMINMAXINFO: UINT = 0x0024;
pub const WM_KEYDOWN: UINT = 0x0100;
pub const WM_CHAR: UINT = 0x0102;
pub const WM_COMMAND: UINT = 0x0111;
pub const WM_TIMER: UINT = 0x0113;
pub const WM_HSCROLL: UINT = 0x0114;
pub const WM_CTLCOLORSTATIC: UINT = 0x0138;
pub const WM_CTLCOLORBTN: UINT = 0x0135;
pub const WM_MOUSEMOVE: UINT = 0x0200;
pub const WM_LBUTTONDOWN: UINT = 0x0201;
pub const WM_LBUTTONUP: UINT = 0x0202;
pub const WM_RBUTTONUP: UINT = 0x0205;
pub const WM_MOUSEWHEEL: UINT = 0x020A;
pub const WM_SETICON: UINT = 0x0080;
pub const WM_USER: UINT = 0x0400;
pub const WM_TRAYICON: UINT = WM_USER + 100;
pub const WM_SMARTCAM_SHOW_EXISTING: UINT = WM_USER + 101;
pub const WM_SMARTCAM_FORCE_EXIT: UINT = WM_USER + 102;
pub const ASFW_ANY: DWORD = 0xFFFF_FFFF;
pub const SM_CXSCREEN: INT = 0;
pub const SM_CYSCREEN: INT = 1;

pub const ICON_SMALL: WPARAM = 0;
pub const ICON_BIG: WPARAM = 1;
pub const IMAGE_ICON: UINT = 1;
pub const LR_LOADFROMFILE: UINT = 0x00000010;
pub const LR_DEFAULTSIZE: UINT = 0x00000040;
pub const ATTACH_PARENT_PROCESS: DWORD = 0xFFFF_FFFF;

// OpenFileName flags
pub const OFN_FILEMUSTEXIST: DWORD = 0x00001000;
pub const OFN_PATHMUSTEXIST: DWORD = 0x00000800;
pub const OFN_NOCHANGEDIR: DWORD = 0x00000008;
pub const OFN_EXPLORER: DWORD = 0x00080000;

// GDI+ constants
pub const PixelFormat32bppARGB: i32 = 0x0026200A;
pub const ImageLockModeRead: u32 = 0x0001;

// Window Styles
pub const WS_OVERLAPPED: DWORD = 0x00000000;
pub const WS_POPUP: DWORD = 0x80000000;
pub const WS_CHILD: DWORD = 0x40000000;
pub const WS_VISIBLE: DWORD = 0x10000000;
pub const WS_CAPTION: DWORD = 0x00C00000;
pub const WS_BORDER: DWORD = 0x00800000;
pub const WS_SYSMENU: DWORD = 0x00080000;
pub const WS_THICKFRAME: DWORD = 0x00040000;
pub const WS_MINIMIZEBOX: DWORD = 0x00020000;
pub const WS_MAXIMIZEBOX: DWORD = 0x00010000;
pub const WS_CLIPCHILDREN: DWORD = 0x02000000;
pub const WS_CLIPSIBLINGS: DWORD = 0x04000000;
pub const WS_VSCROLL: DWORD = 0x00200000;
pub const WS_TABSTOP: DWORD = 0x00010000;
pub const WS_OVERLAPPEDWINDOW: DWORD =
    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;

pub const WS_EX_TOPMOST: DWORD = 0x00000008;
pub const WS_EX_APPWINDOW: DWORD = 0x00040000;

// ComboBox / Button / Edit Styles & Messages
pub const BS_PUSHBUTTON: DWORD = 0x00000000;
pub const BS_AUTOCHECKBOX: DWORD = 0x00000003;
pub const CBS_DROPDOWNLIST: DWORD = 0x0003;
pub const CB_ADDSTRING: UINT = 0x0143;
pub const CB_SETCURSEL: UINT = 0x014E;
pub const CB_GETCURSEL: UINT = 0x0147;
pub const CB_RESETCONTENT: UINT = 0x014B;
pub const CBN_SELCHANGE: WORD = 1;
pub const BN_CLICKED: WORD = 0;
pub const BM_GETCHECK: UINT = 0x00F0;
pub const BM_SETCHECK: UINT = 0x00F1;
pub const BST_CHECKED: usize = 1;
pub const BST_UNCHECKED: usize = 0;

// ShowWindow commands
pub const SW_HIDE: INT = 0;
pub const SW_SHOWNORMAL: INT = 1;
pub const SW_SHOW: INT = 5;
pub const SW_RESTORE: INT = 9;

// GDI constants
pub const BI_RGB: DWORD = 0;
pub const DIB_RGB_COLORS: UINT = 0;
pub const SRCCOPY: DWORD = 0x00CC0020;
pub const TRANSPARENT: INT = 1;
pub const COLORONCOLOR: INT = 3;
pub const HALFTONE: INT = 4;
pub const DT_LEFT: UINT = 0x00000000;
pub const DT_CENTER: UINT = 0x00000001;
pub const DT_RIGHT: UINT = 0x00000002;
pub const DT_VCENTER: UINT = 0x00000004;
pub const DT_WORDBREAK: UINT = 0x00000010;
pub const DT_SINGLELINE: UINT = 0x00000020;
pub const DT_NOPREFIX: UINT = 0x00000800;
pub const PS_SOLID: INT = 0;
pub const NULL_BRUSH: INT = 5;

// Cursors & Icons
pub const IDC_ARROW: *const u16 = 32512usize as *const u16;
pub const IDC_HAND: *const u16 = 32649usize as *const u16;
pub const IDC_CROSS: *const u16 = 32515usize as *const u16;
pub const IDI_APPLICATION: *const u16 = 32512usize as *const u16;
pub const IDI_SHIELD: *const u16 = 32518usize as *const u16;

// MessageBox
pub const MB_OK: UINT = 0x00000000;
pub const MB_OKCANCEL: UINT = 0x00000001;
pub const MB_YESNO: UINT = 0x00000004;
pub const MB_ICONINFORMATION: UINT = 0x00000040;
pub const MB_ICONWARNING: UINT = 0x00000030;
pub const MB_ICONERROR: UINT = 0x00000010;
pub const MB_TOPMOST: UINT = 0x00040000;
pub const IDOK: INT = 1;
pub const IDCANCEL: INT = 2;
pub const IDYES: INT = 6;
pub const IDNO: INT = 7;

// Menu flags
pub const MF_STRING: UINT = 0x00000000;
pub const MF_CHECKED: UINT = 0x00000008;
pub const MF_UNCHECKED: UINT = 0x00000000;
pub const MF_SEPARATOR: UINT = 0x00000800;
pub const TPM_RIGHTBUTTON: UINT = 0x0002;
pub const TPM_RETURNCMD: UINT = 0x0100;
pub const TPM_NONOTIFY: UINT = 0x0080;
pub const TPM_BOTTOMALIGN: UINT = 0x0020;

// Shell_NotifyIconW constants
pub const NIM_ADD: DWORD = 0x00000000;
pub const NIM_MODIFY: DWORD = 0x00000001;
pub const NIM_DELETE: DWORD = 0x00000002;
pub const NIF_MESSAGE: UINT = 0x00000001;
pub const NIF_ICON: UINT = 0x00000002;
pub const NIF_TIP: UINT = 0x00000004;
pub const NIF_INFO: UINT = 0x00000010;
pub const NIIF_INFO: DWORD = 0x00000001;

// PeekMessage
pub const PM_REMOVE: UINT = 0x0001;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct POINT {
    pub x: LONG,
    pub y: LONG,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct RECT {
    pub left: LONG,
    pub top: LONG,
    pub right: LONG,
    pub bottom: LONG,
}

impl RECT {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: UINT,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: DWORD,
    pub pt: POINT,
}

pub type WNDPROC = unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT;

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: UINT,
    pub style: UINT,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: INT,
    pub cbWndExtra: INT,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HICON,
}

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: HDC,
    pub fErase: BOOL,
    pub rcPaint: RECT,
    pub fRestore: BOOL,
    pub fIncUpdate: BOOL,
    pub rgbReserved: [BYTE; 32],
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct BITMAPINFOHEADER {
    pub biSize: DWORD,
    pub biWidth: LONG,
    pub biHeight: LONG,
    pub biPlanes: WORD,
    pub biBitCount: WORD,
    pub biCompression: DWORD,
    pub biSizeImage: DWORD,
    pub biXPelsPerMeter: LONG,
    pub biYPelsPerMeter: LONG,
    pub biClrUsed: DWORD,
    pub biClrImportant: DWORD,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct RGBQUAD {
    pub rgbBlue: BYTE,
    pub rgbGreen: BYTE,
    pub rgbRed: BYTE,
    pub rgbReserved: BYTE,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [RGBQUAD; 1],
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct GUID {
    pub Data1: u32,
    pub Data2: u16,
    pub Data3: u16,
    pub Data4: [u8; 8],
}

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: DWORD,
    pub hWnd: HWND,
    pub uID: UINT,
    pub uFlags: UINT,
    pub uCallbackMessage: UINT,
    pub hIcon: HICON,
    pub szTip: [u16; 128],
    pub dwState: DWORD,
    pub dwStateMask: DWORD,
    pub szInfo: [u16; 256],
    pub uTimeoutOrVersion: UINT,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: DWORD,
    pub guidItem: GUID,
    pub hBalloonIcon: HICON,
}

#[repr(C)]
pub struct ICONINFO {
    pub fIcon: BOOL,
    pub xHotspot: DWORD,
    pub yHotspot: DWORD,
    pub hbmMask: HBITMAP,
    pub hbmColor: HBITMAP,
}

#[repr(C)]
pub struct OPENFILENAMEW {
    pub lStructSize: DWORD,
    pub hwndOwner: HWND,
    pub hInstance: HINSTANCE,
    pub lpstrFilter: *const u16,
    pub lpstrCustomFilter: *mut u16,
    pub nMaxCustFilter: DWORD,
    pub nFilterIndex: DWORD,
    pub lpstrFile: *mut u16,
    pub nMaxFile: DWORD,
    pub lpstrFileTitle: *mut u16,
    pub nMaxFileTitle: DWORD,
    pub lpstrInitialDir: *const u16,
    pub lpstrTitle: *const u16,
    pub Flags: DWORD,
    pub nFileOffset: WORD,
    pub nFileExtension: WORD,
    pub lpstrDefExt: *const u16,
    pub lCustData: LPARAM,
    pub lpfnHook: *const c_void,
    pub lpTemplateName: *const u16,
    pub pvReserved: *mut c_void,
    pub dwReserved: DWORD,
    pub FlagsEx: DWORD,
}

#[repr(C)]
pub struct GdiplusStartupInput {
    pub GdiplusVersion: u32,
    pub DebugEventCallback: *const c_void,
    pub SuppressBackgroundThread: BOOL,
    pub SuppressExternalCodecs: BOOL,
}

#[repr(C)]
pub struct GpRect {
    pub X: i32,
    pub Y: i32,
    pub Width: i32,
    pub Height: i32,
}

#[repr(C)]
pub struct BitmapData {
    pub Width: u32,
    pub Height: u32,
    pub Stride: i32,
    pub PixelFormat: i32,
    pub Scan0: *mut u8,
    pub Reserved: usize,
}

pub const fn make_int_resource_w(id: u16) -> *const u16 {
    id as usize as *const u16
}

pub const fn rgb(r: u8, g: u8, b: u8) -> DWORD {
    (r as DWORD) | ((g as DWORD) << 8) | ((b as DWORD) << 16)
}

pub fn to_wstring(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wstring_slice(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
    pub fn LoadLibraryW(lpLibFileName: *const u16) -> HMODULE;
    pub fn GetProcAddress(hModule: HMODULE, lpProcName: *const u8) -> FARPROC;
    pub fn FreeLibrary(hLibModule: HMODULE) -> BOOL;
    pub fn GetLastError() -> DWORD;
    pub fn AttachConsole(dwProcessId: u32) -> i32;
    pub fn GetStdHandle(nStdHandle: u32) -> HANDLE;
    pub fn CreateFileMappingW(
        hFile: HANDLE,
        lpFileMappingAttributes: *mut c_void,
        flProtect: DWORD,
        dwMaximumSizeHigh: DWORD,
        dwMaximumSizeLow: DWORD,
        lpName: *const u16,
    ) -> HANDLE;
    pub fn OpenFileMappingW(
        dwDesiredAccess: DWORD,
        bInheritHandle: BOOL,
        lpName: *const u16,
    ) -> HANDLE;
    pub fn MapViewOfFile(
        hFileMappingObject: HANDLE,
        dwDesiredAccess: DWORD,
        dwFileOffsetHigh: DWORD,
        dwFileOffsetLow: DWORD,
        dwNumberOfBytesToMap: usize,
    ) -> *mut c_void;
    pub fn UnmapViewOfFile(lpBaseAddress: *const c_void) -> BOOL;
    pub fn CreateEventW(
        lpEventAttributes: *mut c_void,
        bManualReset: BOOL,
        bInitialState: BOOL,
        lpName: *const u16,
    ) -> HANDLE;
    pub fn SetEvent(hEvent: HANDLE) -> BOOL;
    pub fn CreateMutexW(
        lpMutexAttributes: *mut c_void,
        bInitialOwner: BOOL,
        lpName: *const u16,
    ) -> HANDLE;
    pub fn GetCurrentProcessId() -> DWORD;
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn GetConsoleWindow() -> HWND;
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> WORD;
    pub fn CreateWindowExW(
        dwExStyle: DWORD,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: DWORD,
        X: INT,
        Y: INT,
        nWidth: INT,
        nHeight: INT,
        hWndParent: HWND,
        hMenu: HMENU,
        hInstance: HINSTANCE,
        lpParam: *mut c_void,
    ) -> HWND;
    pub fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
    pub fn DestroyWindow(hWnd: HWND) -> BOOL;
    pub fn ShowWindow(hWnd: HWND, nCmdShow: INT) -> BOOL;
    pub fn ShowWindowAsync(hWnd: HWND, nCmdShow: INT) -> BOOL;
    pub fn UpdateWindow(hWnd: HWND) -> BOOL;
    pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
    pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
    pub fn AllowSetForegroundWindow(dwProcessId: DWORD) -> BOOL;
    pub fn BringWindowToTop(hWnd: HWND) -> BOOL;
    pub fn GetSystemMetrics(nIndex: INT) -> INT;
    pub fn DefWindowProcW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: UINT, wMsgFilterMax: UINT) -> BOOL;
    pub fn PeekMessageW(
        lpMsg: *mut MSG,
        hWnd: HWND,
        wMsgFilterMin: UINT,
        wMsgFilterMax: UINT,
        wRemoveMsg: UINT,
    ) -> BOOL;
    pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostQuitMessage(nExitCode: INT);
    pub fn SendMessageW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn PostMessageW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> BOOL;
    pub fn SetTimer(hWnd: HWND, nIDEvent: usize, uElapse: UINT, lpTimerFunc: *const c_void) -> usize;
    pub fn KillTimer(hWnd: HWND, uIDEvent: usize) -> BOOL;
    pub fn BeginPaint(hWnd: HWND, lpPaint: *mut PAINTSTRUCT) -> HDC;
    pub fn EndPaint(hWnd: HWND, lpPaint: *const PAINTSTRUCT) -> BOOL;
    pub fn InvalidateRect(hWnd: HWND, lpRect: *const RECT, bErase: BOOL) -> BOOL;
    pub fn GetClientRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
    pub fn FillRect(hDC: HDC, lprc: *const RECT, hbr: HBRUSH) -> INT;
    pub fn FrameRect(hDC: HDC, lprc: *const RECT, hbr: HBRUSH) -> INT;
    pub fn DrawTextW(
        hdc: HDC,
        lpchText: *const u16,
        cchText: INT,
        lprc: *mut RECT,
        format: UINT,
    ) -> INT;
    pub fn LoadCursorW(hInstance: HINSTANCE, lpCursorName: *const u16) -> HCURSOR;
    pub fn SetCursor(hCursor: HCURSOR) -> HCURSOR;
    pub fn LoadIconW(hInstance: HINSTANCE, lpIconName: *const u16) -> HICON;
    pub fn LoadImageW(
        hInst: HINSTANCE,
        name: *const u16,
        type_: UINT,
        cx: INT,
        cy: INT,
        fuLoad: UINT,
    ) -> HANDLE;
    pub fn CreateIconIndirect(piconinfo: *const ICONINFO) -> HICON;
    pub fn DestroyIcon(hIcon: HICON) -> BOOL;
    pub fn SetCapture(hWnd: HWND) -> HWND;
    pub fn ReleaseCapture() -> BOOL;
    pub fn MessageBoxW(hWnd: HWND, lpText: *const u16, lpCaption: *const u16, uType: UINT) -> INT;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(hMenu: HMENU, uFlags: UINT, uIDNewItem: usize, lpNewItem: *const u16) -> BOOL;
    pub fn TrackPopupMenu(
        hMenu: HMENU,
        uFlags: UINT,
        x: INT,
        y: INT,
        nReserved: INT,
        hWnd: HWND,
        prcRect: *const RECT,
    ) -> BOOL;
    pub fn DestroyMenu(hMenu: HMENU) -> BOOL;
    pub fn GetCursorPos(lpPoint: *mut POINT) -> BOOL;
    pub fn ScreenToClient(hWnd: HWND, lpPoint: *mut POINT) -> BOOL;
    pub fn MoveWindow(hWnd: HWND, X: INT, Y: INT, nWidth: INT, nHeight: INT, bRepaint: BOOL) -> BOOL;
    pub fn SetWindowTextW(hWnd: HWND, lpString: *const u16) -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateCompatibleDC(hdc: HDC) -> HDC;
    pub fn DeleteDC(hdc: HDC) -> BOOL;
    pub fn CreateCompatibleBitmap(hdc: HDC, cx: INT, cy: INT) -> HBITMAP;
    pub fn CreateBitmap(
        nWidth: INT,
        nHeight: INT,
        nPlanes: UINT,
        nBitCount: UINT,
        lpBits: *const c_void,
    ) -> HBITMAP;
    pub fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
    pub fn DeleteObject(ho: HGDIOBJ) -> BOOL;
    pub fn GetStockObject(i: INT) -> HGDIOBJ;
    pub fn CreateSolidBrush(color: DWORD) -> HBRUSH;
    pub fn CreatePen(iStyle: INT, cWidth: INT, color: DWORD) -> HPEN;
    pub fn CreateFontW(
        cHeight: INT,
        cWidth: INT,
        cEscapement: INT,
        cOrientation: INT,
        cWeight: INT,
        bItalic: DWORD,
        bUnderline: DWORD,
        bStrikeOut: DWORD,
        iCharSet: DWORD,
        iOutPrecision: DWORD,
        iClipPrecision: DWORD,
        iQuality: DWORD,
        iPitchAndFamily: DWORD,
        pszFaceName: *const u16,
    ) -> HFONT;
    pub fn SetBkMode(hdc: HDC, mode: INT) -> INT;
    pub fn SetBkColor(hdc: HDC, color: DWORD) -> DWORD;
    pub fn SetTextColor(hdc: HDC, color: DWORD) -> DWORD;
    pub fn SetStretchBltMode(hdc: HDC, mode: INT) -> INT;
    pub fn StretchDIBits(
        hdc: HDC,
        xDest: INT,
        yDest: INT,
        DestWidth: INT,
        DestHeight: INT,
        xSrc: INT,
        ySrc: INT,
        SrcWidth: INT,
        SrcHeight: INT,
        lpBits: *const c_void,
        lpbmi: *const BITMAPINFO,
        iUsage: UINT,
        rop: DWORD,
    ) -> INT;
    pub fn BitBlt(
        hdc: HDC,
        x: INT,
        y: INT,
        cx: INT,
        cy: INT,
        hdcSrc: HDC,
        x1: INT,
        y1: INT,
        rop: DWORD,
    ) -> BOOL;
    pub fn RoundRect(hdc: HDC, left: INT, top: INT, right: INT, bottom: INT, width: INT, height: INT) -> BOOL;
}

#[link(name = "comdlg32")]
extern "system" {
    pub fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> BOOL;
}

#[link(name = "gdiplus")]
extern "system" {
    pub fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut c_void,
    ) -> i32;
    pub fn GdiplusShutdown(token: usize);
    pub fn GdipCreateBitmapFromFile(filename: *const u16, bitmap: *mut *mut c_void) -> i32;
    pub fn GdipGetImageWidth(image: *mut c_void, width: *mut u32) -> i32;
    pub fn GdipGetImageHeight(image: *mut c_void, height: *mut u32) -> i32;
    pub fn GdipBitmapLockBits(
        bitmap: *mut c_void,
        rect: *const GpRect,
        flags: u32,
        format: i32,
        lockedBitmapData: *mut BitmapData,
    ) -> i32;
    pub fn GdipBitmapUnlockBits(bitmap: *mut c_void, lockedBitmapData: *mut BitmapData) -> i32;
    pub fn GdipDisposeImage(image: *mut c_void) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    pub fn Shell_NotifyIconW(dwMessage: DWORD, lpData: *const NOTIFYICONDATAW) -> BOOL;
    pub fn ShellExecuteW(
        hwnd: HWND,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: INT,
    ) -> HINSTANCE;
    pub fn IsUserAnAdmin() -> BOOL;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegOpenKeyExW(
        hKey: HKEY,
        lpSubKey: *const u16,
        ulOptions: DWORD,
        samDesired: DWORD,
        phkResult: *mut HKEY,
    ) -> LONG;
    pub fn RegCreateKeyExW(
        hKey: HKEY,
        lpSubKey: *const u16,
        Reserved: DWORD,
        lpClass: *const u16,
        dwOptions: DWORD,
        samDesired: DWORD,
        lpSecurityAttributes: *const c_void,
        phkResult: *mut HKEY,
        lpdwDisposition: *mut DWORD,
    ) -> LONG;
    pub fn RegQueryValueExW(
        hKey: HKEY,
        lpValueName: *const u16,
        lpReserved: *mut DWORD,
        lpType: *mut DWORD,
        lpData: *mut BYTE,
        lpcbData: *mut DWORD,
    ) -> LONG;
    pub fn RegSetValueExW(
        hKey: HKEY,
        lpValueName: *const u16,
        Reserved: DWORD,
        dwType: DWORD,
        lpData: *const BYTE,
        cbData: DWORD,
    ) -> LONG;
    pub fn RegDeleteValueW(hKey: HKEY, lpValueName: *const u16) -> LONG;
    pub fn RegCloseKey(hKey: HKEY) -> LONG;
}

/// Decodes any `.jpg`, `.jpeg`, `.png`, or `.bmp` file into top-down `BGRA32` pixels via native Windows GDI+.
pub fn decode_image_file_gdiplus(path: &std::path::Path) -> Option<(Vec<u8>, u32, u32)> {
    if !path.is_file() {
        return None;
    }
    unsafe {
        let mut token: usize = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: std::ptr::null(),
            SuppressBackgroundThread: FALSE,
            SuppressExternalCodecs: FALSE,
        };
        if GdiplusStartup(&mut token, &input, std::ptr::null_mut()) != 0 {
            return None;
        }

        let wpath = to_wstring(&path.to_string_lossy());
        let mut bmp: *mut c_void = std::ptr::null_mut();
        let status = GdipCreateBitmapFromFile(wpath.as_ptr(), &mut bmp);
        if status != 0 || bmp.is_null() {
            GdiplusShutdown(token);
            return None;
        }

        let mut width: u32 = 0;
        let mut height: u32 = 0;
        let _ = GdipGetImageWidth(bmp, &mut width);
        let _ = GdipGetImageHeight(bmp, &mut height);

        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            GdipDisposeImage(bmp);
            GdiplusShutdown(token);
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
            GdipDisposeImage(bmp);
            GdiplusShutdown(token);
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
            // Force opaque alpha on decoded background
            for px in dst_row.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }

        let _ = GdipBitmapUnlockBits(bmp, &mut bdata);
        let _ = GdipDisposeImage(bmp);
        GdiplusShutdown(token);
        Some((out, width, height))
    }
}

/// Opens the native Windows file picker (`GetOpenFileNameW`) for `.jpg`, `.jpeg`, `.png`, `.bmp` images.
pub fn open_background_file_dialog(hwnd_owner: HWND, initial_dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    unsafe {
        // Filter string uses embedded NULs: "Immagini Sfondo (*.jpg;*.jpeg;*.png;*.bmp)\0*.jpg;*.jpeg;*.png;*.bmp\0Tutti i file (*.*)\0*.*\0\0"
        let filter_str = "Immagini Sfondo (*.jpg;*.jpeg;*.png;*.bmp)\0*.jpg;*.jpeg;*.png;*.bmp\0Tutti i file (*.*)\0*.*\0\0";
        let filter_w: Vec<u16> = filter_str.encode_utf16().collect();
        let title_w = to_wstring("Scegli Immagine di Sfondo per SmartCam (.jpg, .png, .bmp)");
        let init_dir_w = initial_dir.map(|p| to_wstring(&p.to_string_lossy()));

        let mut file_buf = vec![0u16; 1024];
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as DWORD;
        ofn.hwndOwner = hwnd_owner;
        ofn.lpstrFilter = filter_w.as_ptr();
        ofn.nFilterIndex = 1;
        ofn.lpstrFile = file_buf.as_mut_ptr();
        ofn.nMaxFile = file_buf.len() as DWORD;
        ofn.lpstrInitialDir = init_dir_w.as_ref().map(|v| v.as_ptr()).unwrap_or(std::ptr::null());
        ofn.lpstrTitle = title_w.as_ptr();
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER;

        if GetOpenFileNameW(&mut ofn) != 0 {
            let s = from_wstring_slice(&file_buf);
            if !s.is_empty() {
                return Some(std::path::PathBuf::from(s));
            }
        }
        None
    }
}

#[link(name = "user32")]
extern "system" {
    pub fn OpenClipboard(hWndNewOwner: HWND) -> BOOL;
    pub fn CloseClipboard() -> BOOL;
    pub fn GetClipboardData(uFormat: UINT) -> HANDLE;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GlobalLock(hMem: HANDLE) -> *mut c_void;
    pub fn GlobalUnlock(hMem: HANDLE) -> BOOL;
}

/// Reads UTF-16 text (`CF_UNICODETEXT = 13`) from the Windows Clipboard.
pub fn read_clipboard_text(hwnd_owner: HWND) -> Option<String> {
    const CF_UNICODETEXT: UINT = 13;
    unsafe {
        if OpenClipboard(hwnd_owner) == 0 {
            return None;
        }
        let hdata = GetClipboardData(CF_UNICODETEXT);
        if hdata.is_null() {
            let _ = CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(hdata) as *const u16;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }
        let mut len = 0usize;
        while len < 512 && *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        let text = String::from_utf16_lossy(slice).trim().to_string();
        let _ = GlobalUnlock(hdata);
        let _ = CloseClipboard();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}


