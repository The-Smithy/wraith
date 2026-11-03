//! Rule based optimization advice, built from one tick of data plus idle times.
//! Every rule names the processes involved so the UI can show exactly what it means.

use std::collections::HashMap;

use crate::classify::is_shell_role;
use crate::model::{Category, ExitedInfo, Insight, PortInfo, Presence, ProcessInfo, Severity, SystemInfo};

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

pub struct Input<'a> {
    pub now: u64,
    pub system: &'a SystemInfo,
    pub processes: &'a [ProcessInfo],
    pub ports: &'a [PortInfo],
    pub exited: &'a [ExitedInfo],
    /// Seconds each process has been below 0.5% CPU.
    pub idle: &'a HashMap<String, u64>,
    pub pressure_pct: u32,
}

fn bytes(n: u64) -> String {
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else {
        format!("{} MB", n / MB)
    }
}

fn duration(secs: u64) -> String {
    if secs >= 86_400 {
        format!("{:.1} days", secs as f64 / 86_400.0)
    } else if secs >= 3600 {
        format!("{:.1} hours", secs as f64 / 3600.0)
    } else {
        format!("{} minutes", secs / 60)
    }
}

fn keys(list: &[&ProcessInfo]) -> Vec<String> {
    list.iter().map(|p| p.key.clone()).collect()
}

fn total(list: &[&ProcessInfo]) -> u64 {
    list.iter().map(|p| p.private).sum()
}

pub fn evaluate(input: &Input) -> Vec<Insight> {
    let mut out = Vec::new();
    pressure(input, &mut out);
    crash_loops(input, &mut out);
    orphans(input, &mut out);
    duplicate_servers(input, &mut out);
    heavy_background(input, &mut out);
    editor_windows(input, &mut out);
    duplicate_language_servers(input, &mut out);
    idle_servers(input, &mut out);
    idle_agents(input, &mut out);
    exposed_servers(input, &mut out);
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(b.reclaimable.cmp(&a.reclaimable)));
    out
}

fn pressure(input: &Input, out: &mut Vec<Insight>) {
    let sys = input.system;
    if let Some(pct) = (sys.used_memory * 100).checked_div(sys.total_memory) {
        if pct >= u64::from(input.pressure_pct) {
            let mut top: Vec<&ProcessInfo> = input.processes.iter().collect();
            top.sort_by_key(|p| std::cmp::Reverse(p.private));
            top.truncate(5);
            out.push(Insight {
                id: "pressure:ram".into(),
                severity: Severity::Critical,
                title: format!("RAM is {pct}% full"),
                detail: format!("{} available. Windows will start paging, which slows everything down.", bytes(sys.available_memory)),
                action: "End or close the largest items below.".into(),
                reclaimable: 0,
                keys: keys(&top),
            });
        }
    }
    if sys.commit_limit > 0 && sys.commit_total * 100 / sys.commit_limit >= 90 {
        out.push(Insight {
            id: "pressure:commit".into(),
            severity: Severity::Critical,
            title: "Commit charge is near its limit".into(),
            detail: format!("{} of {} committed. Past the limit, apps can crash.", bytes(sys.commit_total), bytes(sys.commit_limit)),
            action: "Close apps, or increase the page file.".into(),
            reclaimable: 0,
            keys: Vec::new(),
        });
    }
}

fn crash_loops(input: &Input, out: &mut Vec<Insight>) {
    let mut groups: HashMap<&str, Vec<&ExitedInfo>> = HashMap::new();
    for e in input.exited {
        let recent = input.now.saturating_sub(e.ended) <= 600;
        let short = e.ended.saturating_sub(e.started) <= 120;
        // Git and shells are started and finished constantly by design.
        let relevant = matches!(
            e.category,
            Category::DevServer | Category::Runtime | Category::LanguageServer | Category::Database | Category::McpServer | Category::Service
        );
        if recent && short && relevant {
            groups.entry(e.identity.as_str()).or_default().push(e);
        }
    }
    for (identity, list) in groups {
        if list.len() >= 5 {
            let first = list[0];
            let place = first.project.as_ref().map_or(String::new(), |p| format!(" in {}", p.name));
            out.push(Insight {
                id: format!("crash:{identity}"),
                // A plain script may just run on a timer, so it only gets a tip.
                severity: if first.category == Category::Runtime { Severity::Info } else { Severity::Warning },
                title: format!("{} keeps restarting{place}", first.role),
                detail: format!("{} exits in 10 minutes, each within 2 minutes of starting.", list.len()),
                action: "Check its logs. Ignore this if it runs on a timer.".into(),
                reclaimable: 0,
                keys: Vec::new(),
            });
        }
    }
}

fn orphans(input: &Input, out: &mut Vec<Insight>) {
    let by_key: HashMap<&str, &ProcessInfo> = input.processes.iter().map(|p| (p.key.as_str(), p)).collect();
    // A server under a shell whose own launcher exited is just as forgotten.
    let under_detached_shell = |p: &ProcessInfo| {
        p.app_key != p.key
            && by_key.get(p.app_key.as_str()).is_some_and(|root| {
                is_shell_role(&root.role) && root.parent_gone && root.presence != Presence::Window
            })
    };
    let list: Vec<&ProcessInfo> = input
        .processes
        .iter()
        .filter(|p| {
            (p.parent_gone || under_detached_shell(p))
                && p.presence != Presence::Window
                && p.services.is_empty()
                && matches!(
                    p.category,
                    Category::DevServer | Category::BuildTool | Category::TestRunner | Category::LanguageServer
                        | Category::PackageManager | Category::McpServer | Category::Runtime
                )
        })
        .collect();
    if !list.is_empty() {
        let reclaim = total(&list);
        out.push(Insight {
            id: "orphans".into(),
            severity: if reclaim >= 200 * MB { Severity::Warning } else { Severity::Info },
            title: format!("{} forgotten dev process{}", list.len(), plural(list.len(), "es")),
            detail: "Whatever launched them has exited.".into(),
            action: "End any you no longer need. Some tools detach on purpose.".into(),
            reclaimable: reclaim,
            keys: keys(&list),
        });
    }
}

fn plural(n: usize, suffix: &str) -> &str {
    if n == 1 {
        ""
    } else {
        suffix
    }
}

fn duplicate_servers(input: &Input, out: &mut Vec<Insight>) {
    let mut groups: HashMap<(String, String), Vec<&ProcessInfo>> = HashMap::new();
    for p in input.processes {
        if p.category == Category::DevServer && !p.ports.is_empty() {
            if let (Some(project), Some(tech)) = (&p.project, &p.tech) {
                groups.entry((project.path.clone(), tech.clone())).or_default().push(p);
            }
        }
    }
    for ((path, tech), list) in groups {
        if list.len() >= 2 {
            let project = list[0].project.as_ref().map_or(path.as_str(), |p| p.name.as_str());
            let mut ports: Vec<String> = list.iter().flat_map(|p| p.ports.iter().map(u16::to_string)).collect();
            ports.sort();
            let largest = list.iter().map(|p| p.private).max().unwrap_or(0);
            out.push(Insight {
                id: format!("dupserver:{path}:{tech}"),
                severity: Severity::Warning,
                title: format!("{} {tech} servers for {project}", list.len()),
                detail: format!("Ports {}. An older server was probably left running.", ports.join(", ")),
                action: "End the ones you are not using.".into(),
                reclaimable: total(&list).saturating_sub(largest),
                keys: keys(&list),
            });
        }
    }
}

fn heavy_background(input: &Input, out: &mut Vec<Insight>) {
    // This rule is about having no window, which can't be known everywhere.
    if !input.system.windows_known {
        return;
    }
    let mut groups: HashMap<&str, Vec<&ProcessInfo>> = HashMap::new();
    for p in input.processes {
        let independent = matches!(p.presence, Presence::Background | Presence::Service | Presence::Child);
        let plain = matches!(p.category, Category::App | Category::Other | Category::Service);
        if independent && plain && !p.limited {
            groups.entry(p.name.as_str()).or_default().push(p);
        }
    }
    for (name, list) in groups {
        let sum = total(&list);
        let has_window_sibling = input
            .processes
            .iter()
            .any(|p| p.presence == Presence::Window && list.iter().any(|l| l.app_key == p.app_key));
        if sum >= GB && !has_window_sibling {
            out.push(Insight {
                id: format!("heavy:{name}"),
                severity: Severity::Warning,
                title: format!("{name}: {} with no window", bytes(sum)),
                detail: format!("{} background process{}, nothing on screen.", list.len(), plural(list.len(), "es")),
                action: "Turn it off in its own settings or Windows startup apps.".into(),
                reclaimable: sum,
                keys: keys(&list),
            });
        }
    }
}

fn editor_windows(input: &Input, out: &mut Vec<Insight>) {
    let mut hosts: HashMap<&str, Vec<&ProcessInfo>> = HashMap::new();
    for p in input.processes {
        if p.category == Category::Editor && p.role.ends_with("Extension host") {
            hosts.entry(p.app_label.as_str()).or_default().push(p);
        }
    }
    for (editor, list) in hosts {
        let tree_total: u64 = input.processes.iter().filter(|p| p.app_label == editor).map(|p| p.private).sum();
        let oldest = list.iter().map(|p| p.start_time).min().unwrap_or(input.now);
        let age = input.now.saturating_sub(oldest);
        if list.len() >= 2 {
            out.push(Insight {
                id: format!("windows:{editor}"),
                severity: if tree_total >= 6 * GB { Severity::Warning } else { Severity::Info },
                title: format!("{editor}: {} windows, {}", list.len(), bytes(tree_total)),
                detail: format!("Each window runs its own tooling. Oldest open {}.", duration(age)),
                action: "Close the windows you are not using.".into(),
                reclaimable: tree_total / list.len() as u64,
                keys: keys(&list),
            });
        } else if age >= 3 * 86_400 && tree_total >= 2 * GB {
            out.push(Insight {
                id: format!("uptime:{editor}"),
                severity: Severity::Info,
                title: format!("{editor} open for {}", duration(age)),
                detail: format!("Holding {}.", bytes(tree_total)),
                action: "Restart it to get most of that back.".into(),
                reclaimable: 0,
                keys: keys(&list),
            });
        }
    }
}

fn duplicate_language_servers(input: &Input, out: &mut Vec<Insight>) {
    let mut groups: HashMap<&str, Vec<&ProcessInfo>> = HashMap::new();
    for p in input.processes {
        if p.category == Category::LanguageServer {
            groups.entry(p.role.as_str()).or_default().push(p);
        }
    }
    for (role, list) in groups {
        let sum = total(&list);
        if list.len() >= 2 && sum >= 300 * MB {
            let largest = list.iter().map(|p| p.private).max().unwrap_or(0);
            out.push(Insight {
                id: format!("lsp:{role}"),
                severity: Severity::Info,
                title: format!("{} × {role}: {}", list.len(), bytes(sum)),
                detail: "Editors start one per window.".into(),
                action: "Close windows, or disable the extension per workspace.".into(),
                reclaimable: sum.saturating_sub(largest),
                keys: keys(&list),
            });
        }
    }
}

fn idle_servers(input: &Input, out: &mut Vec<Insight>) {
    let list: Vec<&ProcessInfo> = input
        .processes
        .iter()
        .filter(|p| {
            p.category == Category::DevServer
                && !p.ports.is_empty()
                && p.connections == 0
                && input.idle.get(&p.key).copied().unwrap_or(0) >= 1800
        })
        .collect();
    if !list.is_empty() {
        let reclaim = total(&list);
        out.push(Insight {
            id: "idle-servers".into(),
            severity: Severity::Info,
            title: format!("{} idle dev server{}", list.len(), plural(list.len(), "s")),
            detail: "No clients and almost no CPU for 30+ minutes.".into(),
            action: "End any you are done with.".into(),
            reclaimable: reclaim,
            keys: keys(&list),
        });
    }
}

fn idle_agents(input: &Input, out: &mut Vec<Insight>) {
    let list: Vec<&ProcessInfo> = input
        .processes
        .iter()
        .filter(|p| {
            p.category == Category::AiAgent
                && p.role.starts_with("Claude Code")
                && p.private >= 400 * MB
                && input.idle.get(&p.key).copied().unwrap_or(0) >= 3600
        })
        .collect();
    if !list.is_empty() {
        let reclaim = total(&list);
        out.push(Insight {
            id: "idle-agents".into(),
            severity: Severity::Info,
            title: format!("{} idle Claude Code session{}: {}", list.len(), plural(list.len(), "s"), bytes(reclaim)),
            detail: "Almost no CPU for over an hour.".into(),
            action: "Close the panels you have finished with.".into(),
            reclaimable: reclaim,
            keys: keys(&list),
        });
    }
}

fn exposed_servers(input: &Input, out: &mut Vec<Insight>) {
    let exposed: Vec<&PortInfo> = input.ports.iter().filter(|p| p.exposed && p.category == Category::DevServer).collect();
    if !exposed.is_empty() {
        let ports: Vec<String> = exposed.iter().map(|p| p.port.to_string()).collect();
        out.push(Insight {
            id: "exposed".into(),
            severity: Severity::Info,
            title: format!("{} dev server{} open to your network", exposed.len(), plural(exposed.len(), "s")),
            detail: format!("Port{} {} accept connections from other devices.", plural(exposed.len(), "s"), ports.join(", ")),
            action: "Bind to 127.0.0.1 unless you need network access.".into(),
            reclaimable: 0,
            keys: exposed.iter().filter_map(|p| p.key.clone()).collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(key: &str, category: Category, role: &str, private: u64) -> ProcessInfo {
        ProcessInfo {
            key: key.into(),
            pid: 1,
            parent_key: None,
            parent_gone: false,
            name: "x.exe".into(),
            exe: String::new(),
            cmd_short: String::new(),
            start_time: 1000,
            start_known: true,
            user: String::new(),
            category,
            protected: false,
            role: role.into(),
            tech: None,
            app_label: "App".into(),
            app_key: key.into(),
            project: None,
            services: Vec::new(),
            window_title: None,
            presence: Presence::Child,
            limited: false,
            memory_known: true,
            private,
            working_set: private,
            private_ws: private,
            cpu: 0.0,
            read_rate: 0,
            write_rate: 0,
            handles: 0,
            threads: 0,
            gdi: 0,
            user_objects: 0,
            ports: Vec::new(),
            connections: 0,
        }
    }

    fn run(processes: &[ProcessInfo], idle: &HashMap<String, u64>, exited: &[ExitedInfo]) -> Vec<Insight> {
        let system = SystemInfo {
            total_memory: 32 * GB,
            used_memory: 10 * GB,
            available_memory: 22 * GB,
            commit_total: 10 * GB,
            commit_limit: 40 * GB,
            windows_known: true,
            ..Default::default()
        };
        evaluate(&Input {
            now: 100_000,
            system: &system,
            processes,
            ports: &[],
            exited,
            idle,
            pressure_pct: 90,
        })
    }

    #[test]
    fn healthy_system_has_no_advice() {
        let p = process("1", Category::LanguageServer, "Pylance", 100 * MB);
        assert!(run(&[p], &HashMap::new(), &[]).is_empty());
    }

    #[test]
    fn pressure_is_reported_first() {
        let system = SystemInfo {
            total_memory: 32 * GB,
            used_memory: 31 * GB,
            available_memory: GB,
            ..Default::default()
        };
        let out = evaluate(&Input {
            now: 0,
            system: &system,
            processes: &[],
            ports: &[],
            exited: &[],
            idle: &HashMap::new(),
            pressure_pct: 90,
        });
        assert_eq!(out[0].severity, Severity::Critical);
    }

    #[test]
    fn orphaned_dev_servers_are_flagged_and_counted() {
        let mut a = process("a", Category::DevServer, "Vite dev server", 300 * MB);
        a.parent_gone = true;
        let out = run(&[a], &HashMap::new(), &[]);
        assert_eq!(out[0].id, "orphans");
        assert_eq!(out[0].reclaimable, 300 * MB);
    }

    #[test]
    fn servers_under_a_detached_shell_count_as_orphans() {
        let mut shell = process("s", Category::Terminal, "Shell", 5 * MB);
        shell.parent_gone = true;
        shell.presence = Presence::Background;
        let mut server = process("n", Category::DevServer, "Next.js server", 300 * MB);
        server.app_key = "s".into();
        let out = run(&[shell, server], &HashMap::new(), &[]);
        let orphans = out.iter().find(|i| i.id == "orphans").unwrap();
        assert_eq!(orphans.keys, vec!["n".to_string()]);
    }

    #[test]
    fn tmux_is_not_a_forgotten_shell() {
        let mut tmux = process("t", Category::Terminal, "Terminal multiplexer", 5 * MB);
        tmux.parent_gone = true;
        tmux.presence = Presence::Background;
        let mut server = process("n", Category::DevServer, "Vite", 300 * MB);
        server.app_key = "t".into();
        assert!(!run(&[tmux, server], &HashMap::new(), &[]).iter().any(|i| i.id == "orphans"));
    }

    #[test]
    fn services_are_not_forgotten_processes() {
        let mut server = process("s", Category::Runtime, "Node.js script", 300 * MB);
        server.parent_gone = true;
        server.services = vec!["my-api".into()];
        assert!(!run(&[server], &HashMap::new(), &[]).iter().any(|i| i.id == "orphans"));
    }

    #[test]
    fn no_window_advice_needs_window_detection() {
        let heavy = process("h", Category::App, "Application", 2 * GB);
        let system = SystemInfo { total_memory: 32 * GB, used_memory: GB, windows_known: false, ..Default::default() };
        let out = evaluate(&Input {
            now: 0,
            system: &system,
            processes: std::slice::from_ref(&heavy),
            ports: &[],
            exited: &[],
            idle: &HashMap::new(),
            pressure_pct: 90,
        });
        assert!(!out.iter().any(|i| i.id.starts_with("heavy")));
    }

    #[test]
    fn duplicate_language_servers_count_only_the_extras_as_reclaimable() {
        let list = [
            process("a", Category::LanguageServer, "Pylance", 400 * MB),
            process("b", Category::LanguageServer, "Pylance", 300 * MB),
        ];
        let out = run(&list, &HashMap::new(), &[]);
        assert_eq!(out[0].reclaimable, 300 * MB);
    }

    #[test]
    fn two_vite_servers_for_one_project_are_duplicates() {
        let project = crate::model::Project { name: "site".into(), path: "C:/site".into() };
        let mut a = process("a", Category::DevServer, "Vite", 200 * MB);
        a.ports = vec![5173];
        a.tech = Some("Vite".into());
        a.project = Some(project.clone());
        let mut b = a.clone();
        b.key = "b".into();
        b.ports = vec![5174];
        b.private = 100 * MB;
        let out = run(&[a, b], &HashMap::new(), &[]);
        let dup = out.iter().find(|i| i.id.starts_with("dupserver")).unwrap();
        assert!(dup.detail.contains("5173") && dup.detail.contains("5174"));
        assert_eq!(dup.reclaimable, 100 * MB);
    }

    #[test]
    fn heavy_background_needs_no_window_in_its_tree() {
        let heavy = process("h", Category::App, "Application", 2 * GB);
        assert!(run(std::slice::from_ref(&heavy), &HashMap::new(), &[]).iter().any(|i| i.id.starts_with("heavy")));

        let mut sibling = process("w", Category::App, "Application", 10 * MB);
        sibling.presence = Presence::Window;
        sibling.app_key = "h".into();
        assert!(!run(&[heavy, sibling], &HashMap::new(), &[]).iter().any(|i| i.id.starts_with("heavy")));
    }

    #[test]
    fn idle_server_needs_a_port_no_clients_and_quiet_cpu() {
        let mut p = process("a", Category::DevServer, "Vite", 200 * MB);
        p.ports = vec![5173];
        let quiet = HashMap::from([("a".to_string(), 3600u64)]);
        assert!(run(std::slice::from_ref(&p), &quiet, &[]).iter().any(|i| i.id == "idle-servers"));
        p.connections = 1;
        assert!(!run(std::slice::from_ref(&p), &quiet, &[]).iter().any(|i| i.id == "idle-servers"));
    }

    #[test]
    fn crash_loops_need_repeated_short_lives_of_relevant_processes() {
        let exit = |n: u64, category| ExitedInfo {
            key: n.to_string(),
            pid: 1,
            name: "node.exe".into(),
            role: "Vite dev server".into(),
            app_label: "x".into(),
            project: None,
            category,
            started: 99_900,
            start_known: true,
            ended: 99_950,
            peak_private: 0,
            identity: "vite".into(),
        };
        let looping: Vec<_> = (0..5).map(|n| exit(n, Category::DevServer)).collect();
        assert!(run(&[], &HashMap::new(), &looping).iter().any(|i| i.id.starts_with("crash")));
        let git: Vec<_> = (0..50).map(|n| exit(n, Category::VersionControl)).collect();
        assert!(run(&[], &HashMap::new(), &git).is_empty());
    }
}
