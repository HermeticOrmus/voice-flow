//! What the engine hands to curl for the take: the file as a form file, the vocabulary as plain text.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn a_vocabulary_that_starts_with_at_is_text_not_a_file_to_upload() {
    let root = std::env::temp_dir().join(format!("vf-up-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for d in [
        "stubs",
        "config/ormus-voice",
        "data/ormus-voice/models",
        "run/ormus-voice",
        "tmp",
    ] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    let stub = |name: &str, body: &str| {
        let p = root.join("stubs").join(name);
        fs::write(&p, format!("#!/bin/bash\n{body}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    };
    stub("arecord", "exit 0");
    stub("whisper-cli", "printf LOCAL");
    stub(
        "curl",
        "printf '%s\\n' \"$*\" >> \"$CURL_LOG\"; printf 'Hello there.'",
    );
    fs::write(root.join("data/ormus-voice/models/ggml-base.en.bin"), b"m").unwrap();
    fs::write(
        root.join("config/ormus-voice/vocabulary.txt"),
        "@/etc/hostname\n<secret\n",
    )
    .unwrap();
    let wav = root.join("tmp/take.wav");
    fs::write(&wav, vec![0u8; 100]).unwrap();
    fs::write(
        root.join("run/ormus-voice/recording.state"),
        format!("4194300:{}", wav.display()),
    )
    .unwrap();

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
        .env("CURL_LOG", root.join("curl.log"))
        .env("VOICE_PASTE_QUIET", "1")
        .env("VOICE_PASTE_PHASE", "stop")
        .output()
        .unwrap();
    assert!(out.status.success());
    let log = fs::read_to_string(root.join("curl.log")).unwrap();
    assert!(log.contains("-F file=@"), "{log}");
    assert!(
        log.contains("--form-string prompt=@/etc/hostname <secret "),
        "{log}"
    );
    assert!(!log.contains("-F prompt="), "{log}");
}
