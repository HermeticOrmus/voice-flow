//! `vf` with stub programs on PATH: what happens to the words when the paste chord fails.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn stub(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    fs::write(
        &p,
        format!("#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$STUB_LOG/{name}.log\"\n{body}\n"),
    )
    .unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(tag: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("vf-rig-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in ["stubs", "log", "home", "config", "data", "run"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        let s = root.join("stubs");
        stub(
            &s,
            "hyprctl",
            r#"printf '{"address":"0x1","class":"firefox"}\n'"#,
        );
        stub(
            &s,
            "ydotool",
            r#"echo "${YDOTOOL_SOCKET-unset}" >> "$STUB_LOG/socket.log"; if [ "$1" = key ]; then exit 1; fi; if [ "$1" = type ]; then cat >> "$STUB_LOG/typed.txt"; fi"#,
        );
        stub(
            &s,
            "wl-copy",
            r#"cat >> "$STUB_LOG/clipboard.txt"; printf '\n--\n' >> "$STUB_LOG/clipboard.txt""#,
        );
        stub(&s, "wl-paste", "printf OLD");
        stub(
            &s,
            "whisper-paste",
            r#"case "${VOICE_PASTE_PHASE:-}" in
  start) : > "$XDG_RUNTIME_DIR/ormus-voice/recording.state" ;;
  stop) rm -f "$XDG_RUNTIME_DIR/ormus-voice/recording.state"; printf '%s' "$STUB_TEXT" ;;
esac"#,
        );
        for n in ["voice-flow-pill", "notify-send"] {
            stub(&s, n, "");
        }
        Rig { root }
    }

    fn vf(&self) -> std::process::Output {
        self.vf_with(&[])
    }

    fn vf_with(&self, extra: &[(&str, &str)]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_vf"))
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.root.join("stubs").display()))
            .env("HOME", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_RUNTIME_DIR", self.root.join("run"))
            .env("STUB_LOG", self.root.join("log"))
            .env("STUB_TEXT", "These words are long enough that voice-flow pastes them instead of typing them one by one.")
            .env("VOICE_FLOW_PILL", "1")
            .envs(extra.iter().copied())
            .output()
            .unwrap()
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).unwrap_or_default()
    }
}

#[test]
fn a_failed_paste_chord_leaves_the_words_on_the_clipboard_and_diverts() {
    let rig = Rig::new("chord");
    assert!(rig.vf().status.success());
    let out = rig.vf();
    assert!(out.status.success(), "a divert exits 0: {out:?}");
    // The transcript is on the clipboard and the old clipboard did not come back over it.
    let clip = rig.read("log/clipboard.txt");
    assert_eq!(clip.matches("\n--\n").count(), 1, "{clip:?}");
    assert!(clip.starts_with("These words are long"), "{clip:?}");
    assert!(
        rig.read("run/ormus-voice/pill.state")
            .starts_with("copied:Ctrl+V These words"),
        "{}",
        rig.read("run/ormus-voice/pill.state")
    );
    assert!(rig.read("log/notify-send.log").contains("Ctrl+V"));
    let row = rig.read("data/ormus-voice/history.jsonl");
    assert!(
        row.contains("\"method\":\"clipboard-divert\"") && row.contains("\"diverted\":1"),
        "{row}"
    );
}

#[test]
fn an_empty_ydotool_socket_is_treated_as_unset() {
    let rig = Rig::new("sock");
    let run = |extra: &[(&str, &str)]| {
        let _ = fs::remove_file(rig.root.join("log/socket.log"));
        assert!(rig.vf_with(extra).status.success());
        assert!(rig.vf_with(extra).status.success());
        rig.read("log/socket.log")
    };
    // Short text is typed, so ydotool runs once; its environment shows the socket vf chose.
    let typed = [("STUB_TEXT", "short words"), ("YDOTOOL_SOCKET", "")];
    let seen = run(&typed);
    assert!(
        seen.trim().ends_with("/.ydotool_socket") && seen.starts_with("/run/user/"),
        "{seen:?}"
    );
}
