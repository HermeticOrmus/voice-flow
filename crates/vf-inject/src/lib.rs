//! Where the words go: the focus guard (`hyprctl`), typing (`ydotool type`) and clipboard paste
//! with the old clipboard put back. Every call goes through the same programs bin/voice-flow
//! uses, found on PATH, so the stubs of the test suite stand in for the compositor and the keyboard.
//!
//! WHY ydotool: on Wayland an app cannot synthesize input into another app's window. ydotool
//! injects below the compositor through /dev/uinput (an ACL grants this user access).

use serde_json::Value;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;
use vf_core::profile::{paste_chord, paste_key_name, terminal_class};

/// The focused window as Hyprland reports it; a field it does not give reads as empty.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Window {
    pub address: String,
    pub class: String,
}

/// One `hyprctl activewindow -j` call. No answer (no Hyprland, no window) is the default window.
pub fn active_window() -> Window {
    let out = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = out else {
        return Window::default();
    };
    let Ok(v) = serde_json::from_slice::<Value>(&out.stdout) else {
        return Window::default();
    };
    let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Window {
        address: field("address"),
        class: field("class"),
    }
}

/// A stable key for the current focus: the active window's address, which holds for the life of
/// the window. Empty means unknown, and callers do not divert on unknown (the guard fails open).
pub fn focus_id() -> String {
    active_window().address
}

pub fn active_class() -> String {
    active_window().class
}

/// The paste key name of the focused window: `Ctrl+Shift+V` in a terminal, else `Ctrl+V`.
pub fn current_paste_key_name() -> &'static str {
    paste_key_name(terminal_class(&active_class()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Auto,
    Type,
    Paste,
    Other(String),
}

impl Method {
    pub fn parse(raw: &str) -> Method {
        match raw {
            "auto" => Method::Auto,
            "type" => Method::Type,
            "paste" => Method::Paste,
            other => Method::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Injector {
    /// `$YDOTOOL`, default `ydotool`.
    pub ydotool: String,
    pub method: Method,
    /// `auto`: text up to this many characters is typed, longer text is pasted.
    pub type_max: usize,
}

#[derive(Debug)]
pub enum InjectError {
    UnknownMethod(String),
    /// The text is on the clipboard but the paste chord failed: the caller diverts, it does not
    /// restore the old clipboard over the words.
    PasteChordFailed(io::Error),
    Io(io::Error),
}

impl From<io::Error> for InjectError {
    fn from(e: io::Error) -> Self {
        InjectError::Io(e)
    }
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectError::UnknownMethod(m) => {
                write!(f, "voice-flow: unknown VOICE_FLOW_METHOD='{m}'")
            }
            InjectError::PasteChordFailed(e) => write!(f, "voice-flow: paste chord failed: {e}"),
            InjectError::Io(e) => write!(f, "voice-flow: injection failed: {e}"),
        }
    }
}

/// Whisper wraps long transcripts with line breaks; a typed newline is an Enter keypress and
/// submits chat inputs mid-sentence. Flatten to spaces.
pub fn flatten(text: &str) -> String {
    text.replace('\n', " ")
}

impl Injector {
    /// What `inject` will do with the text, for the history log: `type` or `paste`; an unknown
    /// method is returned as written.
    pub fn resolve_method(&self, text: &str) -> String {
        match &self.method {
            Method::Auto if text.chars().count() <= self.type_max => "type".into(),
            Method::Auto | Method::Paste => "paste".into(),
            Method::Type => "type".into(),
            Method::Other(m) => m.clone(),
        }
    }

    pub fn inject(&self, text: &str) -> Result<(), InjectError> {
        let text = flatten(text);
        match self.resolve_method(&text).as_str() {
            "type" => self.type_text(&text).map_err(Into::into),
            "paste" => self.paste_text(&text),
            other => Err(InjectError::UnknownMethod(other.to_string())),
        }
    }

    fn type_text(&self, text: &str) -> io::Result<()> {
        let mut child = Command::new(&self.ydotool)
            .args(["type", "--file", "-"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        // A closed pipe means ydotool quit early; its exit status says so below.
        let _ = child.stdin.take().map(|mut s| s.write_all(text.as_bytes()));
        check(child.wait()?, "ydotool type")
    }

    /// Clipboard, paste chord, clipboard back. When the chord fails the words stay on the
    /// clipboard (as in bin/voice-flow) and the caller diverts. A non-text clipboard (an image)
    /// is lost: wl-paste -n only reads text.
    fn paste_text(&self, text: &str) -> Result<(), InjectError> {
        let saved = Command::new("wl-paste")
            .arg("-n")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default();
        wl_copy(text.as_bytes())?;
        sleep(Duration::from_millis(50));
        self.paste_chord().map_err(InjectError::PasteChordFailed)?;
        sleep(Duration::from_millis(150));
        let _ = wl_copy(&saved);
        Ok(())
    }

    fn paste_chord(&self) -> io::Result<()> {
        let terminal = terminal_class(&active_class());
        let status = Command::new(&self.ydotool)
            .arg("key")
            .args(paste_chord(terminal))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        check(status, "ydotool key")
    }
}

fn check(status: std::process::ExitStatus, what: &str) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{what} exited with {status}")))
    }
}

/// `wl-copy` with the text on stdin. wl-copy forks to hold the selection, so its streams go to
/// /dev/null and this returns when the parent exits.
pub fn wl_copy(bytes: &[u8]) -> io::Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let _ = child.stdin.take().map(|mut s| s.write_all(bytes));
    check(child.wait()?, "wl-copy")
}

/// Focus moved during the transcription: the text goes to the clipboard and nowhere else.
pub fn divert_to_clipboard(text: &str) {
    let _ = wl_copy(text.as_bytes());
}

/// Rare-event toast for the divert case. It does not follow VOICE_PASTE_QUIET: the pill cannot say
/// "press Ctrl+V", and the person needs to know the words wait on the clipboard.
pub fn note_divert(key: &str) {
    if find_in_path("notify-send").is_none() {
        return;
    }
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "Voice Flow",
            "-t",
            "6000",
            "--",
            "Focus moved — transcript copied",
        ])
        .arg(format!("Press {key} to paste it where you want it."))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// `command -v`: the first executable of that name on PATH.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

fn is_executable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inj(method: &str) -> Injector {
        Injector {
            ydotool: "ydotool".into(),
            method: Method::parse(method),
            type_max: 80,
        }
    }

    #[test]
    fn auto_types_up_to_the_limit_and_pastes_beyond_it() {
        let i = inj("auto");
        assert_eq!(i.resolve_method(&"a".repeat(80)), "type");
        assert_eq!(i.resolve_method(&"a".repeat(81)), "paste");
        assert_eq!(i.resolve_method(&"é".repeat(80)), "type"); // characters, not bytes
    }

    #[test]
    fn a_fixed_method_stays_and_an_unknown_one_is_named() {
        assert_eq!(inj("type").resolve_method(&"a".repeat(500)), "type");
        assert_eq!(inj("paste").resolve_method("a"), "paste");
        assert_eq!(inj("telepathy").resolve_method("a"), "telepathy");
        match inj("telepathy").inject("a") {
            Err(InjectError::UnknownMethod(m)) => assert_eq!(m, "telepathy"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn line_breaks_become_spaces() {
        assert_eq!(flatten("first line\nsecond line"), "first line second line");
    }

    #[test]
    fn find_in_path_skips_what_is_not_executable() {
        assert!(find_in_path("definitely-not-a-program-vf").is_none());
    }
}
