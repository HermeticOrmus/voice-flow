//! One line per dictation in `history.jsonl`: counts only, never the transcript, unless
//! `VOICE_FLOW_LOG_TEXT=1`. The field order and spelling are what the panel and the web read.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<'a> {
    /// `date --iso-8601=seconds`
    pub ts: &'a str,
    pub words: usize,
    pub chars: usize,
    pub model: &'a str,
    pub method: &'a str,
    pub rewrite: bool,
    pub transcribe_ms: u64,
    pub diverted: bool,
    /// The transcript; only when the conf turns it on.
    pub text: Option<&'a str>,
}

impl Record<'_> {
    /// The JSON line, without its newline.
    pub fn to_line(&self) -> String {
        let mut s = format!(
            "{{\"ts\":\"{}\",\"words\":{},\"chars\":{},\"model\":\"{}\",\"method\":\"{}\",\"rewrite\":{},\"transcribe_ms\":{},\"diverted\":{}",
            self.ts,
            self.words,
            self.chars,
            self.model,
            self.method,
            self.rewrite,
            self.transcribe_ms,
            u8::from(self.diverted),
        );
        if let Some(t) = self.text {
            s.push_str(&format!(",\"text\":{}", Value::String(t.to_string())));
        }
        s.push('}');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(text: Option<&str>) -> Record<'_> {
        Record {
            ts: "2026-11-25T10:00:00-05:00",
            words: 3,
            chars: 17,
            model: "medium.en",
            method: "type",
            rewrite: true,
            transcribe_ms: 1234,
            diverted: false,
            text,
        }
    }

    #[test]
    fn the_line_is_what_the_bash_script_writes() {
        assert_eq!(
            rec(None).to_line(),
            r#"{"ts":"2026-11-25T10:00:00-05:00","words":3,"chars":17,"model":"medium.en","method":"type","rewrite":true,"transcribe_ms":1234,"diverted":0}"#
        );
    }

    #[test]
    fn the_text_rides_along_only_when_given() {
        let line = rec(Some("say \"hi\"\n")).to_line();
        assert!(line.ends_with(r#","text":"say \"hi\"\n"}"#), "{line}");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["words"], 3);
        assert!(!rec(None).to_line().contains("\"text\""));
    }
}
