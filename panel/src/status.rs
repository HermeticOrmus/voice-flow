//! What the panel shows live: the dictation state, the engine that served
//! the last take, and today's numbers. Pure functions take their inputs;
//! the `read_*` wrappers read the files voice-flow and whisper-paste write.
use std::collections::HashMap;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// The whisper servers whisper-paste probes, in its order, with the names a person reads:
/// VOICE_PASTE_REMOTE_HOSTS and VOICE_HOST_NAMES ("host=name;host=name"). This machine's :8093 when
/// none are named; a host with no name reads as its address.
pub fn hosts(conf: &HashMap<String, String>) -> Vec<(String, String)> {
    let names: HashMap<&str, &str> = conf
        .get("VOICE_HOST_NAMES")
        .map(|v| v.split(';').filter_map(|p| p.split_once('=')).map(|(h, n)| (h.trim(), n.trim())).collect())
        .unwrap_or_default();
    let order = conf.get("VOICE_PASTE_REMOTE_HOSTS").map(String::as_str).filter(|v| !v.trim().is_empty());
    order
        .unwrap_or("127.0.0.1:8093")
        .split_whitespace()
        .map(|h| {
            let name = names.get(h).map(|n| n.to_string()).unwrap_or_else(|| {
                if h.starts_with("127.") || h.starts_with("localhost") { "this machine".into() } else { h.into() }
            });
            (h.to_string(), name)
        })
        .collect()
}

/// The hosts from voice-flow.conf, the environment first, as the scripts read them.
pub fn read_hosts() -> Vec<(String, String)> {
    let mut conf = crate::store::load_conf(&crate::store::conf_path());
    for key in ["VOICE_PASTE_REMOTE_HOSTS", "VOICE_HOST_NAMES"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                conf.insert(key.to_string(), v);
            }
        }
    }
    hosts(&conf)
}

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

/// "remote 127.0.0.1:8093" reads as that host's name; "local base.en" as the fallback.
pub fn backend_label(backend: &str, hosts: &[(String, String)]) -> String {
    let mut words = backend.split_whitespace();
    match (words.next(), words.next()) {
        (Some("remote"), Some(host)) => hosts
            .iter()
            .find(|(h, _)| h == host)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| host.to_string()),
        (Some("local"), Some(model)) => format!("local fallback ({model})"),
        _ => "—".to_string(),
    }
}

/// The first host that accepts a TCP connection, by name. Blocks up to
/// 300 ms per host: call it off the main thread.
pub fn reachable(hosts: &[(String, String)]) -> Option<String> {
    hosts.iter().find_map(|(host, name)| {
        let addr = host.to_socket_addrs().ok()?.next()?;
        TcpStream::connect_timeout(&addr, Duration::from_millis(300))
            .ok()
            .map(|_| name.clone())
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

    fn conf(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn hosts_come_from_the_conf_and_default_to_this_machine() {
        assert_eq!(hosts(&conf(&[])), vec![("127.0.0.1:8093".to_string(), "this machine".to_string())]);
        let named = conf(&[
            ("VOICE_PASTE_REMOTE_HOSTS", "127.0.0.1:8093 192.0.2.7:8093 198.51.100.4:8093"),
            ("VOICE_HOST_NAMES", "192.0.2.7:8093=the server;127.0.0.1:8093=desk"),
        ]);
        let names: Vec<String> = hosts(&named).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["desk", "the server", "198.51.100.4:8093"]);
    }

    #[test]
    fn backend_names() {
        let hs = hosts(&conf(&[("VOICE_HOST_NAMES", "127.0.0.1:8093=desk")]));
        assert_eq!(backend_label("remote 127.0.0.1:8093", &hs), "desk");
        assert_eq!(backend_label("remote 198.51.100.9:8093", &hs), "198.51.100.9:8093");
        assert_eq!(backend_label("local base.en", &hs), "local fallback (base.en)");
        assert_eq!(backend_label("", &hs), "—");
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
