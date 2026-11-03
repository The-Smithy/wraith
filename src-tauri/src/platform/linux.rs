//! Linux support, built on `/proc`, signals and X11.
//!
//! Wayland doesn't let an app see other apps' windows, so window detection only
//! covers X11 and apps running through XWayland. `windows_fully_known` tells the rest
//! of the app when a missing window proves nothing.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

use super::procfs;
use super::{FileInfo, Listener, PerfInfo, ProcStats, Service, TcpState};

pub const PLATFORM: &str = "linux";
pub const SYSTEM_APP_LABEL: &str = "Linux";
pub const SERVICES_APP_LABEL: &str = "System services";
pub const SYSTEM_ROLE: &str = "System component";
pub const KERNEL_POOL_NAME: &str = "Unreclaimable kernel memory";

const SOCKET_MAP_MAX_AGE: Duration = Duration::from_secs(10);
const SOCKET_MAP_MIN_AGE: Duration = Duration::from_secs(2);
const TERMINATE_GRACE: Duration = Duration::from_secs(3);

/// Every process ID currently in `/proc`.
fn pids() -> impl Iterator<Item = u32> {
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
}

pub fn proc_stats(pid: u32) -> Option<ProcStats> {
    // Readable for every process, so memory is known even for other users' processes.
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let memory = procfs::parse_status(&status);
    // Readable only with permission over the process, which is also what ending it takes.
    let handles = fs::read_dir(format!("/proc/{pid}/fd")).ok().map(|dir| dir.count() as u32);
    Some(ProcStats {
        // Anonymous memory is what a leak grows. File pages can be dropped and re-read.
        private: memory.rss_anon + memory.swap,
        working_set: memory.rss,
        private_ws: memory.rss_anon,
        handles: handles.unwrap_or(0),
        gdi: 0,
        user_objects: 0,
        accessible: handles.is_some(),
    })
}

pub fn thread_counts() -> HashMap<u32, u32> {
    pids()
        .filter_map(|pid| {
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            Some((pid, procfs::parse_stat_threads(&stat)?))
        })
        .collect()
}

pub fn perf_info() -> PerfInfo {
    let meminfo = fs::read_to_string("/proc/meminfo").map(|t| procfs::parse_meminfo(&t)).unwrap_or_default();
    // The kernel only enforces the commit limit in mode 2. Otherwise going past it
    // is normal, so reporting it would raise a false alarm.
    let enforced = fs::read_to_string("/proc/sys/vm/overcommit_memory").is_ok_and(|mode| mode.trim() == "2");
    let handle_count = fs::read_to_string("/proc/sys/fs/file-nr")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    PerfInfo {
        commit_total: meminfo.committed,
        commit_limit: if enforced { meminfo.commit_limit } else { 0 },
        system_cache: meminfo.cached + meminfo.buffers,
        kernel_paged: meminfo.reclaimable,
        kernel_nonpaged: meminfo.unreclaimable,
        process_count: pids().count() as u32,
        thread_count: fs::read_to_string("/proc/loadavg").map_or(0, |t| procfs::parse_loadavg_threads(&t)),
        handle_count,
    }
}

struct SocketMap {
    owners: HashMap<u64, u32>,
    built: Instant,
}

/// Finding who owns a socket means reading every process's open files, which is too
/// slow for each tick. The map is rebuilt periodically, and sooner for a new listener.
static SOCKETS: Mutex<Option<SocketMap>> = Mutex::new(None);

fn build_socket_map() -> HashMap<u64, u32> {
    let mut owners = HashMap::new();
    for pid in pids() {
        let Ok(dir) = fs::read_dir(format!("/proc/{pid}/fd")) else { continue };
        for entry in dir.flatten() {
            if let Some(inode) = fs::read_link(entry.path()).ok().and_then(|l| l.to_str().and_then(procfs::socket_inode)) {
                owners.insert(inode, pid);
            }
        }
    }
    owners
}

pub fn tcp_state() -> TcpState {
    let mut rows = Vec::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(text) = fs::read_to_string(path) {
            rows.extend(procfs::parse_tcp(&text));
        }
    }

    let mut guard = SOCKETS.lock().unwrap_or_else(|e| e.into_inner());
    let stale = guard.as_ref().is_none_or(|map| {
        let age = map.built.elapsed();
        let new_listener = rows.iter().any(|r| r.state == procfs::TCP_LISTEN && !map.owners.contains_key(&r.inode));
        age >= SOCKET_MAP_MAX_AGE || (new_listener && age >= SOCKET_MAP_MIN_AGE)
    });
    if stale {
        *guard = Some(SocketMap { owners: build_socket_map(), built: Instant::now() });
    }
    let owners = guard.as_ref().map(|map| &map.owners);
    let owner = |inode: u64| owners.and_then(|o| o.get(&inode)).copied();

    let mut state = TcpState::default();
    for row in rows {
        match row.state {
            procfs::TCP_LISTEN => state.listeners.push(Listener {
                port: row.port,
                address: row.address,
                exposed: !row.loopback,
                // Zero when it belongs to another user, which reads as unknown.
                pid: owner(row.inode).unwrap_or(0),
            }),
            procfs::TCP_ESTABLISHED => {
                if let Some(pid) = owner(row.inode) {
                    *state.connections_by_pid.entry(pid).or_default() += 1;
                }
                *state.connections_by_port.entry(row.port).or_default() += 1;
            }
            _ => {}
        }
    }
    state
}

pub fn running_services() -> Vec<Service> {
    pids()
        .filter_map(|pid| {
            let cgroup = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
            let unit = procfs::service_from_cgroup(&cgroup)?;
            Some(Service { name: unit.clone(), display_name: unit, pid })
        })
        .collect()
}

/// True when a process with no window found really has none.
pub fn windows_fully_known() -> bool {
    std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

/// Window titles per process, as far as X11 can see them.
pub fn visible_windows() -> HashMap<u32, String> {
    x11_windows().unwrap_or_default()
}

fn x11_windows() -> Option<HashMap<u32, String>> {
    std::env::var_os("DISPLAY")?;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let atom = |name: &[u8]| conn.intern_atom(false, name).ok()?.reply().ok().map(|r| r.atom);
    let client_list = atom(b"_NET_CLIENT_LIST")?;
    let wm_pid = atom(b"_NET_WM_PID")?;
    let wm_name = atom(b"_NET_WM_NAME")?;

    let list = conn.get_property(false, root, client_list, AtomEnum::WINDOW, 0, 4096).ok()?.reply().ok()?;
    let mut out: HashMap<u32, String> = HashMap::new();
    for window in list.value32()? {
        let Some(pid) = conn
            .get_property(false, window, wm_pid, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next())
        else {
            continue;
        };
        let title = conn
            .get_property(false, window, wm_name, AtomEnum::ANY, 0, 512)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.value).into_owned())
            .unwrap_or_default();
        let slot = out.entry(pid).or_default();
        if title.len() > slot.len() {
            *slot = title;
        }
    }
    Some(out)
}

/// Program name to display name, from the installed `.desktop` files.
fn desktop_names() -> &'static HashMap<String, String> {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut folders = vec![
            "/usr/share/applications".to_string(),
            "/usr/local/share/applications".to_string(),
            "/var/lib/flatpak/exports/share/applications".to_string(),
            "/var/lib/snapd/desktop/applications".to_string(),
        ];
        if let Some(home) = dirs::home_dir() {
            folders.push(home.join(".local/share/applications").to_string_lossy().into_owned());
        }
        let mut names = HashMap::new();
        for folder in folders {
            for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
                if entry.path().extension().is_some_and(|e| e == "desktop") {
                    if let Some((program, name)) = fs::read_to_string(entry.path()).ok().and_then(|t| procfs::parse_desktop_entry(&t)) {
                        names.entry(program).or_insert(name);
                    }
                }
            }
        }
        names
    })
}

/// Linux executables carry no names of their own, so these come from `.desktop` files.
/// There is no general way to learn the publisher.
pub fn file_info(path: &str) -> FileInfo {
    let program = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    FileInfo {
        description: desktop_names().get(&program).cloned(),
        company: None,
    }
}

/// Unused on Linux, where paths need no translating. Exists to match Windows.
pub fn drive_devices() -> Vec<(String, String)> {
    Vec::new()
}

/// A process's executable. Other users' links are unreadable, but anyone can read the
/// command line, whose first word is usually the same path.
pub fn image_path(pid: u32, _drives: &[(String, String)]) -> Option<String> {
    if let Ok(link) = fs::read_link(format!("/proc/{pid}/exe")) {
        let path = link.to_string_lossy();
        return Some(path.strip_suffix(" (deleted)").unwrap_or(&path).to_string());
    }
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let first = cmdline.split(|b| *b == 0).next()?;
    let first = String::from_utf8_lossy(first);
    first.starts_with('/').then(|| first.into_owned())
}

/// sysinfo reports start times for every process on Linux, so there is no fallback.
pub fn start_time(_pid: u32) -> Option<u64> {
    None
}

/// Gone, or a zombie waiting for its parent to collect it. Either way it has stopped.
fn has_stopped(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => procfs::parse_stat_state(&stat) == Some('Z'),
        Err(_) => true,
    }
}

/// Asks the process to quit so it can clean up, and forces it if it hasn't after a
/// few seconds. This can block, so only call it from off the main thread.
pub fn terminate(pid: u32) -> Result<(), String> {
    let target = libc::pid_t::try_from(pid).map_err(|e| e.to_string())?;
    // SAFETY: kill takes plain integers and touches no memory.
    if unsafe { libc::kill(target, libc::SIGTERM) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let started = Instant::now();
    while started.elapsed() < TERMINATE_GRACE {
        if has_stopped(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: as above.
    if unsafe { libc::kill(target, libc::SIGKILL) } != 0 && !has_stopped(pid) {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

/// Opens the file manager at `path`, or at its folder if it is a file. Unlike Windows,
/// there is no portable way to select the file itself.
pub fn reveal(path: &Path) -> Result<(), String> {
    let folder = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    std::process::Command::new("xdg-open")
        .arg(folder)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn is_elevated() -> bool {
    // SAFETY: geteuid takes no arguments and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::process::{Command, Stdio};

    #[test]
    fn this_process_reports_real_stats() {
        let stats = proc_stats(std::process::id()).expect("own stats");
        assert!(stats.private > 0 && stats.working_set >= stats.private_ws);
        assert!(stats.accessible && stats.handles > 0);
        assert!(thread_counts()[&std::process::id()] >= 1);
    }

    #[test]
    fn this_process_can_be_located() {
        let path = image_path(std::process::id(), &[]).expect("own path");
        assert!(Path::new(&path).exists(), "{path}");
    }

    #[test]
    fn system_memory_figures_are_readable() {
        let info = perf_info();
        assert!(info.process_count > 0 && info.thread_count > 0);
        assert!(info.commit_total > 0 && info.kernel_nonpaged > 0);
    }

    #[test]
    fn a_listening_socket_is_tied_to_its_process() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::sleep(SOCKET_MAP_MIN_AGE);
        let state = tcp_state();
        let found = state.listeners.iter().find(|l| l.port == port).expect("listener found");
        assert_eq!(found.pid, std::process::id());
        assert!(!found.exposed);
    }

    #[test]
    fn terminate_stops_a_process_that_exits_on_request() {
        let mut child = Command::new("sleep").arg("300").stdout(Stdio::null()).spawn().unwrap();
        let started = Instant::now();
        terminate(child.id()).unwrap();
        assert!(started.elapsed() < TERMINATE_GRACE, "should not have needed force");
        assert!(child.wait().is_ok());
    }

    #[test]
    fn terminate_forces_a_process_that_ignores_the_request() {
        let mut child = Command::new("sh")
            .args(["-c", "trap '' TERM; while :; do sleep 1; done"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        terminate(child.id()).unwrap();
        assert!(child.wait().is_ok());
    }

    #[test]
    fn terminate_reports_a_missing_process() {
        assert!(terminate(u32::try_from(libc::pid_t::MAX).unwrap()).is_err());
    }
}
