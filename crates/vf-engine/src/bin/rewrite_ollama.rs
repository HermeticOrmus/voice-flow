//! voice-rewrite-ollama: stdin in, cleaned text out. Same contract as bin/voice-rewrite-ollama.

use std::io::{Read, Write};
use vf_core::text::strip_trailing_newlines;
use vf_engine::rewrite::{rewrite, settings_from_env, Outcome};

fn main() {
    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let raw = String::from_utf8_lossy(&raw);
    let input = strip_trailing_newlines(&raw);
    if input.is_empty() {
        return;
    }
    let out = match rewrite(input, &settings_from_env()) {
        Outcome::Cleaned(text) => text,
        Outcome::Passthrough => {
            // voice-flow-api reads this file to tell "kept as is" from "the rewrite did not happen".
            if let Some(path) = vf_engine::sys::var("VOICE_REWRITE_STATUS_FILE") {
                let _ = std::fs::write(path, "failed");
            }
            input.to_string()
        }
    };
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{out}");
}
