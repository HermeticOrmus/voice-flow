//! The rewrite runs in its own process group (so a deadline can kill what it started), which
//! takes it out of the group a caller kills (voice-flow-api's killpg). It must still die with
//! whisper-paste: PR_SET_PDEATHSIG.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn alive(pid: i32) -> bool {
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

#[test]
fn the_rewrite_dies_when_whisper_paste_is_killed() {
    let root = std::env::temp_dir().join(format!("vf-orphan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for d in ["stubs", "data/ormus-voice/models", "run/ormus-voice", "tmp"] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    let stub = |name: &str, body: &str| {
        let p = root.join("stubs").join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    };
    stub("curl", "printf 'Hello there.'");
    stub(
        "slow-rewrite",
        &format!("echo $$ > {}/rewrite.pid\nexec sleep 60", root.display()),
    );
    let wav = root.join("tmp/take.wav");
    fs::write(&wav, vec![0u8; 100]).unwrap();

    let mut paste = Command::new(env!("CARGO_BIN_EXE_vf-whisper-paste"))
        .arg("--file")
        .arg(&wav)
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
        .env("VOICE_PASTE_REWRITE_COMMAND", "slow-rewrite")
        .spawn()
        .unwrap();

    let pid_file = root.join("rewrite.pid");
    let start = Instant::now();
    let pid: i32 = loop {
        if let Some(p) = fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            break p;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the rewrite never started"
        );
        sleep(Duration::from_millis(20));
    };
    assert!(alive(pid));

    // SIGKILL cannot be handled: only the kernel's parent-death signal can end the rewrite now.
    paste.kill().unwrap();
    let _ = paste.wait();
    let end = Instant::now() + Duration::from_secs(3);
    while alive(pid) && Instant::now() < end {
        sleep(Duration::from_millis(20));
    }
    let still = alive(pid);
    if still {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
    assert!(!still, "the rewrite outlived whisper-paste");
}
