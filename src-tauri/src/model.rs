//! Data shared between the sampler and the UI. Everything here crosses the IPC
//! boundary as camelCase JSON, mirrored by `src/types.ts`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Editor,
    LanguageServer,
    DevServer,
    BuildTool,
    TestRunner,
    PackageManager,
    Runtime,
    AiAgent,
    McpServer,
    Database,
    Container,
    Terminal,
    VersionControl,
    Browser,
    App,
    Service,
    System,
    Other,
}

impl Category {
    /// Tooling that exists to support development, as opposed to apps the user runs.
    pub fn is_dev(self) -> bool {
        matches!(
            self,
            Category::LanguageServer
                | Category::DevServer
                | Category::BuildTool
                | Category::TestRunner
                | Category::PackageManager
                | Category::Runtime
                | Category::AiAgent
                | Category::McpServer
                | Category::Database
                | Category::Container
                | Category::VersionControl
        )
    }
}

/// How a process relates to the desktop.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Presence {
    /// Owns a visible top level window.
    Window,
    /// Hosts one or more Windows services.
    Service,
    /// Windowless, but part of an app's process tree.
    Child,
    /// Windowless and not tied to any running app.
    Background,
    /// Not tied to an app, and the OS can't say if it has a window. Wayland doesn't tell.
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub key: String,
    pub pid: u32,
    pub parent_key: Option<String>,
    pub parent_gone: bool,
    pub name: String,
    pub exe: String,
    pub cmd_short: String,
    pub start_time: u64,
    /// False when Windows would not report a start time, so uptime is unknown.
    pub start_known: bool,
    pub user: String,
    pub category: Category,
    /// A Windows component. Running as administrator, nothing else would stop these being ended.
    pub protected: bool,
    pub role: String,
    pub tech: Option<String>,
    pub app_label: String,
    pub app_key: String,
    pub project: Option<Project>,
    pub services: Vec<String>,
    pub window_title: Option<String>,
    pub presence: Presence,
    pub limited: bool,
    /// Memory can be known even when the process is otherwise off limits, as on Linux.
    pub memory_known: bool,
    /// Committed private bytes. This is the number that matters for leaks.
    pub private: u64,
    /// Full working set, including pages shared with other processes.
    pub working_set: u64,
    /// Private working set, the figure Task Manager shows as "Memory".
    pub private_ws: u64,
    pub cpu: f32,
    pub read_rate: u64,
    pub write_rate: u64,
    pub handles: u32,
    pub threads: u32,
    pub gdi: u32,
    pub user_objects: u32,
    pub ports: Vec<u16>,
    pub connections: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDetail {
    pub key: String,
    pub cmd: String,
    pub cwd: String,
    pub exe: String,
    pub description: Option<String>,
    pub publisher: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppSummary {
    pub label: String,
    pub category: Category,
    pub process_count: u32,
    pub instance_count: u32,
    pub private: u64,
    pub working_set: u64,
    pub private_ws: u64,
    pub cpu: f32,
    pub handles: u32,
    pub threads: u32,
    pub trend: Vec<u32>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub port: u16,
    pub address: String,
    pub exposed: bool,
    pub pid: u32,
    pub key: Option<String>,
    pub process_name: String,
    pub role: String,
    pub tech: Option<String>,
    pub app_label: String,
    pub project: Option<Project>,
    pub category: Category,
    pub connections: u32,
    pub private: u64,
    pub uptime_secs: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub pid: u32,
    pub key: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub total_memory: u64,
    pub used_memory: u64,
    pub available_memory: u64,
    pub commit_total: u64,
    pub commit_limit: u64,
    pub system_cache: u64,
    pub kernel_paged: u64,
    pub kernel_nonpaged: u64,
    pub cpu: f32,
    pub cpu_count: u32,
    pub process_count: u32,
    pub thread_count: u32,
    pub handle_count: u32,
    pub uptime_secs: u64,
    pub limited_processes: u32,
    pub elevated: bool,
    pub host_name: String,
    pub platform: String,
    pub windows_known: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Memory,
    Handles,
    Threads,
    Gdi,
    UserObjects,
    NonpagedPool,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Plateaued,
    Watching,
    Low,
    Medium,
    High,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    SteadyClimb,
    StairStep,
    SawtoothRisingFloor,
    Plateau,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SubjectKind {
    Process,
    App,
    System,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LeakReport {
    pub id: String,
    pub subject: SubjectKind,
    pub key: Option<String>,
    pub name: String,
    pub role: String,
    pub app_label: String,
    pub project: Option<Project>,
    pub metric: Metric,
    pub confidence: Confidence,
    pub pattern: Pattern,
    pub rate_per_hour: f64,
    pub growth: f64,
    pub growth_pct: f64,
    pub start_value: f64,
    pub current_value: f64,
    pub observed_secs: u64,
    pub z_score: f64,
    pub monotonicity: f64,
    pub hours_to_exhaustion: Option<f64>,
    pub series: Vec<[f64; 2]>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    /// What to do about it, in a few words.
    pub action: String,
    pub reclaimable: u64,
    pub keys: Vec<String>,
}

fn known() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExitedInfo {
    pub key: String,
    pub pid: u32,
    pub name: String,
    pub role: String,
    pub app_label: String,
    pub project: Option<Project>,
    pub category: Category,
    pub started: u64,
    #[serde(default = "known")]
    pub start_known: bool,
    pub ended: u64,
    pub peak_private: u64,
    pub identity: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SelfStats {
    pub private: u64,
    pub cpu: f32,
    pub sample_ms: u32,
    pub tracked_series: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub seq: u64,
    pub t: u64,
    pub monitor_since: u64,
    pub system: SystemInfo,
    pub processes: Vec<ProcessInfo>,
    pub apps: Vec<AppSummary>,
    pub ports: Vec<PortInfo>,
    pub services: Vec<ServiceInfo>,
    pub leaks: Vec<LeakReport>,
    pub insights: Vec<Insight>,
    pub exited: Vec<ExitedInfo>,
    pub self_stats: SelfStats,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub interval_secs: u32,
    pub bucket_secs: u32,
    pub leak_min_minutes: u32,
    pub leak_warmup_minutes: u32,
    pub notify_leaks: bool,
    pub notify_pressure: bool,
    pub pressure_pct: u32,
    pub close_to_tray: bool,
    pub ignored: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            interval_secs: 2,
            bucket_secs: 30,
            leak_min_minutes: 30,
            leak_warmup_minutes: 10,
            notify_leaks: true,
            notify_pressure: true,
            pressure_pct: 90,
            close_to_tray: true,
            ignored: Vec::new(),
        }
    }
}

impl Settings {
    /// Keeps values from a hand edited file or the UI inside sensible limits.
    pub fn sanitized(mut self) -> Self {
        self.interval_secs = self.interval_secs.clamp(1, 60);
        self.bucket_secs = self.bucket_secs.clamp(self.interval_secs, 600);
        self.leak_min_minutes = self.leak_min_minutes.clamp(1, 1440);
        self.leak_warmup_minutes = self.leak_warmup_minutes.min(240);
        self.pressure_pct = self.pressure_pct.clamp(50, 99);
        self
    }
}
