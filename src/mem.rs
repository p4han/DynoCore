//! Memory engine: reads RAM use and cleans it the way Mem Reduct does.
use core::ffi::c_void;
use std::mem::{size_of, transmute, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND, LUID, RECT};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::SetSystemFileCacheSize;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXSCREEN, SM_CYSCREEN,
};

/// Area order everywhere: 0 working sets, 1 system cache, 2 modified pages, 3 standby list.
#[allow(dead_code)]
pub const AREA_NAMES: [&str; 4] = ["Working sets", "System cache", "Modified pages", "Standby list"];

const MEMORY_EMPTY_WORKING_SETS: i32 = 2;
const MEMORY_FLUSH_MODIFIED_LIST: i32 = 3;
const MEMORY_PURGE_STANDBY_LIST: i32 = 4;
const SYSTEM_MEMORY_LIST_INFORMATION: i32 = 80;

type NtSetSystemInformation = unsafe extern "system" fn(i32, *mut c_void, u32) -> i32;

/// (bytes in use, total bytes), counted like Task Manager: total minus available.
pub fn used() -> (u64, u64) {
    unsafe {
        let mut m: MEMORYSTATUSEX = zeroed();
        m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) == 0 {
            return (0, 1);
        }
        (m.ullTotalPhys.saturating_sub(m.ullAvailPhys), m.ullTotalPhys)
    }
}

fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut token: HANDLE = 0 as _;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut luid: LUID = zeroed();
        let wname = crate::wide(name);
        if LookupPrivilegeValueW(null(), wname.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return false;
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        let ok = AdjustTokenPrivileges(token, 0, &tp, 0, null_mut(), null_mut()) != 0;
        let err = GetLastError(); // 1300 = privilege not held
        CloseHandle(token);
        ok && err == 0
    }
}

fn nt_set() -> Option<NtSetSystemInformation> {
    unsafe {
        let ntdll = GetModuleHandleW(crate::wide("ntdll.dll").as_ptr());
        if (ntdll as usize) == 0 {
            return None;
        }
        let p = GetProcAddress(ntdll, b"NtSetSystemInformation\0".as_ptr());
        p.map(|f| transmute::<unsafe extern "system" fn() -> isize, NtSetSystemInformation>(f))
    }
}

fn memory_command(cmd: i32) -> bool {
    match nt_set() {
        Some(f) => {
            let mut c: i32 = cmd;
            unsafe { f(SYSTEM_MEMORY_LIST_INFORMATION, &mut c as *mut i32 as *mut c_void, 4) >= 0 }
        }
        None => false,
    }
}

fn flush_file_cache() -> bool {
    unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) != 0 }
}

/// Cleans the chosen areas in the order that frees the most:
/// working sets, file cache, modified pages, then standby.
/// Returns Some(true/false) for areas that were tried and None for areas that were skipped.
pub fn clean(areas: [bool; 4]) -> [Option<bool>; 4] {
    enable_privilege("SeProfileSingleProcessPrivilege");
    enable_privilege("SeIncreaseQuotaPrivilege");
    let mut out = [None; 4];
    if areas[0] {
        out[0] = Some(memory_command(MEMORY_EMPTY_WORKING_SETS));
    }
    if areas[1] {
        out[1] = Some(flush_file_cache());
    }
    if areas[2] {
        out[2] = Some(memory_command(MEMORY_FLUSH_MODIFIED_LIST));
    }
    if areas[3] {
        out[3] = Some(memory_command(MEMORY_PURGE_STANDBY_LIST));
    }
    out
}

/// Merges identical pages using the Windows CombineMemoryPages API if available.
pub fn combine_memory_pages() -> bool {
    unsafe {
        let k32 = GetModuleHandleW(crate::wide("kernel32.dll").as_ptr());
        if (k32 as usize) == 0 {
            return false;
        }
        let p = GetProcAddress(k32, b"CombineMemoryPages\0".as_ptr());
        if let Some(f) = p {
            type CombinePagesFn = unsafe extern "system" fn(HANDLE, *const c_void, usize, *mut usize, *mut c_void) -> i32;
            let func: CombinePagesFn = transmute(f);
            let mut combined: usize = 0;
            return func(GetCurrentProcess(), null(), 0, &mut combined, null_mut()) != 0;
        }
        false
    }
}

/// True when the window in front covers the whole screen (usually a game or a video).
#[allow(dead_code)]
pub fn fullscreen_app(own: HWND) -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if (h as usize) == 0 || h == own {
            return false;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(h, buf.as_mut_ptr(), 64);
        let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        if class == "Progman" || class == "WorkerW" || class == "Shell_TrayWnd" {
            return false; // the desktop itself
        }
        let mut r: RECT = zeroed();
        if GetWindowRect(h, &mut r) == 0 {
            return false;
        }
        r.left <= 0 && r.top <= 0 && r.right >= GetSystemMetrics(SM_CXSCREEN) && r.bottom >= GetSystemMetrics(SM_CYSCREEN)
    }
}
