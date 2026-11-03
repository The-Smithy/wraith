//! Parsers for Linux's text interfaces. They hold no OS calls, so they are tested on
//! every platform with sample text.

use std::net::{Ipv4Addr, Ipv6Addr};

const KIB: u64 = 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StatusMemory {
    pub rss: u64,
    pub rss_anon: u64,
    pub swap: u64,
}

/// Reads the memory lines of `/proc/<pid>/status`.
pub fn parse_status(text: &str) -> StatusMemory {
    let mut out = StatusMemory::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else { continue };
        let slot = match key {
            "VmRSS" => &mut out.rss,
            "RssAnon" => &mut out.rss_anon,
            "VmSwap" => &mut out.swap,
            _ => continue,
        };
        *slot = first_number(rest) * KIB;
    }
    out
}

fn first_number(text: &str) -> u64 {
    text.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// Thread count from `/proc/<pid>/stat`. The name field can hold spaces and brackets,
/// so fields are counted from the last closing bracket.
pub fn parse_stat_threads(text: &str) -> Option<u32> {
    let rest = &text[text.rfind(')')? + 1..];
    rest.split_whitespace().nth(17)?.parse().ok()
}

/// The process state letter from `/proc/<pid>/stat`.
pub fn parse_stat_state(text: &str) -> Option<char> {
    text[text.rfind(')')? + 1..].split_whitespace().next()?.chars().next()
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MemInfo {
    pub committed: u64,
    pub commit_limit: u64,
    pub cached: u64,
    pub buffers: u64,
    pub reclaimable: u64,
    pub unreclaimable: u64,
}

pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut out = MemInfo::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else { continue };
        let slot = match key {
            "Committed_AS" => &mut out.committed,
            "CommitLimit" => &mut out.commit_limit,
            "Cached" => &mut out.cached,
            "Buffers" => &mut out.buffers,
            "SReclaimable" => &mut out.reclaimable,
            "SUnreclaim" => &mut out.unreclaimable,
            _ => continue,
        };
        *slot = first_number(rest) * KIB;
    }
    out
}

/// Total threads from `/proc/loadavg`, the number after the slash.
pub fn parse_loadavg_threads(text: &str) -> u32 {
    text.split_whitespace()
        .nth(3)
        .and_then(|field| field.split_once('/'))
        .and_then(|(_, total)| total.parse().ok())
        .unwrap_or(0)
}

pub const TCP_ESTABLISHED: u8 = 0x01;
pub const TCP_LISTEN: u8 = 0x0A;

#[derive(Debug, Clone, PartialEq)]
pub struct TcpRow {
    pub address: String,
    pub loopback: bool,
    pub port: u16,
    pub state: u8,
    pub inode: u64,
}

/// One row per socket from `/proc/net/tcp` or `/proc/net/tcp6`. Addresses are hex
/// words in the machine's byte order, which is little endian everywhere Wraith runs.
pub fn parse_tcp(text: &str) -> Vec<TcpRow> {
    text.lines().skip(1).filter_map(parse_tcp_row).collect()
}

fn parse_tcp_row(line: &str) -> Option<TcpRow> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let (address, port) = fields.get(1)?.split_once(':')?;
    let (address, loopback) = parse_address(address)?;
    Some(TcpRow {
        address,
        loopback,
        port: u16::from_str_radix(port, 16).ok()?,
        state: u8::from_str_radix(fields.get(3)?, 16).ok()?,
        inode: fields.get(9)?.parse().ok()?,
    })
}

fn parse_address(hex: &str) -> Option<(String, bool)> {
    match hex.len() {
        8 => {
            let ip = Ipv4Addr::from(u32::from_str_radix(hex, 16).ok()?.to_le_bytes());
            Some((ip.to_string(), ip.is_loopback()))
        }
        32 => {
            let mut bytes = [0u8; 16];
            for (i, chunk) in bytes.chunks_mut(4).enumerate() {
                let word = u32::from_str_radix(hex.get(i * 8..i * 8 + 8)?, 16).ok()?;
                chunk.copy_from_slice(&word.to_le_bytes());
            }
            let ip = Ipv6Addr::from(bytes);
            let loopback = ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback());
            Some((format!("[{ip}]"), loopback))
        }
        _ => None,
    }
}

/// The inode from a `/proc/<pid>/fd` link like `socket:[12345]`.
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
}

/// The systemd service a process belongs to, from `/proc/<pid>/cgroup`. Apps a desktop
/// starts also run in units, but those are named `app-...` and aren't services to a person.
pub fn service_from_cgroup(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let path = line.rsplit(':').next()?;
        let unit = path.rsplit('/').next()?.strip_suffix(".service")?;
        (!unit.is_empty() && !unit.starts_with("app-") && !unit.starts_with("user@")).then(|| unit.to_string())
    })
}

/// The program and display name from a `.desktop` file, ignoring its other sections.
pub fn parse_desktop_entry(text: &str) -> Option<(String, String)> {
    let mut in_entry = false;
    let (mut name, mut exec) = (None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(value) = line.strip_prefix("Name=") {
                name.get_or_insert_with(|| value.to_string());
            } else if let Some(value) = line.strip_prefix("Exec=") {
                exec.get_or_insert_with(|| value.to_string());
            }
        }
    }
    Some((exec_program(&exec?)?, name?))
}

/// The real program in an `Exec=` line, past any `env FOO=bar` wrapper.
fn exec_program(exec: &str) -> Option<String> {
    let mut tokens = exec.split_whitespace().map(|t| t.trim_matches('"')).peekable();
    if tokens.peek().is_some_and(|t| t.rsplit('/').next() == Some("env")) {
        tokens.next();
    }
    let program = tokens.find(|t| !t.contains('='))?;
    Some(program.rsplit('/').next()?.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_memory_is_converted_to_bytes() {
        let text = "Name:\tnode\nVmRSS:\t  2048 kB\nRssAnon:\t  1024 kB\nRssFile:\t 1024 kB\nVmSwap:\t  512 kB\n";
        assert_eq!(
            parse_status(text),
            StatusMemory { rss: 2048 * 1024, rss_anon: 1024 * 1024, swap: 512 * 1024 }
        );
    }

    #[test]
    fn kernel_threads_have_no_memory_lines() {
        assert_eq!(parse_status("Name:\tkworker/0:1\nState:\tI (idle)\n"), StatusMemory::default());
    }

    #[test]
    fn stat_fields_survive_awkward_process_names() {
        let stat = "1234 (Web (Content) x) S 1 1234 1234 0 -1 4194304 100 0 0 0 5 2 0 0 20 0 42 0 999 1000 200 18446744073709551615";
        assert_eq!(parse_stat_threads(stat), Some(42));
        assert_eq!(parse_stat_state(stat), Some('S'));
        assert_eq!(parse_stat_state("9 (x) Z 1 9"), Some('Z'));
        assert_eq!(parse_stat_threads("garbage"), None);
    }

    #[test]
    fn meminfo_reads_the_fields_wraith_needs() {
        let text = "MemTotal: 100 kB\nCached: 10 kB\nBuffers: 1 kB\nSReclaimable: 2 kB\nSUnreclaim: 3 kB\nCommitLimit: 50 kB\nCommitted_AS: 40 kB\n";
        assert_eq!(
            parse_meminfo(text),
            MemInfo { committed: 40960, commit_limit: 51200, cached: 10240, buffers: 1024, reclaimable: 2048, unreclaimable: 3072 }
        );
    }

    #[test]
    fn loadavg_gives_the_thread_total() {
        assert_eq!(parse_loadavg_threads("0.10 0.20 0.30 2/734 12345\n"), 734);
        assert_eq!(parse_loadavg_threads(""), 0);
    }

    #[test]
    fn tcp_v4_rows_decode_address_port_state_and_inode() {
        let text = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 55512 1 0000 100 0 0 10 0\n   1: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 55513 1 0000 100 0 0 10 0\n";
        let rows = parse_tcp(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], TcpRow { address: "127.0.0.1".into(), loopback: true, port: 3306, state: TCP_LISTEN, inode: 55512 });
        assert_eq!((rows[1].address.as_str(), rows[1].loopback, rows[1].port), ("0.0.0.0", false, 8080));
    }

    #[test]
    fn tcp_v6_rows_decode_loopback_any_and_mapped_addresses() {
        let row = |addr: &str| format!("header\n   0: {addr}:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 7 1\n");
        let loopback = parse_tcp(&row("00000000000000000000000001000000"));
        assert_eq!((loopback[0].address.as_str(), loopback[0].loopback), ("[::1]", true));
        let any = parse_tcp(&row("00000000000000000000000000000000"));
        assert_eq!((any[0].address.as_str(), any[0].loopback), ("[::]", false));
        let mapped = parse_tcp(&row("0000000000000000FFFF00000100007F"));
        assert!(mapped[0].loopback, "{}", mapped[0].address);
    }

    #[test]
    fn malformed_tcp_rows_are_skipped() {
        assert!(parse_tcp("header\nnot a row\n   0: zz:zz\n").is_empty());
    }

    #[test]
    fn socket_links_give_their_inode() {
        assert_eq!(socket_inode("socket:[12345]"), Some(12345));
        assert_eq!(socket_inode("pipe:[12345]"), None);
        assert_eq!(socket_inode("/dev/null"), None);
    }

    #[test]
    fn cgroups_identify_services_but_not_desktop_apps() {
        assert_eq!(service_from_cgroup("0::/system.slice/nginx.service\n").as_deref(), Some("nginx"));
        assert_eq!(
            service_from_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/pipewire.service\n").as_deref(),
            Some("pipewire")
        );
        assert_eq!(service_from_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-firefox-123.scope\n"), None);
        assert_eq!(service_from_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.gnome.Terminal.service\n"), None);
        assert_eq!(service_from_cgroup("0::/user.slice/user-1000.slice/session-2.scope\n"), None);
        assert_eq!(service_from_cgroup("0::/\n"), None);
    }

    #[test]
    fn desktop_entries_give_program_and_name() {
        let text = "[Desktop Entry]\nName=Visual Studio Code\nName[de]=Code\nExec=/usr/share/code/code --unity-launch %F\n\n[Desktop Action new]\nName=New Window\nExec=/usr/share/code/code --new-window\n";
        assert_eq!(parse_desktop_entry(text), Some(("code".into(), "Visual Studio Code".into())));
        let wrapped = "[Desktop Entry]\nName=Thing\nExec=env FOO=1 BAR=2 /opt/thing/Thing --flag\n";
        assert_eq!(parse_desktop_entry(wrapped), Some(("thing".into(), "Thing".into())));
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nName=No exec\n"), None);
    }
}
