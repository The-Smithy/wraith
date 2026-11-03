//! Works out what each process is tied to: its parent chain, the app at the top
//! of that chain, its project, and how it relates to the desktop.

use std::collections::HashMap;
use std::path::Path;

use crate::classify::{is_shell_role, script_arg, Class};
use crate::model::{Category, Presence, ProcessInfo, Project};
use crate::platform;

/// Per process facts that never change, computed once when a process first appears.
#[derive(Clone, Debug)]
pub struct Meta {
    pub class: Class,
    pub cmd: String,
    pub cmd_short: String,
    pub cwd: String,
    pub exe: String,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub project: Option<Project>,
}

/// Live values for one process on this tick.
#[derive(Clone, Debug, Default)]
pub struct Raw {
    pub key: String,
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub exe: String,
    pub start_time: u64,
    pub user: String,
    pub cpu: f32,
    pub read_rate: u64,
    pub write_rate: u64,
    pub private: u64,
    pub working_set: u64,
    pub private_ws: u64,
    pub handles: u32,
    pub threads: u32,
    pub gdi: u32,
    pub user_objects: u32,
    pub limited: bool,
    pub memory_known: bool,
    /// False when Windows would not report a start time, so start_time is a stand-in.
    pub start_known: bool,
}

#[derive(Default)]
pub struct Context {
    pub services: HashMap<u32, Vec<String>>,
    pub windows: HashMap<u32, String>,
    /// True when a missing window proves nothing, because the OS won't tell.
    pub windows_unknown: bool,
    pub ports: HashMap<u32, Vec<u16>>,
    pub connections: HashMap<u32, u32>,
}

const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "composer.json",
    "pom.xml",
    "build.gradle",
    "deno.json",
];

/// Folders that describe an installed tool rather than a project someone is working on.
fn is_tool_path(lower: &str) -> bool {
    let p = lower.replace('\\', "/");
    p.starts_with("c:/windows")
        || p.contains("/program files")
        || p.contains("/appdata/")
        || p.contains("/.vscode/")
        || p.contains("/.cursor/")
        || p.contains("/.cargo/")
        || p.contains("/.rustup/")
        || p.contains("/nvm4w/")
        || p.contains("/nvm/")
        || p.contains("/.nvm/")
        || p.contains("/.cache/")
        || p.contains("/.local/share/")
        || p.contains("/.vscode-server/")
        || p.starts_with("/usr/")
        || p.starts_with("/opt/")
        || p.starts_with("/snap/")
        || p.starts_with("/var/lib/")
        || p.starts_with("/proc")
        || p.contains("/nodejs/")
        || p.contains("/python3")
}

fn nearest_project(start: &Path, home: Option<&Path>) -> Option<Project> {
    let lower = start.to_string_lossy().to_lowercase();
    if is_tool_path(&lower) {
        return None;
    }
    for dir in start.ancestors().take(12) {
        if home.is_some_and(|h| h == dir) {
            return None;
        }
        // A drive root is never a project, even with a stray marker file in it.
        dir.parent()?;
        if PROJECT_MARKERS.iter().any(|m| dir.join(m).exists()) {
            return Some(Project {
                name: dir.file_name()?.to_string_lossy().into_owned(),
                path: dir.to_string_lossy().into_owned(),
            });
        }
    }
    None
}

/// Finds the project a process belongs to, from its working folder first and then
/// its script path, which also catches tools started from somewhere else.
pub fn detect_project(cwd: &str, args: &[String], home: Option<&Path>) -> Option<Project> {
    if !cwd.is_empty() {
        if let Some(project) = nearest_project(Path::new(cwd), home) {
            return Some(project);
        }
    }
    let script = script_arg(args)?;
    let path = Path::new(script);
    if !path.is_absolute() {
        return None;
    }
    // Tools under node_modules belong to the folder that contains node_modules.
    let lower = script.to_lowercase().replace('\\', "/");
    if let Some(i) = lower.find("/node_modules/") {
        let root = &script[..i];
        if !is_tool_path(&root.to_lowercase()) {
            return nearest_project(Path::new(root), home);
        }
        return None;
    }
    nearest_project(path.parent()?, home)
}

/// Processes whose children should not count as part of them. Everything started
/// by the shell or the service manager is its own app.
fn is_boundary(name_lower: &str) -> bool {
    matches!(
        name_lower,
        "explorer.exe"
            | "services.exe"
            | "wininit.exe"
            | "winlogon.exe"
            | "system"
            | "registry"
            | "smss.exe"
            | "csrss.exe"
            | "svchost.exe"
            | "userinit.exe"
            | "sihost.exe"
            | "taskhostw.exe"
            | "system idle process"
            | "kthreadd"
            | "gnome-shell"
            | "plasmashell"
            | "kwin_wayland"
            | "kwin_x11"
            | "gdm"
            | "gdm3"
            | "sddm"
            | "lightdm"
            | "login"
            | "sshd"
            | "sway"
            | "hyprland"
    )
}

/// A game launched by Steam, or Wraith launched from an editor's terminal, is its own
/// app rather than part of what started it. A shared publisher keeps helpers that own
/// a window, like Steam's web helper, with their real app.
fn is_own_app(child: usize, parent: usize, raws: &[Raw], metas: &HashMap<String, Meta>, ctx: &Context) -> bool {
    let (c, p) = (&raws[child], &raws[parent]);
    if !ctx.windows.contains_key(&c.pid) || c.exe.is_empty() || c.exe.eq_ignore_ascii_case(&p.exe) {
        return false;
    }
    if c.name.eq_ignore_ascii_case("msedgewebview2.exe") {
        return false;
    }
    let publisher = |raw: &Raw| metas.get(&raw.key).and_then(|m| m.publisher.clone());
    let description = |raw: &Raw| metas.get(&raw.key).and_then(|m| m.description.clone());
    // Linux executables have no publisher, so fall back on the installed app's name.
    if publisher(c).is_none() && publisher(p).is_none() {
        return description(c).is_some() && description(c) != description(p);
    }
    publisher(c) != publisher(p)
}

/// Linux hands orphans to the init system, which also starts every desktop app and
/// service. Being its child says nothing about what a process belongs to.
fn is_init(name_lower: &str) -> bool {
    matches!(name_lower, "systemd" | "init")
}

fn stem(name: &str) -> &str {
    name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name)
}

pub fn relate(raws: &[Raw], metas: &HashMap<String, Meta>, ctx: &Context) -> Vec<ProcessInfo> {
    let by_pid: HashMap<u32, usize> = raws.iter().enumerate().map(|(i, r)| (r.pid, i)).collect();
    let lower_names: Vec<String> = raws.iter().map(|r| r.name.to_lowercase()).collect();

    // A parent only counts if it started first. Windows reuses PIDs, so a matching
    // PID alone can point at an unrelated newer process.
    let parents: Vec<Option<usize>> = raws
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let parent = *by_pid.get(&r.ppid?)?;
            let real = parent != i && raws[parent].start_time <= r.start_time + 2 && !is_init(&lower_names[parent]);
            real.then_some(parent)
        })
        .collect();

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); raws.len()];
    for (i, p) in parents.iter().enumerate() {
        if let Some(p) = p {
            children[*p].push(i);
        }
    }

    let roots: Vec<usize> = (0..raws.len())
        .map(|i| {
            let mut current = i;
            for _ in 0..64 {
                match parents[current] {
                    Some(p) if !is_boundary(&lower_names[p]) && !is_own_app(current, p, raws, metas, ctx) => current = p,
                    _ => break,
                }
            }
            current
        })
        .collect();

    // Extension hosts have no project of their own, but the servers they run do.
    let mut projects: Vec<Option<Project>> = raws
        .iter()
        .map(|r| metas.get(&r.key).and_then(|m| m.project.clone()))
        .collect();
    for i in 0..raws.len() {
        if projects[i].is_some() {
            continue;
        }
        let is_host = metas.get(&raws[i].key).is_some_and(|m| m.class.role.ends_with("Extension host"));
        if is_host {
            projects[i] = most_common_project(&children[i], &projects);
        }
    }
    // Then everything inherits from its nearest ancestor with a known project.
    let mut resolved = projects.clone();
    for (i, slot) in resolved.iter_mut().enumerate() {
        if slot.is_some() {
            continue;
        }
        let mut current = i;
        for _ in 0..64 {
            let Some(p) = parents[current] else { break };
            if projects[p].is_some() {
                *slot = projects[p].clone();
                break;
            }
            current = p;
        }
    }

    raws.iter()
        .enumerate()
        .map(|(i, r)| {
            let meta = metas.get(&r.key);
            let mut category = meta.map_or(Category::Other, |m| m.class.category);
            // Decided before a service host like svchost is relabelled as a service below.
            let protected = category == Category::System || is_boundary(&lower_names[i]);
            let mut role = meta.map_or_else(|| "Unknown".to_string(), |m| m.class.role.clone());
            let tech = meta.and_then(|m| m.class.tech.clone());

            // VS Code's terminal host looks like any other Node helper until you
            // see that it owns the shells.
            if role.ends_with("Node helper") {
                let has_terminal = children[i].iter().any(|c| {
                    metas.get(&raws[*c].key).is_some_and(|m| m.class.category == Category::Terminal)
                });
                if has_terminal {
                    role = role.replace("Node helper", "Terminal host");
                }
            }

            let services = ctx.services.get(&r.pid).cloned().unwrap_or_default();
            if !services.is_empty() && matches!(category, Category::System | Category::App | Category::Other) {
                category = Category::Service;
                role = if services.len() == 1 {
                    format!("Service: {}", services[0])
                } else {
                    format!("Service host ({} services)", services.len())
                };
            }

            // A plain script that listens on a port is serving something.
            if category == Category::Runtime && ctx.ports.contains_key(&r.pid) {
                category = Category::DevServer;
            }

            let window_title = ctx.windows.get(&r.pid).cloned();
            let root = roots[i];
            let app = app_label(root, raws, metas, ctx, &lower_names, &parents);

            // "Application" says nothing, and a bare "GPU process" could be anyone's.
            if category == Category::App {
                let same_exe_as_parent = parents[i].is_some_and(|p| !r.exe.is_empty() && raws[p].exe.eq_ignore_ascii_case(&r.exe));
                role = if role == "Application" && same_exe_as_parent {
                    format!("{app}: Worker process")
                } else if role == "Application" {
                    meta.and_then(|m| m.description.clone()).unwrap_or_else(|| stem(&r.name).to_string())
                } else {
                    format!("{app}: {role}")
                };
            }

            let presence = if window_title.is_some() {
                Presence::Window
            } else if !services.is_empty() {
                Presence::Service
            } else if root != i {
                Presence::Child
            } else if ctx.windows_unknown {
                Presence::Unknown
            } else {
                Presence::Background
            };

            let parent_gone = r
                .ppid
                .is_some_and(|ppid| ppid != 0 && ppid != 4 && parents[i].is_none());

            ProcessInfo {
                key: r.key.clone(),
                pid: r.pid,
                parent_key: parents[i].map(|p| raws[p].key.clone()),
                parent_gone,
                name: r.name.clone(),
                exe: r.exe.clone(),
                cmd_short: meta.map_or_else(String::new, |m| m.cmd_short.clone()),
                start_time: r.start_time,
                start_known: r.start_known,
                user: r.user.clone(),
                category,
                protected,
                role,
                tech,
                app_label: app,
                app_key: raws[root].key.clone(),
                project: resolved[i].clone(),
                services,
                window_title,
                presence,
                limited: r.limited,
                memory_known: r.memory_known,
                private: r.private,
                working_set: r.working_set,
                private_ws: r.private_ws,
                cpu: r.cpu,
                read_rate: r.read_rate,
                write_rate: r.write_rate,
                handles: r.handles,
                threads: r.threads,
                gdi: r.gdi,
                user_objects: r.user_objects,
                ports: ctx.ports.get(&r.pid).cloned().unwrap_or_default(),
                connections: ctx.connections.get(&r.pid).copied().unwrap_or(0),
            }
        })
        .collect()
}

fn most_common_project(indices: &[usize], projects: &[Option<Project>]) -> Option<Project> {
    let mut counts: HashMap<&str, (u32, &Project)> = HashMap::new();
    for &i in indices {
        if let Some(p) = &projects[i] {
            counts.entry(p.path.as_str()).or_insert((0, p)).0 += 1;
        }
    }
    counts.into_values().max_by_key(|(n, p)| (*n, p.path.clone())).map(|(_, p)| p.clone())
}

fn app_label(
    root: usize,
    raws: &[Raw],
    metas: &HashMap<String, Meta>,
    ctx: &Context,
    lower_names: &[String],
    parents: &[Option<usize>],
) -> String {
    let r = &raws[root];
    let meta = metas.get(&r.key);
    let category = meta.map_or(Category::Other, |m| m.class.category);
    let tech = meta.and_then(|m| m.class.tech.as_deref());

    if matches!(category, Category::Editor | Category::Browser) {
        if let Some(tech) = tech {
            return tech.to_string();
        }
    }
    if let Some(services) = ctx.services.get(&r.pid) {
        if let [only] = services.as_slice() {
            if category != Category::System {
                return format!("Service: {only}");
            }
        }
        return platform::SERVICES_APP_LABEL.into();
    }
    if is_boundary(&lower_names[root]) || category == Category::System {
        return platform::SYSTEM_APP_LABEL.into();
    }
    // A dev tool whose launcher has exited is tied to nothing, so name it by what it does.
    let detached = parents[root].is_none()
        && r.ppid.is_some_and(|p| p != 0 && p != 4)
        && (category.is_dev() || meta.is_some_and(|m| is_shell_role(&m.class.role)))
        && !ctx.windows.contains_key(&r.pid);
    if detached {
        let role = meta.map_or("Process", |m| m.class.role.as_str());
        return format!("Detached: {role}");
    }
    if let Some(description) = meta.and_then(|m| m.description.as_deref()) {
        return description.to_string();
    }
    stem(&r.name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;

    fn proc(pid: u32, ppid: u32, name: &str, start: u64, args: &[&str]) -> (Raw, Meta) {
        let key = format!("{pid}-{start}");
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let raw = Raw {
            key,
            pid,
            ppid: Some(ppid),
            name: name.into(),
            start_time: start,
            ..Default::default()
        };
        let meta = Meta {
            class: classify(name, "", &args),
            cmd: String::new(),
            cmd_short: String::new(),
            cwd: String::new(),
            exe: String::new(),
            description: None,
            publisher: None,
            project: None,
        };
        (raw, meta)
    }

    fn build(items: Vec<(Raw, Meta)>, ctx: &Context) -> Vec<ProcessInfo> {
        let mut raws = Vec::new();
        let mut metas = HashMap::new();
        for (r, m) in items {
            metas.insert(r.key.clone(), m);
            raws.push(r);
        }
        relate(&raws, &metas, ctx)
    }

    #[test]
    fn language_server_is_tied_to_its_editor() {
        let code = "Code.exe";
        let out = build(
            vec![
                proc(10, 5, "explorer.exe", 100, &[]),
                proc(20, 10, code, 200, &[code]),
                proc(30, 20, code, 210, &[code, "--type=utility", "--utility-sub-type=node.mojom.NodeService", "--inspect-port=0"]),
                proc(40, 30, code, 220, &[code, r"c:\Users\me\.vscode\extensions\ms-python.vscode-pylance-2026.4.1\dist\server.bundle.js"]),
            ],
            &Context::default(),
        );
        let lsp = out.iter().find(|p| p.pid == 40).unwrap();
        assert_eq!(lsp.app_label, "VS Code");
        assert_eq!(lsp.parent_key.as_deref(), Some("30-210"));
        assert_eq!(lsp.app_key, "20-200");
        assert_eq!(lsp.presence, Presence::Child);
        assert!(!lsp.parent_gone);
    }

    #[test]
    fn a_newer_process_with_the_old_parent_pid_is_not_the_parent() {
        // PID 50 was reused after the real parent died.
        let out = build(
            vec![
                proc(50, 1, "other.exe", 900, &[]),
                proc(60, 50, "node.exe", 500, &["node", r"C:\p\node_modules\vite\bin\vite.js"]),
            ],
            &Context::default(),
        );
        let vite = out.iter().find(|p| p.pid == 60).unwrap();
        assert!(vite.parent_gone);
        assert_eq!(vite.parent_key, None);
        assert_eq!(vite.app_label, "Detached: Vite dev server");
        assert_eq!(vite.presence, Presence::Background);
    }

    #[test]
    fn terminal_host_is_recognised_from_its_shells() {
        let code = "Code.exe";
        let out = build(
            vec![
                proc(20, 10, code, 200, &[code]),
                proc(30, 20, code, 210, &[code, "--type=utility", "--utility-sub-type=node.mojom.NodeService"]),
                proc(40, 30, "powershell.exe", 220, &[]),
            ],
            &Context::default(),
        );
        assert_eq!(out.iter().find(|p| p.pid == 30).unwrap().role, "VS Code: Terminal host");
    }

    #[test]
    fn services_are_labelled_and_windows_win_over_children() {
        let mut ctx = Context::default();
        ctx.services.insert(70, vec!["MySvc".into()]);
        ctx.services.insert(71, vec!["A".into(), "B".into()]);
        ctx.windows.insert(80, "Hello".into());
        let out = build(
            vec![
                proc(70, 1, "mysvc.exe", 100, &["mysvc.exe"]),
                proc(71, 1, "svchost.exe", 100, &[]),
                proc(79, 1, "host.exe", 100, &[]),
                proc(80, 79, "child.exe", 110, &[]),
            ],
            &ctx,
        );
        let svc = out.iter().find(|p| p.pid == 70).unwrap();
        assert_eq!((svc.category, svc.presence), (Category::Service, Presence::Service));
        assert_eq!(svc.app_label, "Service: MySvc");
        assert_eq!(out.iter().find(|p| p.pid == 71).unwrap().app_label, platform::SERVICES_APP_LABEL);
        assert_eq!(out.iter().find(|p| p.pid == 80).unwrap().presence, Presence::Window);
    }

    #[test]
    fn scripts_listening_on_a_port_count_as_servers() {
        let mut ctx = Context::default();
        ctx.ports.insert(60, vec![8765]);
        let out = build(
            vec![
                proc(60, 1, "python.exe", 100, &["python", r"C:\p\serve.py"]),
                proc(61, 1, "python.exe", 100, &["python", r"C:\p\job.py"]),
            ],
            &ctx,
        );
        assert_eq!(out.iter().find(|p| p.pid == 60).unwrap().category, Category::DevServer);
        assert_eq!(out.iter().find(|p| p.pid == 61).unwrap().category, Category::Runtime);
    }

    #[test]
    fn generic_app_roles_are_replaced_with_real_names() {
        let (mut root, mut root_meta) = proc(1, 500, "asus_framework.exe", 100, &["asus_framework.exe"]);
        root.exe = r"D:\a\asus_framework.exe".into();
        root_meta.class = classify("asus_framework.exe", &root.exe, &[]);
        root_meta.description = Some("ASUS Framework".into());
        let (mut gpu, mut gpu_meta) = proc(2, 1, "asus_framework.exe", 110, &["x", "--type=gpu-process"]);
        gpu.exe = root.exe.clone();
        gpu_meta.class = classify("asus_framework.exe", &gpu.exe, &["x".to_string(), "--type=gpu-process".to_string()]);
        let out = build(vec![(root, root_meta), (gpu, gpu_meta)], &Context::default());
        assert_eq!(out[0].role, "ASUS Framework");
        assert_eq!(out[0].app_label, "ASUS Framework");
        assert_eq!(out[1].role, "ASUS Framework: GPU process");
    }

    fn windowed(pid: u32, ppid: u32, name: &str, start: u64, exe: &str, publisher: &str) -> (Raw, Meta) {
        let (mut raw, mut meta) = proc(pid, ppid, name, start, &[name]);
        raw.exe = exe.into();
        meta.class = classify(name, exe, &[]);
        meta.publisher = Some(publisher.into());
        meta.description = Some(name.trim_end_matches(".exe").into());
        (raw, meta)
    }

    #[test]
    fn a_windowed_app_from_another_publisher_is_its_own_app() {
        let mut ctx = Context::default();
        ctx.windows.insert(20, "Melvor Idle 2".into());
        let out = build(
            vec![
                windowed(10, 5, "steam.exe", 100, r"C:\Steam\steam.exe", "Valve"),
                windowed(20, 10, "Melvor2.exe", 110, r"C:\Steam\common\Melvor2.exe", "Games Global"),
                windowed(30, 20, "Melvor2.exe", 120, r"C:\Steam\common\Melvor2.exe", "Games Global"),
            ],
            &ctx,
        );
        assert_eq!(out[1].app_label, "Melvor2");
        assert_eq!(out[1].role, "Melvor2");
        // Its helper belongs to the game, and is named as a worker.
        assert_eq!(out[2].app_label, "Melvor2");
        assert_eq!(out[2].role, "Melvor2: Worker process");
    }

    #[test]
    fn a_window_from_the_same_publisher_stays_with_its_app() {
        let mut ctx = Context::default();
        ctx.windows.insert(20, "Steam".into());
        let out = build(
            vec![
                windowed(10, 5, "steam.exe", 100, r"C:\Steam\steam.exe", "Valve"),
                windowed(20, 10, "steamwebhelper.exe", 110, r"C:\Steam\bin\steamwebhelper.exe", "Valve"),
            ],
            &ctx,
        );
        assert_eq!(out[1].app_label, "steam");
    }

    #[test]
    fn service_hosts_stay_protected_after_being_labelled_as_services() {
        let mut ctx = Context::default();
        ctx.services.insert(71, vec!["Windows Update".into()]);
        ctx.services.insert(72, vec!["Parsec".into()]);
        let out = build(
            vec![proc(71, 1, "svchost.exe", 100, &[]), proc(72, 1, "parsecd.exe", 100, &["parsecd.exe"])],
            &ctx,
        );
        let host = out.iter().find(|p| p.pid == 71).unwrap();
        assert_eq!(host.category, Category::Service);
        assert!(host.protected);
        assert!(!out.iter().find(|p| p.pid == 72).unwrap().protected);
    }

    #[test]
    fn init_is_not_a_meaningful_parent() {
        let out = build(
            vec![
                proc(1, 0, "systemd", 10, &[]),
                proc(50, 1, "node", 100, &["node", "/home/me/app/node_modules/vite/bin/vite.js"]),
                proc(51, 1, "nginx", 100, &[]),
            ],
            &Context::default(),
        );
        let vite = out.iter().find(|p| p.pid == 50).unwrap();
        assert_eq!(vite.parent_key, None);
        assert!(vite.parent_gone);
        assert_eq!(vite.app_label, "Detached: Vite dev server");
        assert!(!out.iter().find(|p| p.pid == 1).unwrap().parent_gone);
    }

    #[test]
    fn wayland_cannot_prove_an_app_has_no_window() {
        let ctx = Context { windows_unknown: true, ..Default::default() };
        let out = build(vec![proc(60, 1, "thing", 100, &[])], &ctx);
        assert_eq!(out[0].presence, Presence::Unknown);
    }

    #[test]
    fn on_linux_a_named_app_with_a_window_is_its_own_app() {
        let mut ctx = Context::default();
        ctx.windows.insert(20, "Editor".into());
        let linux = |pid, ppid, name: &str, exe: &str, description: Option<&str>| {
            let (mut raw, mut meta) = proc(pid, ppid, name, 100, &[name]);
            raw.exe = exe.into();
            meta.description = description.map(str::to_string);
            (raw, meta)
        };
        let out = build(
            vec![
                linux(10, 5, "bash", "/usr/bin/bash", None),
                linux(20, 10, "gimp", "/usr/bin/gimp", Some("GIMP")),
            ],
            &ctx,
        );
        assert_eq!(out[1].app_key, out[1].key);
    }

    #[test]
    fn cycles_in_parent_data_do_not_hang() {
        let out = build(
            vec![proc(1, 2, "a.exe", 100, &[]), proc(2, 1, "b.exe", 100, &[])],
            &Context::default(),
        );
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn project_found_from_cwd_and_inherited_by_children() {
        let dir = std::env::temp_dir().join(format!("wraith-test-{}", std::process::id()));
        let sub = dir.join("my-site").join("src");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("my-site").join("package.json"), "{}").unwrap();

        let args: Vec<String> = vec!["node".into(), "x.js".into()];
        // The temp folder sits under AppData, which is treated as tool storage on
        // real systems, so test the walk directly.
        let found = nearest_project_for_test(&sub);
        assert_eq!(found.unwrap().name, "my-site");
        let _ = detect_project("", &args, None);

        let (r1, mut m1) = proc(1, 0, "cmd.exe", 100, &[]);
        m1.project = Some(Project { name: "my-site".into(), path: "C:/my-site".into() });
        let (r2, m2) = proc(2, 1, "node.exe", 110, &["node", "x.js"]);
        let out = build(vec![(r1, m1), (r2, m2)], &Context::default());
        assert_eq!(out[1].project.as_ref().unwrap().name, "my-site");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn nearest_project_for_test(start: &Path) -> Option<Project> {
        start.ancestors().take(12).find_map(|dir| {
            PROJECT_MARKERS.iter().any(|m| dir.join(m).exists()).then(|| Project {
                name: dir.file_name().unwrap().to_string_lossy().into_owned(),
                path: dir.to_string_lossy().into_owned(),
            })
        })
    }

    #[test]
    fn tool_install_paths_are_not_projects() {
        assert!(is_tool_path(r"c:\users\me\appdata\local\programs\microsoft vs code"));
        assert!(is_tool_path(r"c:\windows\system32"));
        assert!(is_tool_path(r"c:\users\me\.vscode\extensions\foo"));
        assert!(is_tool_path("/usr/share/code"));
        assert!(is_tool_path("/home/me/.local/share/thing"));
        assert!(!is_tool_path("/home/me/projects/my-app"));
        assert!(!is_tool_path(r"c:\projects\my-app"));
    }
}
