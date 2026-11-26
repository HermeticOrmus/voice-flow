//! A live recorder: start writes `pid:wav` with the pid of a running arecord, stop signals it,
//! and a pid that is not an arecord is never signalled. The `arecord` here is a stub that stays
//! alive and logs the SIGTERM it gets.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

// A direct #!/bin/bash shebang keeps the process name the script's own (arecord); through
// /usr/bin/env the kernel would name it bash.
fn script(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(tag: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("vf-rec-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in [
            "stubs",
            "log",
            "home",
            "config",
            "data/ormus-voice/models",
            "run",
            "tmp",
        ] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(
            root.join("data/ormus-voice/models/ggml-base.en.bin"),
            b"model",
        )
        .unwrap();
        let s = root.join("stubs");
        script(
            &s,
            "arecord",
            r#"for last; do :; done
printf 'RIFF' > "$last"
echo started >> "$STUB_LOG/arecord.log"
trap 'echo TERM >> "$STUB_LOG/arecord.sigterm"; exit 0' TERM
while true; do sleep 0.02; done"#,
        );
        script(&s, "whisper-cli", "printf 'LOCAL'");
        script(&s, "curl", "printf 'Hello there.'");
        Rig { root }
    }

    fn run(&self, phase: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_vf-whisper-paste"))
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("stubs").display()),
            )
            .env("HOME", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_RUNTIME_DIR", self.root.join("run"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("STUB_LOG", self.root.join("log"))
            .env("VOICE_PASTE_QUIET", "1")
            .env("VOICE_PASTE_PHASE", phase)
            .output()
            .unwrap()
    }

    fn state(&self) -> String {
        fs::read_to_string(self.root.join("run/ormus-voice/recording.state")).unwrap_or_default()
    }

    fn tmp_left(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.root.join("tmp"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

fn alive(pid: i32) -> bool {
    // A zombie still has a /proc entry; its state letter says Z.
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|s| {
            !s.rsplit(')')
                .next()
                .unwrap_or("")
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

fn wait_gone(pid: i32) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if !alive(pid) {
            return true;
        }
        sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn start_records_with_a_live_pid_and_stop_ends_it_and_leaves_no_temp_file() {
    let rig = Rig::new("live");
    assert!(rig.run("start").status.success());
    let state = rig.state();
    let (pid, wav) = state.trim_end().split_once(':').expect("pid:wav");
    let pid: i32 = pid.parse().unwrap();
    assert!(alive(pid), "the recorder runs after start");
    assert_eq!(
        fs::read_to_string(format!("/proc/{pid}/comm"))
            .unwrap()
            .trim(),
        "arecord"
    );
    assert!(Path::new(wav).starts_with(rig.root.join("tmp")));

    let out = rig.run("stop");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Hello there.");
    assert!(wait_gone(pid), "stop signalled the recorder and it is gone");
    assert_eq!(
        fs::read_to_string(rig.root.join("log/arecord.sigterm"))
            .unwrap()
            .trim(),
        "TERM"
    );
    assert_eq!(rig.state(), "", "the state file is gone");
    assert_eq!(rig.tmp_left(), Vec::<String>::new(), "no temp file is left");
}

fn spawn_sleeper() -> Child {
    Command::new("sleep")
        .arg("30")
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn a_pid_that_is_not_an_arecord_is_never_signalled() {
    let rig = Rig::new("other");
    let mut other = spawn_sleeper();
    let pid = other.id() as i32;
    let wav = rig.root.join("tmp/take.wav");
    fs::create_dir_all(rig.root.join("run/ormus-voice")).unwrap();
    fs::write(&wav, b"RIFF").unwrap();
    fs::write(
        rig.root.join("run/ormus-voice/recording.state"),
        format!("{pid}:{}", wav.display()),
    )
    .unwrap();

    let out = rig.run("stop");
    assert!(out.status.success());
    sleep(Duration::from_millis(300));
    assert!(
        alive(pid),
        "a reused pid belongs to someone else and keeps running"
    );
    other.kill().unwrap();
    let _ = other.wait();
}
