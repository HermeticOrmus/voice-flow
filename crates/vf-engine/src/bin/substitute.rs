//! voice-substitute: stdin is a transcript, stdout the transcript with the learned corrections
//! applied. Same contract as bin/voice-substitute.

use std::io::{Read, Write};
use vf_core::subst;

fn main() {
    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let path = vf_engine::sys::var("VOICE_SUBSTITUTIONS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| vf_engine::sys::dirs().config.join("substitutions.tsv"));
    let out = match std::fs::read_to_string(path) {
        Ok(tsv) => subst::apply(&subst::parse_rules(&tsv), &text),
        Err(_) => text,
    };
    let _ = std::io::stdout().write_all(out.as_bytes());
}
