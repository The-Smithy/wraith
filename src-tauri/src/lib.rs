mod classify;
mod history;
mod insights;
mod leaks;
mod model;
mod platform;
mod relate;
mod sampler;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;

use history::{Bucket, SysBucket};
use model::*;
use sampler::{now_secs, Notice, Sampler};

const SAVE_EVERY: Duration = Duration::from_secs(300);

struct Shared {
    sampler: Mutex<Sampler>,
    snapshot: RwLock<Arc<Snapshot>>,
    settings: RwLock<Settings>,
    data_dir: PathBuf,
    quitting: AtomicBool,
}

impl Shared {
    fn state_path(&self) -> PathBuf {
        self.data_dir.join("history.json")
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    /// Writes to a temp file first, so a crash mid write cannot corrupt the history.
    fn save_history(&self) {
        let json = self.sampler.lock().history.to_json(now_secs());
        if let Some(json) = json {
            let tmp = self.state_path().with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, self.state_path());
            }
        }
    }
}

fn data_dir() -> PathBuf {
    let dir = std::env::var_os("WRAITH_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_local_dir().map(|d| d.join("Wraith")))
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load_settings(dir: &std::path::Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .sanitized()
}

#[tauri::command(async)]
fn get_snapshot(since: u64, shared: State<'_, Arc<Shared>>) -> tauri::ipc::Response {
    let snapshot = shared.snapshot.read().clone();
    let body = if snapshot.seq == since {
        "null".to_string()
    } else {
        serde_json::to_string(snapshot.as_ref()).unwrap_or_else(|_| "null".into())
    };
    tauri::ipc::Response::new(body)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessHistory {
    fine: Vec<Bucket>,
    lifetime: Vec<Bucket>,
    span: u64,
}

// Commands that take the sampler lock or touch other processes run off the main
// thread, so a slow tick can never freeze the window.
#[tauri::command(async)]
fn get_process_history(key: String, shared: State<'_, Arc<Shared>>) -> Option<ProcessHistory> {
    let sampler = shared.sampler.lock();
    let hist = sampler.history.procs.get(&key)?;
    Some(ProcessHistory {
        fine: hist.fine.iter().copied().collect(),
        lifetime: hist.series.points(),
        span: hist.series.span,
    })
}

#[tauri::command(async)]
fn get_app_history(label: String, shared: State<'_, Arc<Shared>>) -> Option<ProcessHistory> {
    let sampler = shared.sampler.lock();
    let hist = sampler.history.apps.get(&label)?;
    Some(ProcessHistory {
        fine: hist.fine.iter().copied().collect(),
        lifetime: hist.series.points(),
        span: hist.series.span,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemHistory {
    fine: Vec<SysBucket>,
    lifetime: Vec<SysBucket>,
}

#[tauri::command(async)]
fn get_system_history(shared: State<'_, Arc<Shared>>) -> SystemHistory {
    let sampler = shared.sampler.lock();
    SystemHistory {
        fine: sampler.history.system_fine.iter().copied().collect(),
        lifetime: sampler.history.system.points(),
    }
}

#[tauri::command(async)]
fn get_process_detail(key: String, shared: State<'_, Arc<Shared>>) -> Option<ProcessDetail> {
    let snapshot = shared.snapshot.read().clone();
    let process = snapshot.processes.iter().find(|p| p.key == key)?;
    let sampler = shared.sampler.lock();
    let meta = sampler.meta(&key)?;
    Some(ProcessDetail {
        key,
        cmd: meta.cmd.clone(),
        cwd: meta.cwd.clone(),
        exe: process.exe.clone(),
        description: meta.description.clone(),
        publisher: meta.publisher.clone(),
    })
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct KillOutcome {
    killed: Vec<u32>,
    failed: Vec<KillFailure>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KillFailure {
    pid: u32,
    name: String,
    reason: String,
}

/// Confirms a PID still belongs to the process the user was looking at. Windows
/// reuses PIDs, so the snapshot alone is not enough to act on.
fn still_same_process(pid: u32, start_time: u64) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let Ok(mut sys) = System::new() else {
        return false;
    };
    let target = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[target]), true, ProcessRefreshKind::nothing());
    sys.process(target).is_some_and(|p| match p.start_time() {
        0 => platform::start_time(pid) == Some(start_time),
        reported => reported == start_time,
    })
}

#[tauri::command(async)]
fn kill_process(key: String, tree: bool, shared: State<'_, Arc<Shared>>) -> Result<KillOutcome, String> {
    let snapshot = shared.snapshot.read().clone();
    let target = snapshot
        .processes
        .iter()
        .find(|p| p.key == key)
        .ok_or("That process has already exited.")?;

    let own_pid = std::process::id();
    if target.pid == own_pid {
        // Checked up front: its tree is the WebView2 process showing this very UI.
        return Err("This is Wraith itself. Quit from the tray icon instead.".into());
    }

    // Children first, so a parent cannot respawn something we just ended.
    let mut order = vec![target];
    if tree {
        let mut seen: HashSet<&str> = HashSet::from([target.key.as_str()]);
        let mut index = 0;
        while index < order.len() {
            let parent = order[index].key.as_str();
            for child in snapshot.processes.iter().filter(|p| p.parent_key.as_deref() == Some(parent)) {
                if seen.insert(child.key.as_str()) {
                    order.push(child);
                }
            }
            index += 1;
        }
        order.reverse();
    }

    let mut outcome = KillOutcome::default();
    for process in order {
        let result = if process.protected {
            Err("Windows components are protected.".to_string())
        } else if !still_same_process(process.pid, process.start_time) {
            Err("It already exited, and its PID may have been reused.".to_string())
        } else {
            platform::terminate(process.pid)
        };
        match result {
            Ok(()) => outcome.killed.push(process.pid),
            Err(reason) => outcome.failed.push(KillFailure {
                pid: process.pid,
                name: process.name.clone(),
                reason,
            }),
        }
    }
    Ok(outcome)
}

#[tauri::command(async)]
fn reveal_path(path: String) -> Result<(), String> {
    let target = std::path::Path::new(&path);
    if !target.exists() {
        return Err("That path no longer exists.".into());
    }
    platform::reveal(target)
}

#[tauri::command]
fn get_settings(shared: State<'_, Arc<Shared>>) -> Settings {
    shared.settings.read().clone()
}

#[tauri::command]
fn set_settings(settings: Settings, shared: State<'_, Arc<Shared>>) -> Result<Settings, String> {
    let clean = settings.sanitized();
    let json = serde_json::to_string_pretty(&clean).map_err(|e| e.to_string())?;
    std::fs::write(shared.settings_path(), json).map_err(|e| e.to_string())?;
    *shared.settings.write() = clean.clone();
    Ok(clean)
}

fn format_rate(report: &LeakReport) -> String {
    match report.metric {
        Metric::Memory | Metric::NonpagedPool => format!("{:.0} MB per hour", report.rate_per_hour / 1_048_576.0),
        Metric::Handles => format!("{:.0} handles per hour", report.rate_per_hour),
        Metric::Threads => format!("{:.0} threads per hour", report.rate_per_hour),
        Metric::Gdi => format!("{:.0} GDI objects per hour", report.rate_per_hour),
        Metric::UserObjects => format!("{:.0} USER objects per hour", report.rate_per_hour),
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn run_sampler(app: AppHandle, shared: Arc<Shared>) {
    let mut last_save = Instant::now();
    loop {
        if shared.quitting.load(Ordering::Relaxed) {
            return;
        }
        let started = Instant::now();
        let settings = shared.settings.read().clone();
        let (snapshot, notices) = shared.sampler.lock().tick(&settings);

        let percent = (snapshot.system.used_memory * 100)
            .checked_div(snapshot.system.total_memory)
            .unwrap_or(0);
        let leaks = snapshot.leaks.iter().filter(|l| l.confidence >= Confidence::Low).count();
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(format!("Wraith: RAM {percent}%, {leaks} suspected leaks")));
        }
        *shared.snapshot.write() = Arc::new(snapshot);

        for notice in notices {
            match notice {
                Notice::Leak(report) if settings.notify_leaks => {
                    notify(
                        &app,
                        &format!("Possible leak: {}", report.name),
                        &format!("{}. Growing about {}.", report.role, format_rate(&report)),
                    );
                }
                Notice::Pressure { pct, available } if settings.notify_pressure => {
                    notify(
                        &app,
                        &format!("RAM is {pct}% full"),
                        &format!("{:.1} GB available.", available as f64 / 1_073_741_824.0),
                    );
                }
                _ => {}
            }
        }

        if last_save.elapsed() >= SAVE_EVERY {
            shared.save_history();
            last_save = Instant::now();
        }

        let interval = Duration::from_secs(u64::from(settings.interval_secs));
        std::thread::sleep(interval.saturating_sub(started.elapsed()).max(Duration::from_millis(200)));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    let dir = data_dir();
    let settings = load_settings(&dir);
    let persisted = std::fs::read_to_string(dir.join("history.json")).ok();
    let shared = Arc::new(Shared {
        sampler: Mutex::new(Sampler::new(&settings, persisted.as_deref())),
        snapshot: RwLock::new(Arc::new(Snapshot::default())),
        settings: RwLock::new(settings),
        data_dir: dir,
        quitting: AtomicBool::new(false),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_process_history,
            get_app_history,
            get_system_history,
            get_process_detail,
            kill_process,
            reveal_path,
            get_settings,
            set_settings
        ])
        .setup(move |app| {
            let show = MenuItem::with_id(app, "show", "Open Wraith", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Wraith")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => {
                        let shared = app.state::<Arc<Shared>>();
                        shared.quitting.store(true, Ordering::Relaxed);
                        shared.save_history();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            let handle = app.handle().clone();
            let worker = shared.clone();
            std::thread::Builder::new()
                .name("sampler".into())
                .spawn(move || run_sampler(handle, worker))?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let shared = window.state::<Arc<Shared>>();
                if shared.settings.read().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    shared.quitting.store(true, Ordering::Relaxed);
                    shared.save_history();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Wraith");
}
