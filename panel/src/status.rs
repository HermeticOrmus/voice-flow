//! What the panel shows live: the dictation state, the engine that served
//! the last take, and today's numbers. Pure functions take their inputs;
//! the `read_*` wrappers read the files voice-flow and whisper-paste write.
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Hosts whisper-paste probes, in its order, with the names a person reads.
pub const HOSTS: &[(&str, &str)] = &[
    ("127.0.0.1:8093", "Sun"),
    ("10.10.90.145:8093", "sg · HQ LAN"),
    ("100.66.108.108:8093", "sg · Tailscale"),
];

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("ormus-voice")
}

/// idle, listening or transcribing: from the engine's recording flag and
/// the pill's state word (thinking while the server works).
pub fn state(recording: bool, pill: &str) -> &'static str {
    let word = pill.split(':').next().unwrap_or("");
    if word.starts_with("thinking") {
        "transcribing"
    } else if recording {
        "listening"
    } else {
        "idle"
    }
}

pub fn read_state() -> &'static str {
    let dir = runtime_dir();
    let pill = fs::read_to_string(dir.join("pill.state")).unwrap_or_default();
    state(dir.join("recording.state").exists(), pill.trim())
}

/// "remote 127.0.0.1:8093" reads as Sun; "local base.en" as the fallback.
pub fn backend_label(backend: &str) -> String {
    let mut words = backend.split_whitespace();
    match (words.next(), words.next()) {
        (Some("remote"), Some(host)) => HOSTS
            .iter()
            .find(|(h, _)| *h == host)
            .map(|(_, name)| name.to_string())
            .unwrap_or_else(|| host.to_string()),
        (Some("local"), Some(model)) => format!("local fallback ({model})"),
        _ => "—".to_string(),
    }
}

/// The first host that accepts a TCP connection, by name. Blocks up to
/// 300 ms per host: call it off the main thread.
pub fn reachable() -> Option<&'static str> {
    HOSTS.iter().find_map(|(host, name)| {
        let addr = host.to_socket_addrs().ok()?.next()?;
        TcpStream::connect_timeout(&addr, Duration::from_millis(300))
            .ok()
            .map(|_| *name)
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Today {
    pub dictations: usize,
    pub words: u64,
    pub avg_ms: Option<u64>,
}

fn field_u64(line: &str, key: &str) -> Option<u64> {
    let start = line.find(&format!("\"{key}\":"))? + key.len() + 3;
    let digits: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Rows of history.jsonl whose timestamp starts with `date` (YYYY-MM-DD).
pub fn today(history: &str, date: &str) -> Today {
    let prefix = format!("\"ts\":\"{date}");
    let mut t = Today::default();
    let mut ms_total = 0u64;
    let mut ms_rows = 0u64;
    for line in history.lines().filter(|l| l.contains(&prefix)) {
        t.dictations += 1;
        t.words += field_u64(line, "words").unwrap_or(0);
        if let Some(ms) = field_u64(line, "transcribe_ms") {
            ms_total += ms;
            ms_rows += 1;
        }
    }
    t.avg_ms = ms_total.checked_div(ms_rows);
    t
}

pub fn local_date() -> String {
    Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn read_today(history_path: &std::path::Path) -> Today {
    today(
        &fs::read_to_string(history_path).unwrap_or_default(),
        &local_date(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_words() {
        assert_eq!(state(false, ""), "idle");
        assert_eq!(state(true, "listening:verbatim"), "listening");
        assert_eq!(state(true, "thinking"), "transcribing");
        assert_eq!(state(false, "done:Hello"), "idle");
    }

    #[test]
    fn backend_names() {
        assert_eq!(backend_label("remote 127.0.0.1:8093"), "Sun");
        assert_eq!(backend_label("remote 10.0.0.9:8093"), "10.0.0.9:8093");
        assert_eq!(backend_label("local base.en"), "local fallback (base.en)");
        assert_eq!(backend_label(""), "—");
    }

    #[test]
    fn todays_rows_only() {
        let history = "\
{\"ts\":\"2026-10-26T23:59:00-05:00\",\"words\":9,\"chars\":50,\"model\":\"base.en\",\"method\":\"type\",\"rewrite\":false,\"transcribe_ms\":900,\"diverted\":0}
{\"ts\":\"2026-10-27T13:44:36-05:00\",\"words\":48,\"chars\":286,\"model\":\"base.en\",\"method\":\"paste\",\"rewrite\":false,\"transcribe_ms\":1343,\"diverted\":0}
{\"ts\":\"2026-10-27T20:03:20-05:00\",\"words\":238,\"chars\":1278,\"model\":\"base.en\",\"method\":\"paste\",\"rewrite\":false,\"transcribe_ms\":1606,\"diverted\":0,\"text\":\"Let me\"}
";
        let t = today(history, "2026-10-27");
        assert_eq!(
            t,
            Today {
                dictations: 2,
                words: 286,
                avg_ms: Some(1474)
            }
        );
        assert_eq!(today(history, "2026-10-28"), Today::default());
    }
}
