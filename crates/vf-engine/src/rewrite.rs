//! The clean-up call to Ollama: the same request, timeouts and length guard as bin/voice-rewrite-ollama.

use crate::{http, sys};
use serde_json::{json, Value};
use std::io::Write;
use vf_core::express;
use vf_core::text::{char_len, sed_trim, strip_trailing_newlines};

pub struct Settings {
    pub model: String,
    /// Floor of curl's `-m`; the wait grows by a second per 100 characters.
    pub timeout_floor: u64,
    /// `OLLAMA_HOST`: one base URL that skips the probe.
    pub host: Option<String>,
    /// `OLLAMA_HOSTS`, first that answers `/api/version` wins.
    pub hosts: Vec<String>,
    /// Raw `VOICE_FLOW_EXPRESS`.
    pub express: String,
    pub vocabulary: String,
    /// Text of `corrections.jsonl`, when there is one.
    pub corrections: Option<String>,
}

/// What the rewrite gives back: the cleaned text, or the input untouched.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Cleaned(String),
    /// No host, no answer, an empty answer or a length that says the model composed instead of
    /// cleaning: the caller pastes the raw input.
    Passthrough,
}

/// The JSON body, in the field order and layout `jq -n` writes. A qwen3 model is a thinking model:
/// reasoning is turned off for the rewrite path or it burns the timeout on `<think>` blocks.
pub fn request_body(model: &str, system: &str, prompt: &str) -> String {
    let mut body = json!({
        "model": model,
        "system": system,
        "prompt": prompt,
        "stream": false,
        "keep_alive": -1,
        "options": { "temperature": 0, "num_ctx": 16384 }
    });
    if model.starts_with("qwen3") {
        body["think"] = Value::Bool(false);
    }
    serde_json::to_string_pretty(&body).unwrap_or_default()
}

/// Strips the transcript tags a model sometimes echoes: `<transcript>`, `</transcript>`.
pub fn strip_transcript_tags(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let body = after.trim_start_matches('/');
        if let Some(tail) = body.strip_prefix("transcript>") {
            rest = tail;
        } else {
            out.push('<');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// The over-editing guard: a clean-up shrinks filler or converts spoken forms, it never balloons
/// or guts the text.
pub fn length_ok(input_len: usize, output_len: usize) -> bool {
    !(output_len > input_len * 5 / 2 || output_len < input_len / 4)
}

/// `.response` of Ollama's answer; anything else is empty.
fn response_text(resp: &str) -> String {
    match serde_json::from_str::<Value>(resp)
        .ok()
        .and_then(|v| v.get("response").cloned())
    {
        Some(Value::String(s)) => s,
        Some(Value::Null) | Some(Value::Bool(false)) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

pub fn rewrite(input: &str, s: &Settings) -> Outcome {
    let timeout = s.timeout_floor + (char_len(input) / 100) as u64;
    let host = match &s.host {
        Some(h) => h.clone(),
        None => match s.hosts.iter().find(|h| {
            http::curl(
                &[
                    "-s",
                    "--connect-timeout",
                    "0.4",
                    "-m",
                    "1",
                    &format!("{h}/api/version"),
                ]
                .map(String::from),
            )
            .ok()
        }) {
            Some(h) => h.clone(),
            None => return Outcome::Passthrough,
        },
    };

    let pairs = s
        .corrections
        .as_deref()
        .map(express::correction_pairs)
        .unwrap_or_default();
    let system = express::system_prompt(&s.express, &s.vocabulary, &pairs);
    let body = request_body(&s.model, &system, &express::wrapped_prompt(input));

    let args: Vec<String> = [
        "-sS",
        "-m",
        &timeout.to_string(),
        "-X",
        "POST",
        &format!("{host}/api/generate"),
        "-H",
        "content-type: application/json",
        "--data-binary",
        "@-",
    ]
    .map(String::from)
    .to_vec();
    // The body (the transcript) goes on stdin, not in argv.
    let reply = http::curl_stdin(&args, Some(body.as_bytes()));
    let resp = strip_trailing_newlines(&reply.stdout).to_string();
    debug_log(input, &resp);

    let cleaned = strip_trailing_newlines(&response_text(&resp)).to_string();
    if cleaned.is_empty() {
        return Outcome::Passthrough;
    }
    let cleaned = sed_trim(&strip_transcript_tags(&cleaned));
    if cleaned.is_empty() || !length_ok(char_len(input), char_len(&cleaned)) {
        return Outcome::Passthrough;
    }
    Outcome::Cleaned(cleaned)
}

/// `VOICE_REWRITE_DEBUG=1` keeps the input and the answer in `rewrite.log`.
fn debug_log(input: &str, resp: &str) {
    if sys::var("VOICE_REWRITE_DEBUG").as_deref() != Some("1") {
        return;
    }
    let path = sys::dirs().data.join("rewrite.log");
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let stamp = std::process::Command::new("date")
        .arg("-Iseconds")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "=== {stamp} ===\nINPUT: {input}\nRESP: {resp}");
    }
}

/// Settings from the environment and the conf, as the script reads them.
pub fn settings_from_env() -> Settings {
    let dirs = sys::dirs();
    let conf = sys::load_conf(&dirs);
    let hosts =
        sys::env_or_conf(&conf, "OLLAMA_HOSTS").unwrap_or_else(|| "http://127.0.0.1:11434".into());
    let corrections_path = sys::var("VOICE_PASTE_CORRECTIONS_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dirs.data.join("corrections.jsonl"));
    Settings {
        model: sys::var("VOICE_REWRITE_MODEL").unwrap_or_else(|| "qwen3:32b".into()),
        timeout_floor: sys::var("VOICE_REWRITE_TIMEOUT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(15),
        host: sys::var("OLLAMA_HOST"),
        hosts: hosts.split_whitespace().map(str::to_string).collect(),
        express: sys::var("VOICE_FLOW_EXPRESS").unwrap_or_else(|| "prompt".into()),
        vocabulary: std::env::var("VOICE_REWRITE_VOCABULARY").unwrap_or_default(),
        corrections: std::fs::read_to_string(corrections_path).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_has_the_jq_layout_and_order() {
        let b = request_body("llama3.2:3b", "SYS", "PROMPT");
        assert!(b.starts_with("{\n  \"model\": \"llama3.2:3b\",\n  \"system\": \"SYS\",\n  \"prompt\": \"PROMPT\",\n  \"stream\": false,\n  \"keep_alive\": -1,\n"), "{b}");
        assert!(b.contains("\"num_ctx\": 16384"));
        assert!(!b.contains("think"));
    }

    #[test]
    fn a_qwen3_model_gets_think_false_last() {
        let b = request_body("qwen3:32b", "s", "p");
        assert!(
            b.contains("\"model\": \"qwen3:32b\"") && b.trim_end().ends_with("\"think\": false\n}"),
            "{b}"
        );
    }

    #[test]
    fn tags_are_stripped_wherever_they_sit() {
        assert_eq!(
            strip_transcript_tags("<transcript>hi</transcript> there <b>x</b>"),
            "hi there <b>x</b>"
        );
        assert_eq!(strip_transcript_tags("a <//transcript> b <"), "a  b <");
    }

    #[test]
    fn the_length_guard_matches_the_integer_arithmetic_of_the_script() {
        assert!(length_ok(100, 250));
        assert!(!length_ok(100, 251));
        assert!(length_ok(100, 25));
        assert!(!length_ok(100, 24));
        assert!(length_ok(3, 0)); // integer division: 3 / 4 is 0, so a short input has no lower bound
    }

    #[test]
    fn response_text_reads_only_the_response_field() {
        assert_eq!(
            response_text(r#"{"response":"Hello there."}"#),
            "Hello there."
        );
        assert_eq!(response_text(r#"{"response":null}"#), "");
        assert_eq!(response_text("not json"), "");
        assert_eq!(response_text(""), "");
    }
}
