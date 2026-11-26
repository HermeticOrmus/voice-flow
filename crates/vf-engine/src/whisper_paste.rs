//! whisper-paste: record a take, gate it, send it to a whisper host (failing over down the list,
//! last resort local whisper-cli), filter junk, apply the learned corrections, optionally rewrite,
//! and print the text. Empty stdout is no paste. Same command line, environment, files and exit
//! codes as bin/whisper-paste (see its header for every tunable).

use crate::http;
use crate::notify::Notifier;
use crate::sys::{self, Cleanup};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use vf_core::conf::Conf;
use vf_core::dirs::Dirs;
use vf_core::filters;
use vf_core::subst;
use vf_core::text::{sed_trim, strip_trailing_newlines};

pub struct Cfg {
    pub mode: String,
    pub duration: u64,
    pub max_duration: u64,
    pub model_name: String,
    pub rms_min: f64,
    pub min_logprob: f64,
    pub threads: String,
    pub gpu: String,
    pub gpu_device: String,
    pub remote: String,
    pub remote_hosts: String,
    pub remote_timeout: u64,
    pub upload: String,
    pub model_path: PathBuf,
    pub vocab: String,
    pub rewrite_command: String,
    pub dirs: Dirs,
    /// voice-flow-api sets this: one small file per fact for the caller; Super+H leaves it unset.
    pub report_dir: Option<PathBuf>,
    pub notifier: Notifier,
}

impl Cfg {
    pub fn from_env() -> Cfg {
        let dirs = sys::dirs();
        let conf: Conf = sys::load_conf(&dirs);
        let model_name = sys::var("VOICE_PASTE_MODEL").unwrap_or_else(|| "base.en".into());
        let model_path = sys::var("VOICE_PASTE_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs.data
                    .join("models")
                    .join(format!("ggml-{model_name}.bin"))
            });
        let vocab_file = sys::var("VOICE_PASTE_VOCABULARY_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs.config.join("vocabulary.txt"));
        let threads = sys::var("VOICE_PASTE_THREADS").unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .to_string()
        });
        Cfg {
            mode: sys::var("VOICE_PASTE_MODE").unwrap_or_else(|| "toggle".into()),
            duration: sys::var("VOICE_PASTE_SECONDS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(8),
            max_duration: sys::var("VOICE_PASTE_MAX_SECONDS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            model_name,
            rms_min: filters::awk_number(
                &sys::var("VOICE_PASTE_RMS_MIN").unwrap_or_else(|| "0.005".into()),
            ),
            min_logprob: filters::awk_number(
                &sys::var("VOICE_PASTE_MIN_LOGPROB").unwrap_or_else(|| "-1.5".into()),
            ),
            threads,
            gpu: sys::var("VOICE_PASTE_GPU").unwrap_or_else(|| "0".into()),
            gpu_device: sys::var("VOICE_PASTE_GPU_DEVICE").unwrap_or_else(|| "1".into()),
            remote: sys::var("VOICE_PASTE_REMOTE").unwrap_or_else(|| "on".into()),
            remote_hosts: sys::env_or_conf(&conf, "VOICE_PASTE_REMOTE_HOSTS")
                .unwrap_or_else(|| "127.0.0.1:8093".into()),
            remote_timeout: sys::var("VOICE_PASTE_REMOTE_TIMEOUT")
                .and_then(|v| v.parse().ok())
                .unwrap_or(6),
            upload: sys::env_or_conf(&conf, "VOICE_PASTE_UPLOAD").unwrap_or_else(|| "wav".into()),
            model_path,
            vocab: read_vocab(&vocab_file),
            rewrite_command: std::env::var("VOICE_PASTE_REWRITE_COMMAND").unwrap_or_default(),
            notifier: Notifier {
                quiet: std::env::var("VOICE_PASTE_QUIET").as_deref() == Ok("1"),
                id_file: dirs.state.join("notify.id"),
            },
            report_dir: sys::var("VOICE_PASTE_REPORT_DIR").map(PathBuf::from),
            dirs,
        }
    }

    fn drop_file(&self) -> PathBuf {
        self.dirs.state.join("last-drop")
    }

    fn state_file(&self) -> PathBuf {
        self.dirs.state.join("recording.state")
    }

    /// Why the last take produced no text: silent, junk <logprob>, empty or hallucination.
    /// voice-flow shows it in the pill's cancel state.
    fn drop(&self, why: &str) {
        let _ = fs::write(self.drop_file(), why);
    }

    fn rep(&self, name: &str, value: &str) {
        if let Some(dir) = &self.report_dir {
            let _ = fs::write(dir.join(name), value);
        }
    }
}

/// The vocabulary file: its lines but comments and blank ones, each followed by a space (what
/// `grep -v ... | tr '\n' ' '` leaves).
pub fn read_vocab(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| format!("{l} "))
        .collect()
}

pub fn main_with(args: &[String]) -> i32 {
    let cfg = Cfg::from_env();
    let _ = fs::create_dir_all(&cfg.dirs.data);
    let _ = fs::create_dir_all(&cfg.dirs.state);

    let file_mode = if args.first().map(String::as_str) == Some("--file") {
        match args.get(1) {
            Some(p) => Some(PathBuf::from(p)),
            None => {
                eprintln!("whisper-paste: --file needs a path");
                return 1;
            }
        }
    } else {
        None
    };

    if file_mode.is_none() {
        let missing = if sys::find_in_path("arecord").is_none() {
            Some((
                4000,
                "arecord not installed (apt install alsa-utils)".to_string(),
            ))
        } else if sys::find_in_path("whisper-cli").is_none() {
            Some((4000, "whisper-cli not in PATH".to_string()))
        } else if !cfg.model_path.is_file() {
            Some((
                5000,
                format!("model not found at {}", cfg.model_path.display()),
            ))
        } else {
            None
        };
        if let Some((ms, why)) = missing {
            cfg.notifier.note(ms, "Voice paste failed", &why);
            cfg.notifier.clear();
            return 1;
        }
    }

    if let Some(path) = file_mode {
        let _consumed = Cleanup(vec![path.clone()]);
        transcribe_and_paste(&cfg, &path);
        return 0;
    }

    let phase = sys::var("VOICE_PASTE_PHASE").unwrap_or_default();
    let mut mode = cfg.mode.clone();
    let state = cfg.state_file();
    if phase == "start" {
        mode = "toggle".into();
        let _ = fs::remove_file(&state);
    }
    if phase == "stop" {
        mode = "toggle".into();
        if !state.exists() {
            cfg.notifier.note(
                2500,
                "🤫 No active recording",
                "stop phase fired without start",
            );
            cfg.notifier.clear();
            return 0;
        }
    }

    if mode == "toggle" && state.exists() {
        return stop_phase(&cfg);
    }
    if mode == "toggle" {
        return start_phase(&cfg);
    }
    fixed_mode(&cfg)
}

fn beep(sound: &str) {
    if sys::find_in_path("paplay").is_some() {
        let _ = Command::new("paplay")
            .arg("--volume=20000")
            .arg(format!("/usr/share/sounds/freedesktop/stereo/{sound}.oga"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Slice 1 records with `arecord`; native PipeWire capture is spike S3.
fn arecord_args(seconds: u64, wav: &Path) -> Vec<String> {
    [
        "-q",
        "-d",
        &seconds.to_string(),
        "-f",
        "S16_LE",
        "-r",
        "48000",
        "-c",
        "1",
        "-t",
        "wav",
        &wav.to_string_lossy(),
    ]
    .map(String::from)
    .to_vec()
}

/// First press: start arecord detached (its own process group, no inherited pipes, so the key
/// bind that ran us does not wait for it) and write `pid:wav` to the state file.
fn start_phase(cfg: &Cfg) -> i32 {
    let Ok((wav, _)) = sys::mktemp("voice", ".wav") else {
        return 1;
    };
    cfg.notifier.note(
        cfg.max_duration * 1000,
        "🎙 Recording",
        "press Super+V again to stop",
    );
    beep("message");
    use std::os::unix::process::CommandExt;
    let child = Command::new("arecord")
        .args(arecord_args(cfg.max_duration, &wav))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    let started = child.and_then(|mut c| {
        let pid = c.id();
        // The recorder is up when it has written its WAV header; one that quit with an error
        // (no device, bad flags) is a failed start now, not an empty take later.
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if let Some(status) = c.try_wait()? {
                return if status.success() {
                    Ok(pid)
                } else {
                    Err(std::io::Error::other("arecord quit"))
                };
            }
            if fs::metadata(&wav).map(|m| m.len() > 0).unwrap_or(false) {
                break;
            }
            sleep(Duration::from_millis(10));
        }
        Ok(pid)
    });
    match started {
        Ok(pid) => {
            let _ = fs::write(cfg.state_file(), format!("{pid}:{}\n", wav.display()));
            0
        }
        Err(_) => {
            cfg.notifier
                .note(4000, "Voice paste failed", "could not start arecord");
            let _ = fs::remove_file(&wav);
            cfg.notifier.clear();
            1
        }
    }
}

/// Second press: stop the recorder, then transcribe.
fn stop_phase(cfg: &Cfg) -> i32 {
    let state = fs::read_to_string(cfg.state_file()).unwrap_or_default();
    let state = state.trim_end_matches('\n');
    let (pid, wav) = state.split_once(':').unwrap_or((state, ""));
    let wav = PathBuf::from(wav);
    let _ = fs::remove_file(cfg.state_file());
    let _gone = Cleanup(vec![wav.clone()]);

    if let Ok(pid) = pid.parse::<i32>() {
        if is_recorder(pid) {
            // SAFETY: kill(2) with a pid and a signal number touches no memory.
            unsafe { libc::kill(pid, libc::SIGTERM) };
            // arecord flushes the WAV header on SIGTERM; give it a moment to finalize.
            for _ in 0..5 {
                if !pid_alive(pid) {
                    break;
                }
                sleep(Duration::from_millis(100));
            }
        }
    }
    beep("complete");
    transcribe_and_paste(cfg, &wav);
    0
}

fn pid_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the pid exists.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// The pid is ours to signal when it is alive and, where /proc says, still an arecord: a pid
/// number can be reused after the recorder ended (a take that hit its cap, a crash).
fn is_recorder(pid: i32) -> bool {
    if !pid_alive(pid) {
        return false;
    }
    match fs::read_to_string(format!("/proc/{pid}/comm")) {
        Ok(comm) => comm.trim().starts_with("arecord"),
        Err(_) => true,
    }
}

/// VOICE_PASTE_MODE=fixed: record N seconds straight through, then transcribe.
fn fixed_mode(cfg: &Cfg) -> i32 {
    let Ok((wav, _)) = sys::mktemp("voice", ".wav") else {
        return 1;
    };
    let _gone = Cleanup(vec![wav.clone()]);
    cfg.notifier.note(
        cfg.duration * 1000 + 500,
        "🎙 Recording",
        &format!("{}s — speak now", cfg.duration),
    );
    beep("message");
    let _ = Command::new("arecord")
        .args(arecord_args(cfg.duration, &wav))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    beep("complete");
    transcribe_and_paste(cfg, &wav);
    0
}

/// `sox FILE -n stat` prints its report on stderr.
fn rms_of(wav: &Path) -> Option<f64> {
    let out = Command::new("sox")
        .arg(wav)
        .args(["-n", "stat"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    filters::parse_rms(&report)
}

/// Runs the gates on a finished take and prints the text to stdout. Always clears the notify state.
pub fn transcribe_and_paste(cfg: &Cfg, wav: &Path) {
    let _ = fs::remove_file(cfg.drop_file());
    let text = transcribe(cfg, wav);
    cfg.notifier.clear();
    if let Some(text) = text {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{text}");
    }
}

fn transcribe(cfg: &Cfg, wav: &Path) -> Option<String> {
    if !wav.is_file() {
        cfg.notifier.note(2500, "🤫 Empty", "no recording captured");
        cfg.drop("empty");
        return None;
    }

    // Silence gate.
    if sys::find_in_path("sox").is_some() {
        if let Some(rms) = rms_of(wav) {
            if rms < cfg.rms_min {
                cfg.notifier.note(
                    2500,
                    "🤫 Silent",
                    &format!("no speech detected (RMS={rms})"),
                );
                cfg.drop("silent");
                return None;
            }
        }
    }

    // Mic tuning (vibecoding-record): high-pass, noise reduction, compressor, presence EQ, always
    // the 16 kHz mono file whisper.cpp expects. Without it the take goes on as it is.
    let Ok((cleaned, _)) = sys::mktemp("vibecode-clean", ".wav") else {
        cfg.notifier
            .note(4000, "Voice paste failed", "could not make a temp file");
        cfg.drop("tempfile");
        return None;
    };
    let mut _cleanup = Cleanup(vec![cleaned.clone()]);
    let tuned = sys::find_in_path("vibecoding-record").is_some()
        && Command::new("vibecoding-record")
            .arg("--process")
            .arg(wav)
            .arg(&cleaned)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    if !tuned {
        let _ = fs::copy(wav, &cleaned);
    }
    let _ = fs::remove_file(wav);

    let vocab_note = if cfg.vocab.is_empty() { "" } else { " +vocab" };
    let backend_state = cfg.dirs.data.join("backend.state");
    let t_start = Instant::now();

    // The wait grows with the take: the floor plus a tenth of the audio's seconds.
    let size = fs::metadata(&cleaned).map(|m| m.len()).unwrap_or(0);
    let mut header = [0u8; 44];
    let read = fs::File::open(&cleaned)
        .and_then(|mut f| f.read(&mut header))
        .unwrap_or(0);
    let secs = filters::wav_seconds(&header[..read], size);
    let wait_s = cfg.remote_timeout + secs / 10;

    // On a slow uplink the upload, not whisper, sets the wait; Opus at 32 kbit/s is about a
    // tenth of the WAV. A failed encode sends the WAV.
    let mut upload = cleaned.clone();
    if cfg.remote == "on" && cfg.upload == "opus" && sys::find_in_path("ffmpeg").is_some() {
        // The target is created here, exclusively and private, so ffmpeg -y never writes through
        // a path someone else planted.
        if let Ok((ogg, _)) = sys::mktemp("vibecode-clean", ".ogg") {
            _cleanup.0.push(ogg.clone());
            let encoded = Command::new("ffmpeg")
                .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
                .arg(&cleaned)
                .args([
                    "-c:a",
                    "libopus",
                    "-b:a",
                    "32k",
                    "-ac",
                    "1",
                    "-application",
                    "voip",
                ])
                .arg(&ogg)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if encoded && fs::metadata(&ogg).map(|m| m.len() > 0).unwrap_or(false) {
                upload = ogg.clone();
            }
        }
    }

    let mut trimmed = String::new();
    let mut logprob = None;
    let mut served = false;
    if cfg.remote == "on" {
        for host in cfg.remote_hosts.split_whitespace() {
            cfg.notifier.note(
                30000,
                "✍️ Transcribing",
                &format!("whisper-server @ {host}{vocab_note}…"),
            );
            // A host that ANSWERS has served the request even when the transcript is empty (VAD
            // trimmed pure silence): only a failed call moves on to the next host.
            let mut args: Vec<String> = [
                "-sSf",
                "--connect-timeout",
                "0.4",
                "-m",
                &wait_s.to_string(),
                "-F",
            ]
            .map(String::from)
            .to_vec();
            args.push(format!("file=@{}", upload.display()));
            args.extend(["-F".to_string(), "response_format=json".to_string()]);
            if !cfg.vocab.is_empty() {
                // --form-string: with -F a vocabulary starting with @ or < would make curl upload a local file.
                args.extend(["--form-string".to_string(), format!("prompt={}", cfg.vocab)]);
            }
            args.push(format!("http://{host}/inference"));
            let reply = http::curl(&args);
            if reply.ok() {
                let answer = filters::parse_answer(strip_trailing_newlines(&reply.stdout));
                trimmed = answer.text;
                logprob = answer.logprob;
                served = true;
                cfg.rep("engine", host);
                if cfg.report_dir.is_none() {
                    let _ = fs::write(&backend_state, format!("remote {host}"));
                }
                break;
            }
        }
    }

    // Local fallback: whisper-cli, cold start per call, only when no host answered at all.
    if !served {
        cfg.rep("fellback", "1");
        let (backend_note, gpu_flags) = if cfg.gpu == "1" {
            (
                format!("GPU dev {}", cfg.gpu_device),
                vec!["-dev".to_string(), cfg.gpu_device.clone()],
            )
        } else {
            (format!("CPU {}t", cfg.threads), vec!["-ng".to_string()])
        };
        cfg.notifier.note(
            30000,
            "✍️ Transcribing",
            &format!(
                "local whisper.cpp ({}, {backend_note}{vocab_note})…",
                cfg.model_name
            ),
        );
        let mut args = vec![
            "-m".to_string(),
            cfg.model_path.display().to_string(),
            "-f".into(),
            cleaned.display().to_string(),
            "-l".into(),
            "en".into(),
            "-t".into(),
            cfg.threads.clone(),
        ];
        args.extend(gpu_flags);
        args.extend(["-nt".to_string(), "-np".to_string()]);
        if !cfg.vocab.is_empty() {
            args.extend([
                "--prompt".to_string(),
                cfg.vocab.clone(),
                "--carry-initial-prompt".to_string(),
            ]);
        }
        // A failing whisper-cli (a missing library, a bad model) is an empty transcript.
        trimmed = match Command::new("whisper-cli")
            .args(&args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        {
            Ok(o) if o.status.success() => sed_trim(&String::from_utf8_lossy(&o.stdout)),
            _ => String::new(),
        };
        cfg.rep("engine", "local");
        if cfg.report_dir.is_none() {
            let _ = fs::write(&backend_state, format!("local {}", cfg.model_name));
        }
    }
    cfg.rep("transcribe_ms", &t_start.elapsed().as_millis().to_string());
    cfg.rep("raw", &trimmed);

    if trimmed.is_empty() {
        cfg.notifier
            .note(2500, "🤫 Empty transcript", "whisper produced no text");
        cfg.drop("empty");
        return None;
    }

    // Junk gate: room noise and game audio come back as fluent-looking text in any language, but
    // whisper's log-prob on them is far below speech. Only when the server gave segments.
    if filters::is_junk(logprob, cfg.min_logprob) {
        let p = logprob.unwrap_or_default();
        cfg.notifier.note(
            2500,
            "🤫 Junk transcript",
            &format!("(log-prob {p} — not pasted)"),
        );
        cfg.drop(&format!("junk {p}"));
        return None;
    }

    if filters::hallucination(&trimmed) {
        cfg.notifier.note(
            2500,
            "🤫 Hallucinated silence",
            &format!("(\"{trimmed}\" — not pasted)"),
        );
        cfg.drop("hallucination");
        return None;
    }

    // Learned corrections (voice-train): deterministic heard -> canonical fixes before any LLM,
    // so trained terms land right even in verbatim mode where the rewrite is skipped.
    let tsv_path = sys::var("VOICE_SUBSTITUTIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|| cfg.dirs.config.join("substitutions.tsv"));
    if let Ok(tsv) = fs::read_to_string(tsv_path) {
        let substituted = subst::apply(&subst::parse_rules(&tsv), &trimmed);
        let substituted = strip_trailing_newlines(&substituted);
        if !substituted.is_empty() {
            trimmed = substituted.to_string();
        }
    }

    let last_paste = cfg.dirs.data.join("last-paste.txt");
    let keep_last_paste = |text: &str| {
        // A voice-flow-api take must not touch Super+H's last-paste.txt.
        if cfg.report_dir.is_none() {
            let _ = fs::write(&last_paste, text);
        }
    };

    // Optional AI rewrite. The vocabulary rides along so the rewriter can correct mishearings to
    // canonical spellings, defense in depth beside whisper's own prompt.
    let words: Vec<&str> = cfg.rewrite_command.split_whitespace().collect();
    if let Some((program, rest)) = words.split_first() {
        cfg.notifier
            .note(30000, "🪄 Cleaning up", "rewriting via LLM…");
        let t_rw = Instant::now();
        let rewritten = run_rewrite(program, rest, &trimmed, &cfg.vocab);
        cfg.rep("rewrite_ms", &t_rw.elapsed().as_millis().to_string());
        let rewritten = sed_trim(&rewritten);
        if !rewritten.is_empty() {
            cfg.notifier
                .note(3000, "✓ Pasted (rewritten)", &head_bytes(&rewritten, 80));
            keep_last_paste(&rewritten);
            return Some(rewritten);
        }
        cfg.rep("rewrite", "failed");
        cfg.notifier.note(
            2000,
            "⚠ Rewrite failed; raw paste",
            &trimmed.chars().take(60).collect::<String>(),
        );
    }

    cfg.notifier
        .note(3000, "✓ Pasted", &head_bytes(&trimmed, 80));
    keep_last_paste(&trimmed);
    Some(trimmed)
}

fn head_bytes(s: &str, n: usize) -> String {
    let mut end = n.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// How long the rewrite command may run: `VOICE_PASTE_REWRITE_DEADLINE` seconds, else the
/// ollama command's own budget (`VOICE_REWRITE_TIMEOUT`, 15 s, plus a second per 100 characters)
/// and a 30 s margin for its host probe and a cold start.
fn rewrite_deadline(input: &str) -> Duration {
    if let Some(s) = sys::var("VOICE_PASTE_REWRITE_DEADLINE").and_then(|v| v.parse::<u64>().ok()) {
        return Duration::from_secs(s);
    }
    let floor = sys::var("VOICE_REWRITE_TIMEOUT")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(15);
    Duration::from_secs(floor + (input.chars().count() / 100) as u64 + 30)
}

/// The rewrite command reads the transcript on stdin and prints the clean text. A command that
/// fails, is missing, or runs past its deadline (it is killed) gives an empty answer, which means
/// "paste the raw text".
fn run_rewrite(program: &str, args: &[&str], input: &str, vocab: &str) -> String {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    // Its own process group, so a deadline can end the command and everything it started (a
    // script's curl or sleep): a grandchild would otherwise keep the output pipe open.
    //
    // That group is not the one a caller kills (voice-flow-api's killpg of whisper-paste's
    // group), so the child also asks the kernel to SIGKILL it when its parent dies
    // (PR_SET_PDEATHSIG). The signal fires when the parent THREAD that spawned it exits, so this
    // runs on the main thread, which lives for the whole take; never move it onto a worker thread.
    let parent = std::process::id() as i32;
    let mut command = Command::new(program);
    // SAFETY: the closure runs between fork and exec and calls only prctl(2) and getppid(2),
    // which are async-signal-safe and allocate nothing.
    unsafe {
        command.pre_exec(move || {
            libc::prctl(
                libc::PR_SET_PDEATHSIG,
                libc::SIGKILL as libc::c_ulong,
                0,
                0,
                0,
            );
            // The parent may have died before prctl took effect; then nobody will signal us.
            if libc::getppid() != parent {
                return Err(std::io::Error::other(
                    "parent died before the rewrite started",
                ));
            }
            Ok(())
        });
    }
    let Ok(mut child) = command
        .args(args)
        .process_group(0)
        .env("VOICE_REWRITE_VOCABULARY", vocab)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let input = input.to_string();
        // A thread feeds stdin so a long transcript cannot deadlock against a full stdout pipe.
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let mut stdout = child.stdout.take();
    let (tx, rx) = std::sync::mpsc::channel();
    let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let sink = std::sync::Arc::clone(&collected);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        if let Some(o) = stdout.as_mut() {
            while let Ok(n) = o.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if let Ok(mut b) = sink.lock() {
                    b.extend_from_slice(&chunk[..n]);
                }
            }
        }
        let _ = tx.send(());
    });
    let kill_group = |child: &mut std::process::Child| {
        // SAFETY: kill(2) with a negative pid signals the process group we created above.
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        let _ = child.kill();
        let _ = child.wait();
    };
    let deadline = Instant::now() + rewrite_deadline(input);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => sleep(Duration::from_millis(20)),
            _ => {
                // Whatever it printed so far is not a clean-up; the reader is not waited for.
                kill_group(&mut child);
                return String::new();
            }
        }
    }
    // The command is done; a leftover process that still holds the pipe gets a moment, then goes.
    // What the command printed by then is its answer, and is kept.
    if rx.recv_timeout(Duration::from_secs(2)).is_err() {
        kill_group(&mut child);
    }
    let out = collected.lock().map(|b| b.clone()).unwrap_or_default();
    strip_trailing_newlines(&String::from_utf8_lossy(&out)).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_is_the_lines_without_comments_each_followed_by_a_space() {
        let dir = std::env::temp_dir().join(format!("vf-vocab-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("vocabulary.txt");
        fs::write(&f, "# bias\nPyTorch\n\n  # indented comment\nOmalauncher\n").unwrap();
        assert_eq!(read_vocab(&f), "PyTorch Omalauncher ");
        assert_eq!(read_vocab(&dir.join("missing")), "");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn head_bytes_never_cuts_a_character() {
        assert_eq!(head_bytes("ééé", 3), "é");
        assert_eq!(head_bytes("abc", 80), "abc");
    }
}
