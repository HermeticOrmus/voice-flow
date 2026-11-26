//! What whisper-paste decides about a take and its transcript: the silence gate, the reading of a
//! whisper server's answer, the log-prob junk gate and the hallucination filters.

use crate::text::sed_trim;
use serde_json::Value;

/// A whisper server's answer: the text, and the length-weighted mean `avg_logprob` of its segments
/// when the server sent them (openai-whisper does; whisper.cpp answers text only).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub text: String,
    pub logprob: Option<f64>,
}

/// JSON with a `text` field is read as JSON; anything else (a text-only server) is the text itself.
/// The text comes back trimmed like `sed` trims it.
pub fn parse_answer(raw: &str) -> Answer {
    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw) else {
        return Answer {
            text: sed_trim(raw),
            logprob: None,
        };
    };
    let text = match obj.get("text") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    Answer {
        text: sed_trim(&text),
        logprob: weighted_logprob(&obj),
    }
}

/// sum(len * avg_logprob) / sum(len) over `segments`; no segments, or only empty ones, is none.
fn weighted_logprob(obj: &serde_json::Map<String, Value>) -> Option<f64> {
    let segments = obj.get("segments")?.as_array()?;
    let (mut n, mut sum) = (0.0_f64, 0.0_f64);
    for s in segments {
        let len = match s.get("text") {
            Some(Value::String(t)) => t.chars().count() as f64,
            _ => 0.0,
        };
        let p = s.get("avg_logprob")?.as_f64()?;
        n += len;
        sum += len * p;
    }
    if n == 0.0 {
        None
    } else {
        Some(sum / n)
    }
}

/// The junk gate: room noise and game audio come back as fluent-looking text, but whisper's
/// log-prob on them is far below speech (measured: junk -5.9, speech -0.3 to -1.0).
pub fn is_junk(logprob: Option<f64>, min_logprob: f64) -> bool {
    logprob.is_some_and(|p| p < min_logprob)
}

/// The number the way awk reads it: a leading number, else 0.
pub fn awk_number(s: &str) -> f64 {
    let s = s.trim();
    (1..=s.len())
        .rev()
        .filter(|i| s.is_char_boundary(*i))
        .find_map(|i| s[..i].parse::<f64>().ok().filter(|v| v.is_finite()))
        .unwrap_or(0.0)
}

/// The RMS amplitude from `sox FILE -n stat` (its report goes to stderr): the third field of the
/// `RMS     amplitude:     0.0204` line.
pub fn parse_rms(sox_stat: &str) -> Option<f64> {
    let line = sox_stat.lines().find(|l| {
        let l = l.trim_start();
        l.starts_with("RMS") && l[3..].trim_start().starts_with("amplitude")
    })?;
    let value = line.split([':', ' ']).filter(|f| !f.is_empty()).nth(2)?;
    value.parse().ok()
}

/// Seconds of audio from a PCM WAV header: `(bytes - 44) / byte-rate`. An odd header reads as 0.
pub fn wav_seconds(header: &[u8], size: u64) -> u64 {
    let Some(rate) = header
        .get(28..32)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    else {
        return 0;
    };
    if (8000..=768_000).contains(&rate) && size > 44 {
        (size - 44) / u64::from(rate)
    } else {
        0
    }
}

/// Why a transcript is a whisper hallucination, if it is: a stock phrase from training data, or a
/// word repeated four times in a row.
pub fn hallucination(trimmed: &str) -> bool {
    let lower: String = trimmed
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, '.' | ',' | '!' | '?'))
        .collect();
    if matches!(
        lower.as_str(),
        "" | "you"
            | "thank you"
            | "thanks for watching"
            | "bye"
            | "okay"
            | "thank you for watching"
            | "music"
            | "the"
            | "subscribe"
    ) {
        return true;
    }
    lower.split('\n').any(repeats_four_times)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `\b(\w+)\b([[:space:],-]+\1\b){3,}` on one line: four equal words with only spaces, commas and
/// hyphens between them.
pub fn repeats_four_times(line: &str) -> bool {
    let mut run = 0usize;
    let mut prev: Option<&str> = None;
    let mut rest = line;
    while !rest.is_empty() {
        let word_start = rest.find(is_word);
        let Some(ws) = word_start else { break };
        let sep = &rest[..ws];
        let after = &rest[ws..];
        let we = after.find(|c: char| !is_word(c)).unwrap_or(after.len());
        let word = &after[..we];
        let sep_allowed = sep
            .chars()
            .all(|c| c.is_whitespace() || c == ',' || c == '-')
            && !sep.is_empty();
        if prev == Some(word) && sep_allowed {
            run += 1;
        } else {
            run = 1;
        }
        if run >= 4 {
            return true;
        }
        prev = Some(word);
        rest = &after[we..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const JUNK: &str = r#"{"text":"Vasime glaha, haiu, ro只是信 Мария gault, više. Thank you.",
        "segments":[{"text":"Vasime glaha, haiu, ro只是信 Мария gault, više.","avg_logprob":-5.92},
        {"text":" Thank you.","avg_logprob":-0.66}]}"#;

    #[test]
    fn a_json_answer_gives_text_and_a_length_weighted_logprob() {
        let a = parse_answer(JUNK);
        assert!(a.text.starts_with("Vasime"));
        let p = a.logprob.unwrap();
        assert!(p < -1.5 && p > -5.92, "{p}");
        assert!(is_junk(a.logprob, -1.5));
    }

    #[test]
    fn spanish_speech_passes_the_gate() {
        let a = parse_answer(
            r#"{"text":"¿Qué es esto?","segments":[{"text":"¿Qué es esto?","avg_logprob":-0.98}]}"#,
        );
        assert_eq!(a.text, "¿Qué es esto?");
        assert!(!is_junk(a.logprob, -1.5));
    }

    #[test]
    fn a_text_only_server_is_the_text_and_is_not_gated() {
        assert_eq!(
            parse_answer("  Hello there.  \n"),
            Answer {
                text: "Hello there.".into(),
                logprob: None
            }
        );
        assert_eq!(
            parse_answer(r#"{"text":"plain server answer"}"#).logprob,
            None
        );
        assert_eq!(parse_answer("").text, "");
        assert_eq!(parse_answer("42").text, "42");
    }

    #[test]
    fn a_null_text_is_empty() {
        assert_eq!(parse_answer(r#"{"text":null}"#).text, "");
        assert_eq!(parse_answer(r#"{}"#).text, "");
    }

    #[test]
    fn a_segment_without_a_logprob_leaves_no_number() {
        assert_eq!(
            parse_answer(r#"{"text":"a","segments":[{"text":"a"}]}"#).logprob,
            None
        );
    }

    #[test]
    fn awk_reads_a_leading_number() {
        assert_eq!(awk_number("-1.5"), -1.5);
        assert_eq!(awk_number("0.005"), 0.005);
        assert_eq!(awk_number("abc"), 0.0);
        assert_eq!(awk_number("-2x"), -2.0);
    }

    #[test]
    fn the_rms_line_of_sox() {
        let out = "Samples read:  8000\nRMS     amplitude:     0.0204\nMaximum amplitude: 0.1\n";
        assert_eq!(parse_rms(out), Some(0.0204));
        assert_eq!(parse_rms("nothing"), None);
    }

    #[test]
    fn wav_seconds_from_the_header() {
        let mut h = vec![0u8; 44];
        h[28..32].copy_from_slice(&32000u32.to_le_bytes());
        assert_eq!(wav_seconds(&h, 44 + 32000 * 300), 300);
        assert_eq!(wav_seconds(&h, 44), 0);
        assert_eq!(wav_seconds(&[0u8; 10], 5000), 0);
        h[28..32].copy_from_slice(&5u32.to_le_bytes());
        assert_eq!(wav_seconds(&h, 99999), 0);
    }

    #[test]
    fn stock_phrases_are_hallucinations() {
        for t in [
            "",
            "You.",
            "Thank you!",
            "Thanks for watching.",
            "Bye",
            "okay.",
            "Music",
            "the",
            "Subscribe",
            "Thank you for watching",
        ] {
            assert!(hallucination(t), "{t}");
        }
        for t in ["thank you very much", "Hello there.", "the cat"] {
            assert!(!hallucination(t), "{t}");
        }
    }

    #[test]
    fn four_repeats_are_a_hallucination_three_are_not() {
        assert!(hallucination("go go go go"));
        assert!(hallucination("no, no, no, no, no"));
        assert!(hallucination("a-a-a-a"));
        assert!(!hallucination("go go go"));
        assert!(!hallucination("go go go stop go"));
        assert!(!hallucination("go: go: go: go"));
        assert!(hallucination("hello\nbye bye bye bye"));
        assert!(!hallucination("bye bye\nbye bye"));
    }
}
