//! The tick: collect everything, relate it, update history, run the analyzers.
//! Holds no Tauri types, so it can be driven directly in tests.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, ThreadKind, UpdateKind, Users};

use crate::classify::classify;
use crate::history::{metric_points, Bucket, History, ProcHist, SysBucket};
use crate::insights;
use crate::leaks::{self, Point, Thresholds};
use crate::model::*;
use crate::platform;
use crate::relate::{detect_project, relate, Context, Meta, Raw};

const MB: f64 = 1024.0 * 1024.0;
const SERVICE_REFRESH_SECS: u64 = 30;
const USER_REFRESH_SECS: u64 = 300;
const CMD_SHORT_CHARS: usize = 220;
/// Memory leak checks skip processes below this. Tiny ones cannot hurt much.
const MIN_LEAK_PRIVATE: u64 = 50 * 1024 * 1024;

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub enum Notice {
    Leak(Box<LeakReport>),
    Pressure { pct: u64, available: u64 },
}

pub struct Sampler {
    sys: System,
    users: Option<Users>,
    users_at: u64,
    metas: HashMap<String, Meta>,
    /// Stand-in start times for processes that refuse to report one.
    first_seen: HashMap<u32, u64>,
    home: Option<PathBuf>,
    drives: Vec<(String, String)>,
    services: Vec<platform::Service>,
    services_at: u64,
    pub history: History,
    leaks: Vec<LeakReport>,
    insights: Vec<Insight>,
    leak_eval_at: u64,
    notified: HashSet<String>,
    pressure_notified: bool,
    previous: HashMap<String, ProcessInfo>,
    last_tick: Instant,
    elevated: bool,
    seq: u64,
    self_pid: u32,
}

impl Sampler {
    pub fn new(settings: &Settings, persisted: Option<&str>) -> Self {
        let now = now_secs();
        let mut history = History::new(u64::from(settings.bucket_secs), now);
        if let Some(json) = persisted {
            history.restore(json, now);
            history.span = u64::from(settings.bucket_secs);
        }
        Sampler {
            sys: System::new().expect("sysinfo supports this OS"),
            users: Users::new_with_refreshed_list().ok(),
            users_at: now,
            metas: HashMap::new(),
            first_seen: HashMap::new(),
            home: dirs::home_dir(),
            drives: platform::drive_devices(),
            services: Vec::new(),
            services_at: 0,
            history,
            leaks: Vec::new(),
            insights: Vec::new(),
            leak_eval_at: 0,
            notified: HashSet::new(),
            pressure_notified: false,
            previous: HashMap::new(),
            last_tick: Instant::now(),
            elevated: platform::is_elevated(),
            seq: 0,
            self_pid: std::process::id(),
        }
    }

    pub fn meta(&self, key: &str) -> Option<&Meta> {
        self.metas.get(key)
    }

    pub fn tick(&mut self, settings: &Settings) -> (Snapshot, Vec<Notice>) {
        let started = Instant::now();
        let now = now_secs();
        let elapsed = self.last_tick.elapsed().as_secs_f64().max(0.001);
        self.last_tick = Instant::now();

        self.sys.refresh_memory();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet),
        );

        if now.saturating_sub(self.users_at) >= USER_REFRESH_SECS {
            if let Some(users) = &mut self.users {
                users.refresh();
            }
            self.users_at = now;
        }
        if now.saturating_sub(self.services_at) >= SERVICE_REFRESH_SECS {
            self.services = platform::running_services();
            self.services_at = now;
        }

        let cpu_count = self.sys.cpus().len().max(1);
        let threads = platform::thread_counts();
        let raws = self.collect_raw(&threads, cpu_count, elapsed);

        let tcp = platform::tcp_state();
        let mut ctx = Context {
            windows: platform::visible_windows(),
            windows_unknown: !platform::windows_fully_known(),
            connections: tcp.connections_by_pid.clone(),
            ..Default::default()
        };
        for s in &self.services {
            ctx.services.entry(s.pid).or_default().push(s.display_name.clone());
        }
        for l in &tcp.listeners {
            let ports = ctx.ports.entry(l.pid).or_default();
            if !ports.contains(&l.port) {
                ports.push(l.port);
            }
        }
        for ports in ctx.ports.values_mut() {
            ports.sort_unstable();
        }

        let processes = relate(&raws, &self.metas, &ctx);
        let system = self.system_info(&raws, cpu_count);
        let ports = build_ports(&tcp, &processes, now);
        let services = self.build_services(&processes);

        self.track_exits(&processes, now);
        self.update_history(&processes, &system, now);
        let apps = self.build_apps(&processes);

        let mut notices = Vec::new();
        if now.saturating_sub(self.leak_eval_at) >= u64::from(settings.bucket_secs) {
            self.leak_eval_at = now;
            self.leaks = self.evaluate_leaks(&processes, &system, settings);
            for report in &self.leaks {
                if report.confidence >= Confidence::Medium && self.notified.insert(report.id.clone()) {
                    notices.push(Notice::Leak(Box::new(report.clone())));
                }
            }
            let idle: HashMap<String, u64> = processes
                .iter()
                .filter_map(|p| {
                    self.history.procs.get(&p.key).map(|h| (p.key.clone(), h.idle_secs(0.5, now)))
                })
                .collect();
            let exited: Vec<ExitedInfo> = self.history.exited.iter().cloned().collect();
            self.insights = insights::evaluate(&insights::Input {
                now,
                system: &system,
                processes: &processes,
                ports: &ports,
                exited: &exited,
                idle: &idle,
                pressure_pct: settings.pressure_pct,
            });
        }

        if let Some(pct) = (system.used_memory * 100).checked_div(system.total_memory) {
            if pct >= u64::from(settings.pressure_pct) {
                if !self.pressure_notified {
                    self.pressure_notified = true;
                    notices.push(Notice::Pressure { pct, available: system.available_memory });
                }
            } else if pct + 5 < u64::from(settings.pressure_pct) {
                self.pressure_notified = false;
            }
        }

        self.previous = processes.iter().map(|p| (p.key.clone(), p.clone())).collect();
        self.seq += 1;

        let own = processes.iter().find(|p| p.pid == self.self_pid);
        let self_stats = SelfStats {
            private: own.map_or(0, |p| p.private),
            cpu: own.map_or(0.0, |p| p.cpu),
            sample_ms: started.elapsed().as_millis() as u32,
            tracked_series: self.history.procs.len() as u32,
        };

        let snapshot = Snapshot {
            seq: self.seq,
            t: now,
            monitor_since: self.history.monitor_since,
            system,
            processes,
            apps,
            ports,
            services,
            leaks: self.leaks.clone(),
            insights: self.insights.clone(),
            exited: self.history.exited.iter().rev().take(200).cloned().collect(),
            self_stats,
        };
        (snapshot, notices)
    }

    fn collect_raw(&mut self, threads: &HashMap<u32, u32>, cpu_count: usize, elapsed: f64) -> Vec<Raw> {
        let mut out = Vec::with_capacity(self.sys.processes().len());
        let mut seen = HashSet::with_capacity(self.sys.processes().len());
        for (pid, p) in self.sys.processes() {
            // Linux can list each thread as its own entry. They aren't processes.
            if matches!(p.thread_kind(), Some(ThreadKind::Userland)) {
                continue;
            }
            let pid_num = pid.as_u32();
            // Without this, the epoch would read as decades of uptime, and the key
            // would change if a real start time ever showed up.
            let start_time = match (self.first_seen.get(&pid_num), p.start_time()) {
                (Some(seen), _) => *seen,
                (None, 0) => match platform::start_time(pid_num) {
                    Some(real) => real,
                    None => *self.first_seen.entry(pid_num).or_insert_with(now_secs),
                },
                (None, reported) => reported,
            };
            let key = format!("{pid_num}-{start_time}");
            let name = p.name().to_string_lossy().into_owned();
            seen.insert(key.clone());

            if !self.metas.contains_key(&key) {
                let mut exe = p.exe().map_or_else(String::new, |e| e.to_string_lossy().into_owned());
                if exe.is_empty() {
                    exe = platform::image_path(pid_num, &self.drives).unwrap_or_default();
                }
                let args: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
                let cwd = p.cwd().map_or_else(String::new, |c| c.to_string_lossy().into_owned());
                let class = classify(&name, &exe, &args);
                let file = if exe.is_empty() { platform::FileInfo::default() } else { platform::file_info(&exe) };
                let project = if matches!(class.category, Category::System | Category::Browser) {
                    None
                } else {
                    detect_project(&cwd, &args, self.home.as_deref())
                };
                let cmd = args.join(" ");
                let cmd_short: String = cmd.chars().take(CMD_SHORT_CHARS).collect();
                self.metas.insert(
                    key.clone(),
                    Meta {
                        class,
                        cmd,
                        cmd_short,
                        cwd,
                        exe,
                        description: file.description,
                        publisher: file.company,
                        project,
                    },
                );
            }

            let exe = self.metas.get(&key).map_or_else(String::new, |m| m.exe.clone());
            let stats = platform::proc_stats(pid_num);
            let disk = p.disk_usage();
            let user = p
                .user_id()
                .and_then(|id| self.users.as_ref()?.get_user_by_id(id))
                .map_or_else(String::new, |u| u.name().to_string());
            out.push(Raw {
                key,
                pid: pid_num,
                ppid: p.parent().map(Pid::as_u32),
                name,
                exe,
                start_time,
                start_known: !self.first_seen.contains_key(&pid_num),
                user,
                cpu: (p.cpu_usage() / cpu_count as f32).max(0.0),
                read_rate: (disk.read_bytes as f64 / elapsed) as u64,
                write_rate: (disk.written_bytes as f64 / elapsed) as u64,
                private: stats.map_or(p.virtual_memory(), |s| s.private),
                working_set: stats.map_or(p.memory(), |s| s.working_set),
                private_ws: stats.map_or(p.memory(), |s| s.private_ws),
                handles: stats.map_or(0, |s| s.handles),
                threads: threads.get(&pid_num).copied().unwrap_or(0),
                gdi: stats.map_or(0, |s| s.gdi),
                user_objects: stats.map_or(0, |s| s.user_objects),
                limited: !stats.is_some_and(|s| s.accessible),
                memory_known: stats.is_some(),
            });
        }
        self.metas.retain(|k, _| seen.contains(k));
        let processes = self.sys.processes();
        self.first_seen.retain(|pid, _| processes.contains_key(&Pid::from_u32(*pid)));
        out
    }

    fn system_info(&self, raws: &[Raw], cpu_count: usize) -> SystemInfo {
        let perf = platform::perf_info();
        SystemInfo {
            total_memory: self.sys.total_memory(),
            used_memory: self.sys.used_memory(),
            available_memory: self.sys.available_memory(),
            commit_total: perf.commit_total,
            commit_limit: perf.commit_limit,
            system_cache: perf.system_cache,
            kernel_paged: perf.kernel_paged,
            kernel_nonpaged: perf.kernel_nonpaged,
            cpu: self.sys.global_cpu_usage(),
            cpu_count: cpu_count as u32,
            process_count: perf.process_count,
            thread_count: perf.thread_count,
            handle_count: perf.handle_count,
            uptime_secs: System::uptime().unwrap_or(0),
            limited_processes: raws.iter().filter(|r| r.limited).count() as u32,
            elevated: self.elevated,
            host_name: System::host_name().unwrap_or_default(),
            platform: platform::PLATFORM.to_string(),
            windows_known: platform::windows_fully_known(),
        }
    }

    fn build_services(&self, processes: &[ProcessInfo]) -> Vec<ServiceInfo> {
        let keys: HashMap<u32, &str> = processes.iter().map(|p| (p.pid, p.key.as_str())).collect();
        self.services
            .iter()
            .map(|s| ServiceInfo {
                name: s.name.clone(),
                display_name: s.display_name.clone(),
                pid: s.pid,
                key: keys.get(&s.pid).map(|k| k.to_string()),
            })
            .collect()
    }

    fn track_exits(&mut self, processes: &[ProcessInfo], now: u64) {
        let alive: HashSet<&str> = processes.iter().map(|p| p.key.as_str()).collect();
        let gone: Vec<String> = self.previous.keys().filter(|k| !alive.contains(k.as_str())).cloned().collect();
        for key in gone {
            let Some(p) = self.previous.remove(&key) else { continue };
            let hist = self.history.procs.remove(&key);
            let identity = format!(
                "{}|{}|{}",
                p.name,
                p.role,
                p.project.as_ref().map_or("", |x| x.path.as_str())
            );
            self.history.record_exit(ExitedInfo {
                key,
                pid: p.pid,
                name: p.name,
                role: p.role,
                app_label: p.app_label,
                project: p.project,
                category: p.category,
                started: p.start_time,
                start_known: p.start_known,
                ended: now,
                peak_private: hist.map_or(p.private, |h| h.peak_private),
                identity,
            });
        }
        // Series restored from disk for processes that ended while we were closed.
        if self.seq == 0 {
            self.history.procs.retain(|k, _| alive.contains(k.as_str()));
        }
    }

    fn update_history(&mut self, processes: &[ProcessInfo], system: &SystemInfo, now: u64) {
        let span = self.history.span;
        let mut apps: HashMap<&str, Bucket> = HashMap::new();
        for p in processes {
            let sample = sample_of(p, now);
            self.history
                .procs
                .entry(p.key.clone())
                .or_insert_with(|| ProcHist::new(span))
                .push(sample);
            let total = apps.entry(p.app_label.as_str()).or_insert(Bucket { t: now, k: 1, ..Default::default() });
            total.p += sample.p;
            total.w += sample.w;
            total.h += sample.h;
            total.n += sample.n;
            total.g += sample.g;
            total.u += sample.u;
            total.c += sample.c;
        }
        for (label, mut bucket) in apps {
            bucket.pm = bucket.p;
            bucket.px = bucket.p;
            bucket.hm = bucket.h;
            bucket.nm = bucket.n;
            bucket.gm = bucket.g;
            bucket.um = bucket.u;
            self.history.push_app(label, bucket);
        }
        let kib = |v: u64| (v / 1024) as u32;
        self.history.push_system(SysBucket {
            t: now,
            used: kib(system.used_memory),
            commit: kib(system.commit_total),
            nonpaged: kib(system.kernel_nonpaged),
            nonpaged_min: kib(system.kernel_nonpaged),
            cache: kib(system.system_cache),
            cpu: system.cpu,
            k: 1,
        });
    }

    fn build_apps(&self, processes: &[ProcessInfo]) -> Vec<AppSummary> {
        let mut map: HashMap<&str, AppSummary> = HashMap::new();
        let mut instances: HashMap<&str, HashSet<&str>> = HashMap::new();
        for p in processes {
            let entry = map.entry(p.app_label.as_str()).or_insert_with(|| AppSummary {
                label: p.app_label.clone(),
                category: p.category,
                process_count: 0,
                instance_count: 0,
                private: 0,
                working_set: 0,
                private_ws: 0,
                cpu: 0.0,
                handles: 0,
                threads: 0,
                trend: Vec::new(),
            });
            entry.process_count += 1;
            entry.private += p.private;
            entry.working_set += p.working_set;
            entry.private_ws += p.private_ws;
            entry.cpu += p.cpu;
            entry.handles += p.handles;
            entry.threads += p.threads;
            // The root process decides how the whole app is categorised.
            if p.key == p.app_key {
                entry.category = p.category;
            }
            instances.entry(p.app_label.as_str()).or_default().insert(p.app_key.as_str());
        }
        let mut apps: Vec<AppSummary> = map
            .into_iter()
            .map(|(label, mut summary)| {
                summary.instance_count = instances.get(label).map_or(1, |s| s.len() as u32);
                if let Some(hist) = self.history.apps.get(label) {
                    let skip = hist.fine.len().saturating_sub(30);
                    summary.trend = hist.fine.iter().skip(skip).map(|b| b.p / 1024).collect();
                }
                summary
            })
            .collect();
        apps.sort_by_key(|a| std::cmp::Reverse(a.private));
        apps
    }

    fn evaluate_leaks(&self, processes: &[ProcessInfo], system: &SystemInfo, settings: &Settings) -> Vec<LeakReport> {
        let min_secs = f64::from(settings.leak_min_minutes) * 60.0;
        let solid = min_secs * 4.0;
        let warmup = f64::from(settings.leak_warmup_minutes) * 60.0;
        let is_ignored = |name: &str| settings.ignored.iter().any(|i| i.eq_ignore_ascii_case(name));
        let available = system.available_memory as f64;

        let checks: [(Metric, Thresholds); 5] = [
            (Metric::Memory, Thresholds { min_growth: 64.0 * MB, min_growth_pct: 15.0, min_rate_per_hour: 8.0 * MB, min_seconds: min_secs, solid_seconds: solid }),
            (Metric::Handles, Thresholds { min_growth: 500.0, min_growth_pct: 25.0, min_rate_per_hour: 50.0, min_seconds: min_secs, solid_seconds: solid }),
            (Metric::Threads, Thresholds { min_growth: 30.0, min_growth_pct: 30.0, min_rate_per_hour: 5.0, min_seconds: min_secs, solid_seconds: solid }),
            (Metric::Gdi, Thresholds { min_growth: 200.0, min_growth_pct: 25.0, min_rate_per_hour: 25.0, min_seconds: min_secs, solid_seconds: solid }),
            (Metric::UserObjects, Thresholds { min_growth: 200.0, min_growth_pct: 25.0, min_rate_per_hour: 25.0, min_seconds: min_secs, solid_seconds: solid }),
        ];

        let mut out = Vec::new();
        for p in processes {
            if p.pid == self.self_pid || p.category == Category::System || is_ignored(&p.name) || is_ignored(&p.app_label) {
                continue;
            }
            let Some(hist) = self.history.procs.get(&p.key) else { continue };
            let buckets = hist.series.points();
            for (metric, th) in &checks {
                if *metric == Metric::Memory && p.private < MIN_LEAK_PRIVATE {
                    continue;
                }
                let points = metric_points(&buckets, *metric);
                if let Some(trend) = leaks::analyze(&points, p.start_time as f64, warmup, th) {
                    let exhaustion = (*metric == Metric::Memory && trend.confidence > Confidence::Watching)
                        .then(|| available / trend.rate_per_hour);
                    out.push(report(
                        format!("process:{}:{metric:?}", p.key),
                        SubjectKind::Process,
                        Some(p.key.clone()),
                        p.name.clone(),
                        p.role.clone(),
                        p.app_label.clone(),
                        p.project.clone(),
                        *metric,
                        trend,
                        exhaustion,
                        &points,
                    ));
                }
            }
        }

        // App totals catch leaks that a respawning child would hide. Opening more
        // windows raises totals too, so the bar is higher and confidence is capped.
        let app_threshold = Thresholds { min_growth: 512.0 * MB, min_growth_pct: 25.0, min_rate_per_hour: 48.0 * MB, min_seconds: min_secs * 2.0, solid_seconds: solid * 2.0 };
        for (label, hist) in &self.history.apps {
            let category = processes.iter().find(|p| &p.app_label == label && p.key == p.app_key).map(|p| p.category);
            if !matches!(category, Some(Category::Editor | Category::App | Category::Service)) || is_ignored(label) {
                continue;
            }
            let points = metric_points(&hist.series.points(), Metric::Memory);
            if let Some(mut trend) = leaks::analyze(&points, 0.0, 0.0, &app_threshold) {
                trend.confidence = trend.confidence.min(Confidence::Low);
                out.push(report(
                    format!("app:{label}"),
                    SubjectKind::App,
                    None,
                    label.clone(),
                    "All processes combined".into(),
                    label.clone(),
                    None,
                    Metric::Memory,
                    trend,
                    None,
                    &points,
                ));
            }
        }

        // A rising non-paged pool points at a driver, which no process list will show.
        let pool: Vec<Point> = self
            .history
            .system
            .points()
            .iter()
            .map(|b| Point {
                t: b.t as f64,
                avg: f64::from(b.nonpaged) * 1024.0,
                min: f64::from(b.nonpaged_min) * 1024.0,
            })
            .collect();
        let pool_threshold = Thresholds { min_growth: 128.0 * MB, min_growth_pct: 25.0, min_rate_per_hour: 16.0 * MB, min_seconds: min_secs * 2.0, solid_seconds: solid * 2.0 };
        if let Some(trend) = leaks::analyze(&pool, 0.0, 0.0, &pool_threshold) {
            out.push(report(
                "system:nonpaged".into(),
                SubjectKind::System,
                None,
                platform::KERNEL_POOL_NAME.into(),
                "Usually a driver".into(),
                platform::SYSTEM_APP_LABEL.into(),
                None,
                Metric::NonpagedPool,
                trend,
                None,
                &pool,
            ));
        }

        out.sort_by(|a, b| {
            b.confidence
                .cmp(&a.confidence)
                .then(b.rate_per_hour.partial_cmp(&a.rate_per_hour).unwrap_or(std::cmp::Ordering::Equal))
        });
        out
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    id: String,
    subject: SubjectKind,
    key: Option<String>,
    name: String,
    role: String,
    app_label: String,
    project: Option<Project>,
    metric: Metric,
    trend: leaks::Trend,
    hours_to_exhaustion: Option<f64>,
    points: &[Point],
) -> LeakReport {
    LeakReport {
        id,
        subject,
        key,
        name,
        role,
        app_label,
        project,
        metric,
        confidence: trend.confidence,
        pattern: trend.pattern,
        rate_per_hour: trend.rate_per_hour,
        growth: trend.growth,
        growth_pct: trend.growth_pct,
        start_value: trend.start_value,
        current_value: trend.current_value,
        observed_secs: trend.observed_secs as u64,
        z_score: trend.z_score,
        monotonicity: trend.monotonicity,
        hours_to_exhaustion,
        series: leaks::downsample(points, 60),
    }
}

fn sample_of(p: &ProcessInfo, now: u64) -> Bucket {
    let kib = (p.private / 1024) as u32;
    Bucket {
        t: now,
        p: kib,
        pm: kib,
        px: kib,
        w: (p.working_set / 1024) as u32,
        h: p.handles,
        hm: p.handles,
        n: p.threads,
        nm: p.threads,
        g: p.gdi,
        gm: p.gdi,
        u: p.user_objects,
        um: p.user_objects,
        c: p.cpu,
        k: 1,
    }
}

fn build_ports(tcp: &platform::TcpState, processes: &[ProcessInfo], now: u64) -> Vec<PortInfo> {
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    // A server listening on IPv4 and IPv6 is one row, shown as its most exposed address.
    let mut seen: HashMap<(u16, u32), PortInfo> = HashMap::new();
    for l in &tcp.listeners {
        let owner = by_pid.get(&l.pid);
        let entry = seen.entry((l.port, l.pid)).or_insert_with(|| PortInfo {
            port: l.port,
            address: l.address.clone(),
            exposed: l.exposed,
            pid: l.pid,
            key: owner.map(|p| p.key.clone()),
            process_name: owner.map_or_else(|| "Unknown".to_string(), |p| p.name.clone()),
            role: owner.map_or_else(String::new, |p| p.role.clone()),
            tech: owner.and_then(|p| p.tech.clone()),
            app_label: owner.map_or_else(String::new, |p| p.app_label.clone()),
            project: owner.and_then(|p| p.project.clone()),
            category: owner.map_or(Category::Other, |p| p.category),
            connections: tcp.connections_by_port.get(&l.port).copied().unwrap_or(0),
            private: owner.map_or(0, |p| p.private),
            uptime_secs: owner.map_or(0, |p| now.saturating_sub(p.start_time)),
        });
        if l.exposed && !entry.exposed {
            entry.exposed = true;
            entry.address = l.address.clone();
        }
    }
    let mut ports: Vec<PortInfo> = seen.into_values().collect();
    ports.sort_by_key(|p| p.port);
    ports
}

/// Serializes a snapshot of the machine as it is right now, for inspecting
/// classification against real processes: `cargo test dump_live -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore = "reads the real machine and writes a file"]
    fn dump_live() {
        let settings = Settings::default();
        let mut sampler = Sampler::new(&settings, None);
        let _ = sampler.tick(&settings);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let (snapshot, _) = sampler.tick(&settings);
        let out = std::env::var("WRAITH_DUMP").expect("set WRAITH_DUMP to an output path");
        std::fs::write(out, serde_json::to_string_pretty(&snapshot).unwrap()).unwrap();
        println!("sample took {} ms for {} processes", snapshot.self_stats.sample_ms, snapshot.processes.len());
    }
}
