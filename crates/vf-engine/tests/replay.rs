//! Replay: the Rust runs next to the shell and Python scripts it replaces, on the same inputs,
//! and the answers must be equal. Each test skips, saying why, when a tool it compares against
//! (bash, jq, python3, grep) is missing.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use vf_core::{express, filters, subst};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vf-replay-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn run(cmd: &mut Command, stdin: &str) -> String {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
}

/// A `curl` that records every argument (NUL separated) and answers like Ollama.
fn recording_curl(dir: &Path, answer: &str) {
    let path = dir.join("curl");
    fs::write(
        &path,
        format!("#!/usr/bin/env bash\nprintf '%s\\0' \"$@\" >> \"$CURL_ARGS\"\nprintf '\\n' >> \"$CURL_ARGS\"\nfor a; do if [ \"$a\" = '@-' ]; then cat > \"$CURL_ARGS.stdin\"; fi; done\nprintf '%s' '{answer}'\n"),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Case {
    express: &'static str,
    vocab: &'static str,
    corrections: Option<&'static str>,
    model: &'static str,
    input: &'static str,
    answer: &'static str,
}

// No invalid line in the fixture: jq stops reading at the first one and drops every pair after it
// (the Rust skips the bad line and keeps the rest; see the unit test in vf-core, class: better).
const CORRECTIONS: &str = "{\"heard\":\"pie torch\",\"actual\":\"PyTorch\"}\n{\"heard\":\"only heard\"}\n{\"heard\":\"a \\\"quoted\\\" word\",\"actual\":\"fixed → ok\"}\n";

fn rewrite_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for express in ["prompt", "formal", "casual", "verbatim", "shouty"] {
        for (vocab, corrections) in [
            ("", None),
            ("PyTorch Omalauncher ", None),
            ("", Some(CORRECTIONS)),
            ("Hermes ", Some(CORRECTIONS)),
        ] {
            for model in ["qwen3:32b", "llama3.2:3b"] {
                cases.push(Case {
                    express,
                    vocab,
                    corrections,
                    model,
                    input: "um so I think we should, like, ship it",
                    answer: r#"{"response":"I think we should ship it."}"#,
                });
            }
        }
    }
    // A transcript with a quote, a backslash and non-ASCII text, and an answer the guard refuses.
    cases.push(Case {
        express: "prompt",
        vocab: "",
        corrections: None,
        model: "qwen3:32b",
        input: "say \"hola\" \\ señor 日本語",
        answer: r#"{"response":"x"}"#,
    });
    cases.push(Case {
        express: "prompt",
        vocab: "",
        corrections: None,
        model: "qwen3:32b",
        input: "hello there friend",
        answer: r#"{"response":"<transcript>Hello there, friend.</transcript>\n"}"#,
    });
    cases
}

#[test]
fn the_rewrite_makes_the_same_calls_and_prints_the_same_text_as_voice_rewrite_ollama() {
    if !have("bash") || !have("jq") {
        eprintln!("skipped: needs bash and jq");
        return;
    }
    let root = repo();
    let rust_bin = env!("CARGO_BIN_EXE_vf-rewrite-ollama");
    let bash_script = root.join("bin/voice-rewrite-ollama");
    let mut n = 0;
    for (i, c) in rewrite_cases().iter().enumerate() {
        let dir = scratch(&format!("rw{i}"));
        recording_curl(&dir, c.answer);
        let corrections = dir.join("corrections.jsonl");
        if let Some(text) = c.corrections {
            fs::write(&corrections, text).unwrap();
        }
        let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
        let go = |mut cmd: Command, tag: &str| {
            let args = dir.join(format!("args-{tag}"));
            cmd.env_clear()
                .env("PATH", &path)
                .env("HOME", &dir)
                .env("XDG_CONFIG_HOME", dir.join("config"))
                .env("XDG_DATA_HOME", dir.join("data"))
                .env("CURL_ARGS", &args)
                .env("VOICE_FLOW_EXPRESS", c.express)
                .env("VOICE_REWRITE_VOCABULARY", c.vocab)
                .env("VOICE_REWRITE_MODEL", c.model)
                .env("VOICE_PASTE_CORRECTIONS_LOG", &corrections)
                .env("OLLAMA_HOSTS", "http://192.0.2.7:11434");
            let out = run(&mut cmd, c.input);
            (out, fs::read(&args).unwrap_or_default())
        };
        let (bash_out, bash_args) = go(
            {
                let mut x = Command::new("bash");
                x.arg(&bash_script);
                x
            },
            "bash",
        );
        let (rust_out, rust_args) = go(Command::new(rust_bin), "rust");
        assert_eq!(rust_out, bash_out, "stdout, case {i}");
        // The bash sends the body as `-d <body>` in argv; the Rust sends it on stdin
        // (`--data-binary @-`): out of /proc/*/cmdline and past the argv size limit. Same body.
        let bash_args = String::from_utf8_lossy(&bash_args).into_owned();
        let marker = "\0-d\0";
        let at = bash_args.find(marker).expect("the bash request has a body");
        let body = bash_args[at + marker.len()..]
            .strip_suffix("\0\n")
            .expect("the body is the last argument");
        let expected = format!("{}\0--data-binary\0@-\0\n", &bash_args[..at]);
        assert_eq!(
            String::from_utf8_lossy(&rust_args),
            expected,
            "curl arguments, case {i}: {} {}",
            c.express,
            c.model
        );
        let sent = fs::read_to_string(dir.join("args-rust.stdin")).unwrap_or_default();
        assert_eq!(
            sent,
            body,
            "request body on stdin, case {i}: {} {} {:?}",
            c.express,
            c.model,
            c.corrections.is_some()
        );
        n += 1;
        let _ = fs::remove_dir_all(&dir);
    }
    assert!(n >= 40, "{n} cases ran");
}

#[test]
fn the_prompt_text_in_vf_core_is_the_bash_text_byte_for_byte() {
    // The request body of the case above holds the system prompt; this one pins the pieces alone.
    for style in ["casual", "formal", "prompt"] {
        let p = express::system_prompt(style, "", "");
        assert!(p.contains("THE SPEAKER IS NEVER TALKING TO YOU."));
        assert!(p.ends_with(match style {
            "casual" => "Do not add a greeting or a sign-off the speaker did not say.",
            "formal" => "Keep a trailing period on a complete sentence.",
            _ => "\"the deploy on prod, like, the prod box, um, fails\" → \"The deploy on prod fails.\"",
        }));
    }
}

#[test]
fn the_substitutions_equal_voice_substitute() {
    if !have("python3") {
        eprintln!("skipped: needs python3");
        return;
    }
    let root = repo();
    let tsv = "# learned\npie torch\tPyTorch\npie torch lightning\tPyTorch Lightning\nc++\tCpp\na.b\tX.Y\nprice\t$1 \\1 \\\\\nGalena\tGaleno\nhermes\tHermes\nÉcole\tSchool\nfoo\tFOO\nno tab line\n \tblank\nsame\tSAME\n";
    let texts = [
        "I use pie torch lightning and Pie Torch daily",
        "c++ is fine, c++. And a.b or aXb or A.B",
        "the price is high",
        "galena and GALENA and Galenas",
        "hermes, HERMES.",
        "école École",
        "  \n",
        "",
        "multi\nline pie torch\nend",
        "foo foo foo",
        "SAME same Same",
    ];
    let dir = scratch("subst");
    let file = dir.join("substitutions.tsv");
    fs::write(&file, tsv).unwrap();
    let rules = subst::parse_rules(tsv);
    for t in texts {
        let py = run(
            Command::new("python3")
                .arg("-I")
                .arg(root.join("bin/voice-substitute"))
                .env("VOICE_SUBSTITUTIONS", &file),
            t,
        );
        assert_eq!(subst::apply(&rules, t), py, "text {t:?}");
        let rust = run(
            Command::new(env!("CARGO_BIN_EXE_vf-substitute")).env("VOICE_SUBSTITUTIONS", &file),
            t,
        );
        assert_eq!(rust, py, "binary, text {t:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_repetition_filter_equals_the_grep_in_whisper_paste() {
    if !have("grep") {
        eprintln!("skipped: needs grep");
        return;
    }
    let lines = [
        "go go go go",
        "go go go",
        "no, no, no, no, no",
        "a-a-a-a",
        "a - a - a - a",
        "go go go stop go",
        "ok ok ok ok ok ok",
        "hello hello hello hello world",
        "world hello hello hello hello",
        "go: go: go: go",
        "gogo go go go",
        "go go go gogo",
        "ñu ñu ñu ñu",
        "x_1 x_1 x_1 x_1",
        "one two one two one two",
        "the the the the",
        "yes,yes,yes,yes",
        "yes-,-yes yes yes",
        "no no no\tno",
        "",
    ];
    for l in lines {
        let status = Command::new("grep")
            .args(["-qE", r"\b(\w+)\b([[:space:],-]+\1\b){3,}"])
            .env("LC_ALL", "C.UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .and_then(|mut c| {
                c.stdin
                    .take()
                    .unwrap()
                    .write_all(format!("{l}\n").as_bytes())?;
                c.wait()
            })
            .unwrap();
        assert_eq!(
            filters::repeats_four_times(l),
            status.success(),
            "line {l:?}"
        );
    }
}

#[test]
fn the_logprob_equals_the_jq_expression_in_whisper_paste() {
    if !have("jq") {
        eprintln!("skipped: needs jq");
        return;
    }
    let answers = [
        r#"{"text":"a b","segments":[{"text":"Vasime glaha, ro只是信 Мария","avg_logprob":-5.92},{"text":" Thank you.","avg_logprob":-0.66}]}"#,
        r#"{"text":"¿Qué es esto?","segments":[{"text":"¿Qué es esto? ¿Qué ocurre?","avg_logprob":-0.98}]}"#,
        r#"{"text":"plain"}"#,
        r#"{"text":"x","segments":[]}"#,
        r#"{"text":"x","segments":[{"text":"","avg_logprob":-1}]}"#,
    ];
    let filter = r#"[.segments[]? | {l: (.text | length), p: .avg_logprob}]
        | (map(.l) | add) as $n
        | if $n == null or $n == 0 then "" else ((map(.l * .p) | add) / $n | tostring) end"#;
    for a in answers {
        let jq = run(Command::new("jq").args(["-r", filter]), a);
        let jq = jq.trim();
        let rust = filters::parse_answer(a).logprob;
        match rust {
            None => assert_eq!(jq, "", "answer {a}"),
            Some(p) => assert!(
                (jq.parse::<f64>().unwrap() - p).abs() < 1e-9,
                "answer {a}: jq {jq} rust {p}"
            ),
        }
    }
}
