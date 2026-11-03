//! Everything that asks the operating system a question. The rest of the app only
//! uses what is here, so each OS supplies the same functions and types.

use std::collections::HashMap;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
compile_error!("Wraith only supports Windows so far.");

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcStats {
    pub private: u64,
    pub working_set: u64,
    pub private_ws: u64,
    pub handles: u32,
    pub gdi: u32,
    pub user_objects: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PerfInfo {
    pub commit_total: u64,
    pub commit_limit: u64,
    pub system_cache: u64,
    pub kernel_paged: u64,
    pub kernel_nonpaged: u64,
    pub process_count: u32,
    pub thread_count: u32,
    pub handle_count: u32,
}

#[derive(Debug, Clone)]
pub struct Listener {
    pub port: u16,
    pub address: String,
    pub exposed: bool,
    pub pid: u32,
}

#[derive(Debug, Default)]
pub struct TcpState {
    pub listeners: Vec<Listener>,
    /// Established connections per owning process.
    pub connections_by_pid: HashMap<u32, u32>,
    /// Established connections per local port, used to see if a server has clients.
    pub connections_by_port: HashMap<u16, u32>,
}

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub display_name: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Default)]
pub struct FileInfo {
    pub description: Option<String>,
    pub company: Option<String>,
}
