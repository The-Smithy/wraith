//! Thin wrappers over the Win32 APIs that sysinfo does not cover: handle and GUI
//! object counts, TCP ownership, services, windows and file descriptions.

use std::collections::HashMap;
use std::ffi::c_void;
use std::net::{Ipv4Addr, Ipv6Addr};

use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, FILETIME, HANDLE, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6TABLE_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, QueryDosDeviceW, VerQueryValueW};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    K32GetPerformanceInfo, K32GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW,
    SC_ENUM_PROCESS_INFO, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_WIN32,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetGuiResources, GetProcessHandleCount, GetProcessTimes, OpenProcess, OpenProcessToken,
    TerminateProcess, GR_GDIOBJECTS, GR_USEROBJECTS, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, InternalGetWindowText, IsWindowVisible,
    GW_OWNER,
};

use super::{FileInfo, Listener, PerfInfo, ProcStats, Service, TcpState};

pub const PLATFORM: &str = "windows";
pub const SYSTEM_APP_LABEL: &str = "Windows";
pub const SERVICES_APP_LABEL: &str = "Windows Services";
pub const SYSTEM_ROLE: &str = "Windows component";
pub const KERNEL_POOL_NAME: &str = "Kernel non-paged pool";

/// True when a process with no window found really has none.
pub fn windows_fully_known() -> bool {
    true
}

/// Closes a Win32 handle when it goes out of scope.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful Win32 call and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// `PROCESS_MEMORY_COUNTERS_EX2`, which adds the private working set on Windows
/// 10 22H2 and later. Older systems reject the larger size.
#[repr(C)]
#[derive(Default)]
struct MemoryCountersEx2 {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_usage: usize,
    private_working_set_size: usize,
    shared_commit_usage: u64,
}

/// Reads live counters. `None` means the process refused access, which is normal
/// for elevated or protected processes when not running as administrator.
pub fn proc_stats(pid: u32) -> Option<ProcStats> {
    // SAFETY: every pointer passed below refers to a live local of the expected
    // size, and the process handle stays open until `Owned` drops.
    unsafe {
        let handle = Owned(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut stats = ProcStats { accessible: true, ..Default::default() };

        let mut counters = MemoryCountersEx2 {
            cb: std::mem::size_of::<MemoryCountersEx2>() as u32,
            ..Default::default()
        };
        let full = K32GetProcessMemoryInfo(
            handle.0,
            &mut counters as *mut MemoryCountersEx2 as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        );
        if full.as_bool() {
            stats.private = counters.private_usage as u64;
            stats.working_set = counters.working_set_size as u64;
            stats.private_ws = counters.private_working_set_size as u64;
        } else {
            // The EX layout is the same struct, cut off before the private working set.
            let cb = std::mem::offset_of!(MemoryCountersEx2, private_working_set_size) as u32;
            counters.cb = cb;
            let partial = K32GetProcessMemoryInfo(
                handle.0,
                &mut counters as *mut MemoryCountersEx2 as *mut PROCESS_MEMORY_COUNTERS,
                cb,
            );
            if !partial.as_bool() {
                return None;
            }
            stats.private = counters.private_usage as u64;
            stats.working_set = counters.working_set_size as u64;
            stats.private_ws = counters.working_set_size as u64;
        }

        let mut handles = 0u32;
        if GetProcessHandleCount(handle.0, &mut handles).is_ok() {
            stats.handles = handles;
        }
        stats.gdi = GetGuiResources(handle.0, GR_GDIOBJECTS);
        stats.user_objects = GetGuiResources(handle.0, GR_USEROBJECTS);
        Some(stats)
    }
}

/// Thread count per process, from a single toolhelp snapshot.
pub fn thread_counts() -> HashMap<u32, u32> {
    let mut out = HashMap::new();
    // SAFETY: the snapshot handle is owned for the whole walk and `entry.dwSize`
    // is set before the first call, as the API requires.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let snapshot = Owned(snapshot);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot.0, &mut entry).is_ok() {
            loop {
                out.insert(entry.th32ProcessID, entry.cntThreads);
                if Process32NextW(snapshot.0, &mut entry).is_err() {
                    break;
                }
            }
        }
    }
    out
}

pub fn perf_info() -> PerfInfo {
    let mut info = PERFORMANCE_INFORMATION::default();
    let size = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    // SAFETY: `info` is a live struct of exactly the size passed.
    if !unsafe { K32GetPerformanceInfo(&mut info, size) }.as_bool() {
        return PerfInfo::default();
    }
    let page = info.PageSize as u64;
    PerfInfo {
        commit_total: info.CommitTotal as u64 * page,
        commit_limit: info.CommitLimit as u64 * page,
        system_cache: info.SystemCache as u64 * page,
        kernel_paged: info.KernelPaged as u64 * page,
        kernel_nonpaged: info.KernelNonpaged as u64 * page,
        process_count: info.ProcessCount,
        thread_count: info.ThreadCount,
        handle_count: info.HandleCount,
    }
}

const STATE_LISTEN: u32 = 2;
const STATE_ESTABLISHED: u32 = 5;

/// Fetches an extended TCP table into a u32 buffer, which is aligned for the rows.
fn tcp_table(family: u32) -> Option<Vec<u32>> {
    let mut size = 0u32;
    // SAFETY: the first call only reports the size, the second writes at most
    // `size` bytes into a buffer at least that long.
    unsafe {
        let _ = GetExtendedTcpTable(None, &mut size, false, family, TCP_TABLE_OWNER_PID_ALL, 0);
        for _ in 0..4 {
            let mut buffer = vec![0u32; (size as usize / 4) + 64];
            let mut len = (buffer.len() * 4) as u32;
            let result = GetExtendedTcpTable(
                Some(buffer.as_mut_ptr() as *mut c_void),
                &mut len,
                false,
                family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if result == 0 {
                return Some(buffer);
            }
            if result != ERROR_INSUFFICIENT_BUFFER.0 {
                return None;
            }
            size = len;
        }
    }
    None
}

pub fn tcp_state() -> TcpState {
    let mut state = TcpState::default();

    if let Some(buffer) = tcp_table(AF_INET.0 as u32) {
        // SAFETY: the API filled the buffer with a count followed by that many rows.
        let rows = unsafe {
            let table = &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
            std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
        };
        for row in rows {
            let port = u16::from_be(row.dwLocalPort as u16);
            match row.dwState {
                STATE_LISTEN => {
                    let ip = Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes());
                    state.listeners.push(Listener {
                        port,
                        address: ip.to_string(),
                        exposed: !ip.is_loopback(),
                        pid: row.dwOwningPid,
                    });
                }
                STATE_ESTABLISHED => {
                    *state.connections_by_pid.entry(row.dwOwningPid).or_default() += 1;
                    *state.connections_by_port.entry(port).or_default() += 1;
                }
                _ => {}
            }
        }
    }

    if let Some(buffer) = tcp_table(AF_INET6.0 as u32) {
        // SAFETY: as above, for the IPv6 row layout.
        let rows = unsafe {
            let table = &*(buffer.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID);
            std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
        };
        for row in rows {
            let port = u16::from_be(row.dwLocalPort as u16);
            match row.dwState {
                STATE_LISTEN => {
                    let ip = Ipv6Addr::from(row.ucLocalAddr);
                    state.listeners.push(Listener {
                        port,
                        address: format!("[{ip}]"),
                        exposed: !ip.is_loopback(),
                        pid: row.dwOwningPid,
                    });
                }
                STATE_ESTABLISHED => {
                    *state.connections_by_pid.entry(row.dwOwningPid).or_default() += 1;
                    *state.connections_by_port.entry(port).or_default() += 1;
                }
                _ => {}
            }
        }
    }

    state
}

fn from_wide(ptr: PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: the pointer is a NUL terminated string owned by the buffer it came from.
    unsafe { ptr.to_string().unwrap_or_default() }
}

pub fn running_services() -> Vec<Service> {
    let mut out = Vec::new();
    // SAFETY: the service manager handle is closed below, and the rows are read
    // from a buffer that outlives them, after the API reports how many it wrote.
    unsafe {
        let Ok(manager) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE) else {
            return out;
        };
        let mut needed = 0u32;
        let mut count = 0u32;
        let mut resume = 0u32;
        let _ = EnumServicesStatusExW(
            manager,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            None,
            &mut needed,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        );
        let mut buffer = vec![0u64; (needed as usize / 8) + 512];
        let bytes = std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer.len() * 8);
        resume = 0;
        if EnumServicesStatusExW(
            manager,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            Some(bytes),
            &mut needed,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        )
        .is_ok()
        {
            let rows = std::slice::from_raw_parts(buffer.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, count as usize);
            for row in rows {
                let pid = row.ServiceStatusProcess.dwProcessId;
                if pid != 0 {
                    out.push(Service {
                        name: from_wide(row.lpServiceName),
                        display_name: from_wide(row.lpDisplayName),
                        pid,
                    });
                }
            }
        }
        let _ = CloseServiceHandle(manager);
    }
    out
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut HashMap` passed by `visible_windows`, which
    // stays alive and unaliased for the duration of `EnumWindows`.
    let map = &mut *(lparam.0 as *mut HashMap<u32, String>);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()) {
        return BOOL(1);
    }
    let mut cloaked = 0u32;
    let attr = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
    if attr.is_ok() && cloaked != 0 {
        return BOOL(1);
    }
    // Not GetWindowTextW: on this app's own window that sends a message to the UI
    // thread, which deadlocks if that thread is waiting on the sampler.
    let mut text = [0u16; 512];
    let written = InternalGetWindowText(hwnd, &mut text);
    if written <= 0 {
        return BOOL(1);
    }
    let title = String::from_utf16_lossy(&text[..written as usize]);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != 0 {
        let slot = map.entry(pid).or_default();
        if title.len() > slot.len() {
            *slot = title;
        }
    }
    BOOL(1)
}

/// Visible top level window title per process.
pub fn visible_windows() -> HashMap<u32, String> {
    let mut map: HashMap<u32, String> = HashMap::new();
    // SAFETY: the callback only uses this pointer while EnumWindows is running.
    unsafe {
        let _ = EnumWindows(Some(collect_window), LPARAM(&mut map as *mut HashMap<u32, String> as isize));
    }
    map
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// When a process started, in Unix seconds. None if Windows refuses access.
pub fn start_time(pid: u32) -> Option<u64> {
    const FILETIME_TO_UNIX_SECS: u64 = 11_644_473_600;
    // SAFETY: the handle is owned, and all four out pointers are live locals.
    unsafe {
        let handle = Owned(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let (mut created, mut exited, mut kernel, mut user) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user).ok()?;
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        (ticks / 10_000_000).checked_sub(FILETIME_TO_UNIX_SECS)
    }
}

pub fn terminate(pid: u32) -> Result<(), String> {
    // SAFETY: the handle is owned, and TerminateProcess only acts on that handle.
    unsafe {
        let handle = Owned(OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| e.message())?);
        TerminateProcess(handle.0, 1).map_err(|e| e.message())
    }
}

/// Opens the file manager with path selected, or at path if it is a folder.
pub fn reveal(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new("explorer");
    if path.is_dir() {
        command.arg(path);
    } else {
        command.raw_arg(format!("/select,\"{}\"", path.display()));
    }
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn is_elevated() -> bool {
    // SAFETY: the token handle is owned, and the output struct matches the class requested.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let token = Owned(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok()
            && elevation.TokenIsElevated != 0
    }
}

/// Names from an executable's version resource.
pub fn file_info(path: &str) -> FileInfo {
    let mut info = FileInfo::default();
    let path_w = wide(path);
    // SAFETY: `data` holds the whole version block, and strings returned by
    // VerQueryValueW point inside it and are copied out before it is freed.
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(path_w.as_ptr()), None);
        if size == 0 {
            return info;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(PCWSTR(path_w.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).is_err() {
            return info;
        }
        let block = data.as_ptr() as *const c_void;

        let mut translation: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let query = wide("\\VarFileInfo\\Translation");
        if !VerQueryValueW(block, PCWSTR(query.as_ptr()), &mut translation, &mut len).as_bool() || len < 4 {
            return info;
        }
        let codes = translation as *const u16;
        let (lang, page) = (*codes, *codes.add(1));

        let read = |field: &str| -> Option<String> {
            let query = wide(&format!("\\StringFileInfo\\{lang:04x}{page:04x}\\{field}"));
            let mut value: *mut c_void = std::ptr::null_mut();
            let mut chars = 0u32;
            if !VerQueryValueW(block, PCWSTR(query.as_ptr()), &mut value, &mut chars).as_bool() || chars == 0 {
                return None;
            }
            let slice = std::slice::from_raw_parts(value as *const u16, chars as usize);
            // The reported length can run past the terminator into the next entry.
            let end = slice.iter().position(|c| *c == 0).unwrap_or(slice.len());
            let text = String::from_utf16_lossy(&slice[..end]).trim().to_string();
            (!text.is_empty()).then_some(text)
        };
        info.description = read("FileDescription").or_else(|| read("ProductName"));
        info.company = read("CompanyName");
    }
    info
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct SystemProcessIdInformation {
    process_id: usize,
    image_name: UnicodeString,
}

const SYSTEM_PROCESS_ID_INFORMATION: u32 = 88;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(class: u32, info: *mut c_void, length: u32, returned: *mut u32) -> i32;
}

/// `\Device\HarddiskVolume3` style names for each drive letter.
pub fn drive_devices() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for letter in 'A'..='Z' {
        let drive = format!("{letter}:");
        let mut buffer = [0u16; 512];
        // SAFETY: both pointers are valid for the duration of the call.
        let written = unsafe { QueryDosDeviceW(PCWSTR(wide(&drive).as_ptr()), Some(&mut buffer)) } as usize;
        if written > 0 {
            let end = buffer[..written].iter().position(|c| *c == 0).unwrap_or(written);
            out.push((String::from_utf16_lossy(&buffer[..end]), drive));
        }
    }
    out
}

fn to_dos_path(nt_path: &str, drives: &[(String, String)]) -> Option<String> {
    drives.iter().find_map(|(device, drive)| {
        nt_path
            .strip_prefix(device.as_str())
            .filter(|rest| rest.starts_with('\\'))
            .map(|rest| format!("{drive}{rest}"))
    })
}

/// A process's executable path, without needing a handle to it. This is how
/// elevated processes that refuse OpenProcess can still be named and tied to an app.
pub fn image_path(pid: u32, drives: &[(String, String)]) -> Option<String> {
    let mut buffer = vec![0u16; 2048];
    let mut info = SystemProcessIdInformation {
        process_id: pid as usize,
        image_name: UnicodeString {
            length: 0,
            maximum_length: (buffer.len() * 2) as u16,
            buffer: buffer.as_mut_ptr(),
        },
    };
    // SAFETY: `info` matches the layout of this information class, and the buffer
    // it points at outlives the call and is the size it declares.
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESS_ID_INFORMATION,
            &mut info as *mut SystemProcessIdInformation as *mut c_void,
            std::mem::size_of::<SystemProcessIdInformation>() as u32,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return None;
    }
    let chars = (info.image_name.length / 2) as usize;
    to_dos_path(&String::from_utf16_lossy(&buffer[..chars]), drives)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_time_is_in_unix_seconds() {
        let started = start_time(std::process::id()).expect("own start time");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        assert!(started <= now && now - started < 3600, "{started} vs {now}");
    }

    #[test]
    fn nt_paths_become_drive_paths() {
        let drives = vec![
            (r"\Device\HarddiskVolume1".to_string(), "D:".to_string()),
            (r"\Device\HarddiskVolume10".to_string(), "C:".to_string()),
        ];
        assert_eq!(
            to_dos_path(r"\Device\HarddiskVolume10\Windows\x.exe", &drives).as_deref(),
            Some(r"C:\Windows\x.exe")
        );
        assert_eq!(to_dos_path(r"\Device\Other\x.exe", &drives), None);
    }

    #[test]
    fn this_process_can_be_located_without_a_handle() {
        let path = image_path(std::process::id(), &drive_devices()).expect("own image path");
        assert!(path.to_lowercase().ends_with(".exe"), "{path}");
        assert!(std::path::Path::new(&path).exists(), "{path}");
    }

    #[test]
    fn version_strings_stop_at_their_terminator() {
        let exe = std::env::var("SystemRoot").unwrap() + r"\explorer.exe";
        let info = file_info(&exe);
        assert_eq!(info.company.as_deref(), Some("Microsoft Corporation"));
        assert!(!info.description.unwrap_or_default().contains('\0'));
    }
}
