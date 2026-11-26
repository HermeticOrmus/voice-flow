//! vf: the voice-flow command. First press starts a take, second press stops it, transcribes it
//! and injects the text into the focused window. Same command line, environment and files as
//! bin/voice-flow, so the Hyprland bind and the Python pill keep working.
//!
//! Slice 1 has no daemon: the take runs in `whisper-paste` (found on PATH, as bin/voice-flow finds
//! it) and the two key presses talk through `recording.state`, `pill.state` and `agent.mode` in
//! `$XDG_RUNTIME_DIR/ormus-voice`. vfd and its socket come when the pill subscribes to it.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;
use vf_core::conf::Conf;
use vf_core::dirs::Dirs;
use vf_core::express::Express;
use vf_core::history::Record;
use vf_core::profile::{app_profile, Profile};
use vf_core::text::{char_len, strip_trailing_newlines, word_count};
use vf_inject::{InjectError, Injector, Method};

/// The settings, read the way bin/voice-flow reads them: it sources the conf, so a key in the
/// conf wins over the same key in the environment; an empty value falls back to the default.
struct Settings {
    conf: Conf,
}

impl Settings {
    fn lookup(&self, key: &str) -> Option<String> {
        self.conf
            .get(key)
            .map(str::to_string)
            .or_else(|| std::env::var(key).ok())
    }

    /// `${KEY:-default}`
    fn get(&self, key: &str) -> Option<String> {
        self.lookup(key).filter(|v| !v.is_empty())
    }
}

struct Flow {
    s: Settings,
    dirs: Dirs,
    pill_on: bool,
    express: Express,
    /// `VOICE_PASTE_REWRITE_COMMAND` as the engine will see it; empty is no rewrite.
    rewrite: String,
    model: String,
    injector: Injector,
}

impl Flow {
    fn rec_state(&self) -> PathBuf {
        self.dirs.state.join("recording.state")
    }
    fn pill_state(&self) -> PathBuf {
        self.dirs.state.join("pill.state")
    }
    fn agent_marker(&self) -> PathBuf {
        self.dirs.state.join("agent.mode")
    }

    /// listening|thinking|done|cancel|quit, with its detail after a colon.
    fn pill_set(&self, state: &str) {
        if self.pill_on {
            let _ = fs::write(self.pill_state(), state);
        }
    }

    fn pill_launch(&self) {
        if !self.pill_on {
            return;
        }
        let running = Command::new("pgrep")
            .args(["-f", "voice-flow-pill"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !running {
            let _ = Command::new("setsid")
                .args(["-f", "voice-flow-pill"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    /// The focused app's profile. One `hyprctl` call, and none when there is no profiles file.
    fn profile(&self) -> Profile {
        let path = self
            .s
            .get("VOICE_FLOW_APP_PROFILES")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.dirs.config.join("app-profiles.conf"));
        let Ok(conf) = fs::read_to_string(path) else {
            return Profile::Clean;
        };
        let class = vf_inject::active_class();
        if class.is_empty() {
            return Profile::Clean;
        }
        app_profile(&conf, &class)
    }

    /// The word the pill shows while you speak: verbatim when nothing will rewrite, otherwise the
    /// Express style that will shape the words.
    fn forecast(&self) -> &'static str {
        if self.profile() == Profile::Verbatim || self.rewrite.is_empty() {
            "verbatim"
        } else {
            self.express.as_str()
        }
    }

    fn engine(&self, phase: &str, capture: bool) -> String {
        let mut cmd = Command::new("whisper-paste");
        cmd.env("VOICE_PASTE_PHASE", phase)
            .env("VOICE_PASTE_REWRITE_COMMAND", &self.rewrite)
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        if !capture {
            let _ = cmd.stdout(Stdio::null()).status();
            return String::new();
        }
        cmd.stdout(Stdio::piped());
        match cmd.output() {
            Ok(o) => strip_trailing_newlines(&String::from_utf8_lossy(&o.stdout)).to_string(),
            Err(_) => String::new(),
        }
    }

    fn start(&self, agent: bool) -> i32 {
        if agent {
            let _ = fs::File::create(self.agent_marker());
            self.pill_set("listening-agent");
        } else {
            let _ = fs::remove_file(self.agent_marker());
            // The forecast: what will happen to the words. The stop press decides again (focus
            // can move); this is what the pill shows while you speak.
            if self.pill_on {
                self.pill_set(&format!("listening:{}", self.forecast()));
            }
        }
        self.pill_launch();
        self.engine("start", false);
        0
    }

    fn stop(&mut self) -> i32 {
        if self.agent_marker().exists() {
            // Agent mode: the raw transcript goes to the agent as a command and its ANSWER is injected.
            let _ = fs::remove_file(self.agent_marker());
            self.rewrite = self
                .s
                .get("VOICE_FLOW_AGENT_COMMAND")
                .unwrap_or_else(|| "voice-agent-ollama".into());
            self.pill_set("thinking-agent");
        } else if !self.rewrite.is_empty() && self.profile() == Profile::Verbatim {
            // The focused app wants the transcript untouched.
            self.rewrite.clear();
            self.pill_set("thinking");
        } else {
            self.pill_set("thinking");
        }

        // The focus before the (blocking) transcription: the 1-3 s of "thinking" is the only
        // window in which the person can switch apps, and ydotool types into whatever has focus.
        let focus_before = vf_inject::focus_id();
        let t0 = Instant::now();
        let text = self.engine("stop", true);
        let ms = t0.elapsed().as_millis() as u64;

        if text.is_empty() {
            // The engine says why: silent, junk <logprob>, empty, hallucination.
            let reason = fs::read_to_string(self.dirs.state.join("last-drop")).unwrap_or_default();
            self.pill_set(&format!("cancel:{}", strip_trailing_newlines(&reason)));
            return 0;
        }

        let focus_after = vf_inject::focus_id();
        let preview = preview(&text);
        if !focus_before.is_empty() && !focus_after.is_empty() && focus_before != focus_after {
            // Focus changed during transcription: never type into the wrong window.
            return self.divert(&text, &preview, ms, true);
        }
        match self.injector.inject(&text) {
            Ok(()) => {}
            // The words are on the clipboard and the chord did not paste them: the divert route.
            Err(InjectError::PasteChordFailed(e)) => {
                eprintln!("voice-flow: paste chord failed: {e}");
                return self.divert(&text, &preview, ms, false);
            }
            Err(e) => {
                eprintln!("{e}");
                return if matches!(e, InjectError::UnknownMethod(_)) {
                    2
                } else {
                    1
                };
            }
        }
        self.pill_set(&format!("done:{preview}"));
        let method = self.injector.resolve_method(&text);
        self.history(&text, &method, false, ms);
        0
    }

    /// The words wait on the clipboard for a deliberate paste.
    fn divert(&self, text: &str, preview: &str, ms: u64, copy: bool) -> i32 {
        if copy {
            vf_inject::divert_to_clipboard(text);
        }
        let key = vf_inject::current_paste_key_name();
        self.pill_set(&format!("copied:{key} {preview}"));
        vf_inject::note_divert(key);
        self.history(text, "clipboard-divert", true, ms);
        0
    }

    /// One line per dictation: counts only, never the transcript unless VOICE_FLOW_LOG_TEXT=1.
    fn history(&self, text: &str, method: &str, diverted: bool, ms: u64) {
        let path = self
            .s
            .get("VOICE_FLOW_HISTORY_LOG")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.dirs.data.join("history.jsonl"));
        let ts = Command::new("date")
            .arg("--iso-8601=seconds")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let log_text = self.s.lookup("VOICE_FLOW_LOG_TEXT").as_deref() == Some("1");
        let line = Record {
            ts: &ts,
            words: word_count(text),
            chars: char_len(text),
            model: &self.model,
            method,
            rewrite: !self.rewrite.is_empty(),
            transcribe_ms: ms,
            diverted,
            text: log_text.then_some(text),
        }
        .to_line();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// One line for the pill: what landed, newlines as spaces, at most 120 bytes (never mid-character).
fn preview(text: &str) -> String {
    let flat = text.replace('\n', " ");
    let mut end = flat.len().min(120);
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    flat[..end].to_string()
}

fn main() {
    // A compositor-spawned process inherits the compositor's PATH, which may not include
    // ~/.local/bin where whisper-paste and voice-flow-pill live.
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{home}/.local/bin:{path}"));
    if std::env::var_os("YDOTOOL_SOCKET").is_none_or(|v| v.is_empty()) {
        // SAFETY: getuid(2) takes no arguments and cannot fail.
        let uid = unsafe { libc::getuid() };
        std::env::set_var("YDOTOOL_SOCKET", format!("/run/user/{uid}/.ydotool_socket"));
    }

    let dirs = Dirs::from_env(&|k| std::env::var(k).ok());
    let conf_path = std::env::var("VOICE_FLOW_CONF")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs.conf_file());
    let s = Settings {
        conf: Conf::parse(&fs::read_to_string(conf_path).unwrap_or_default()),
    };

    let pill_on = s.get("VOICE_FLOW_PILL").unwrap_or_else(|| "1".into()) == "1";
    if pill_on
        && std::env::var("VOICE_PASTE_QUIET")
            .ok()
            .filter(|v| !v.is_empty())
            .is_none()
    {
        // The pill conveys state, so the engine's notify-send toasts stay quiet.
        std::env::set_var(
            "VOICE_PASTE_QUIET",
            s.get("VOICE_PASTE_QUIET").unwrap_or_else(|| "1".into()),
        );
    }

    // Express is the shape of the clean-up. A verbatim app profile still forces verbatim at stop.
    let express = Express::parse(
        &s.get("VOICE_FLOW_EXPRESS")
            .unwrap_or_else(|| "prompt".into()),
    );
    std::env::set_var("VOICE_FLOW_EXPRESS", express.as_str());
    let rewrite = if s.get("VOICE_FLOW_REWRITE").unwrap_or_else(|| "on".into()) == "off"
        || express == Express::Verbatim
    {
        String::new()
    } else {
        s.get("VOICE_PASTE_REWRITE_COMMAND")
            .unwrap_or_else(|| "voice-rewrite-ollama".into())
    };
    std::env::set_var(
        "VOICE_FLOW_PARTIALS",
        s.get("VOICE_FLOW_PARTIALS").unwrap_or_else(|| "1".into()),
    );
    let model = s
        .get("VOICE_PASTE_MODEL")
        .unwrap_or_else(|| "medium.en".into());
    std::env::set_var("VOICE_PASTE_MODEL", &model);

    let injector = Injector {
        ydotool: s.get("YDOTOOL").unwrap_or_else(|| "ydotool".into()),
        method: Method::parse(&s.get("VOICE_FLOW_METHOD").unwrap_or_else(|| "auto".into())),
        type_max: s
            .get("VOICE_FLOW_TYPE_MAX")
            .and_then(|v| v.parse().ok())
            .unwrap_or(80),
    };
    let _ = fs::create_dir_all(&dirs.state);
    let mut flow = Flow {
        s,
        dirs,
        pill_on,
        express,
        rewrite,
        model,
        injector,
    };

    let agent = std::env::args().nth(1).as_deref() == Some("agent");
    let code = if flow.rec_state().exists() {
        flow.stop()
    } else {
        flow.start(agent)
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_is_one_line_and_cut_on_a_character() {
        assert_eq!(preview("a\nb"), "a b");
        assert_eq!(preview(&"é".repeat(100)).len(), 120);
        assert_eq!(preview(&"x".repeat(200)).len(), 120);
    }

    #[test]
    fn the_conf_wins_over_the_environment_and_empty_means_default() {
        std::env::set_var("VF_TEST_SETTING", "from-env");
        let s = Settings {
            conf: Conf::parse("VF_TEST_SETTING=from-conf\nVF_TEST_EMPTY=\n"),
        };
        assert_eq!(s.get("VF_TEST_SETTING").as_deref(), Some("from-conf"));
        assert_eq!(s.get("VF_TEST_EMPTY"), None);
        let s = Settings {
            conf: Conf::default(),
        };
        assert_eq!(s.get("VF_TEST_SETTING").as_deref(), Some("from-env"));
    }
}
