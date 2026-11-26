//! Desktop toasts through `notify-send`: each one replaces the last of the same take.

use crate::sys;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct Notifier {
    pub quiet: bool,
    pub id_file: PathBuf,
}

impl Notifier {
    /// VOICE_PASTE_QUIET=1 suppresses toasts (voice-flow's system-wide mode: the pill shows state).
    pub fn note(&self, timeout_ms: u64, title: &str, body: &str) {
        if self.quiet || sys::find_in_path("notify-send").is_none() {
            return;
        }
        let mut args: Vec<String> = Vec::new();
        if let Ok(prev) = std::fs::read_to_string(&self.id_file) {
            let prev = prev.trim();
            if !prev.is_empty() {
                args.push(format!("--replace-id={prev}"));
            }
        }
        args.extend(["--print-id", "--hint=int:transient:1", "-t"].map(String::from));
        args.extend([
            timeout_ms.to_string(),
            "-a".into(),
            "Ormus Voice".into(),
            "--".into(),
            title.into(),
            body.into(),
        ]);
        let Ok(out) = Command::new("notify-send")
            .args(&args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return;
        };
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !id.is_empty() {
            let _ = std::fs::write(&self.id_file, format!("{id}\n"));
        }
    }

    /// Clears the id once the take ends, so the next one starts a fresh toast.
    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.id_file);
    }
}
