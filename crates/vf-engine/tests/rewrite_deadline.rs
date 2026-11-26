//! A rewrite command that hangs must not hang the dictation: after its deadline it is killed and
//! the raw transcript is pasted.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{Duration, Instant};

fn run_with_rewrite_stub(
    tag: &str,
    rewrite_body: &str,
) -> (std::path::PathBuf, std::process::Output, Duration) {
    let root = std::env::temp_dir().join(format!("vf-dl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for d in ["stubs", "data/ormus-voice/models", "run/ormus-voice", "tmp"] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    let stub = |name: &str, body: &str| {
        let p = root.join("stubs").join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    };
    stub("arecord", "exit 0");
    stub("whisper-cli", "printf LOCAL");
    stub("curl", "printf 'Hello there.'");
    stub("hung-rewrite", rewrite_body);
    fs::write(root.join("data/ormus-voice/models/ggml-base.en.bin"), b"m").unwrap();
    let wav = root.join("tmp/take.wav");
    fs::write(&wav, vec![0u8; 100]).unwrap();
    fs::write(
        root.join("run/ormus-voice/recording.state"),
        format!("4194300:{}", wav.display()),
    )
    .unwrap();

    let t0 = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_vf-whisper-paste"))
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("stubs").display()),
        )
        .env("HOME", &root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", root.join("run"))
        .env("TMPDIR", root.join("tmp"))
        .env("VOICE_PASTE_QUIET", "1")
        .env("VOICE_PASTE_PHASE", "stop")
        .env("VOICE_PASTE_REWRITE_COMMAND", "hung-rewrite")
        .env("VOICE_PASTE_REWRITE_DEADLINE", "1")
        .output()
        .unwrap();
    (root, out, t0.elapsed())
}

#[test]
fn a_hung_rewrite_command_is_killed_at_its_deadline_and_the_raw_text_comes_back() {
    let (_, out, took) = run_with_rewrite_stub("exec", "exec sleep 30");
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Hello there.");
}

#[test]
fn a_grandchild_of_the_rewrite_command_does_not_hold_the_deadline_open() {
    // The command forks `sleep` (as a script that runs curl or a CLI does) and waits for it. Killing
    // only the command would leave the sleep holding the output pipe.
    let (root, out, took) = run_with_rewrite_stub(
        "grand",
        "sleep 30 &\necho $! > \"$(dirname \"$0\")/../grandchild.pid\"\nwait",
    );
    assert!(
        took < Duration::from_secs(5),
        "the deadline is 1 s: {took:?}"
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Hello there.");
    let pid: i32 = fs::read_to_string(root.join("grandchild.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let gone = (0..50).any(|_| {
        let dead = fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| {
                s.rsplit(')')
                    .next()
                    .unwrap_or("")
                    .trim_start()
                    .starts_with('Z')
            })
            .unwrap_or(true);
        if !dead {
            std::thread::sleep(Duration::from_millis(20));
        }
        dead
    });
    if !gone {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    assert!(gone, "the grandchild sleep is killed with the group");
}

#[test]
fn what_the_command_printed_is_kept_when_a_leftover_process_holds_the_pipe() {
    // The command prints its answer and leaves a background sleep that keeps the pipe open.
    let (_, out, took) = run_with_rewrite_stub("left", "echo Cleaned.\nsleep 30 &\nexit 0");
    assert!(took < Duration::from_secs(8), "{took:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Cleaned.");
}
