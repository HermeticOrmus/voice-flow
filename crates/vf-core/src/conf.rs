//! `voice-flow.conf`: shell `KEY=value` lines, as the panel and the web settings write them.
//! bin/voice-flow sources the file, so a key in the conf wins over the same key in the
//! environment; this parser reads the lines a source would assign.

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Conf {
    rows: Vec<(String, String)>,
}

impl Conf {
    pub fn parse(text: &str) -> Conf {
        let mut rows = Vec::new();
        for line in text.lines() {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").map_or(line, str::trim_start);
            let Some((key, rest)) = line.split_once('=') else {
                continue;
            };
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            rows.push((key.to_string(), unquote(rest)));
        }
        Conf { rows }
    }

    /// The last assignment of a key, as a shell would leave it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.rows
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// One shell word: double quotes (with `\"` and `\\`), single quotes, or bare up to a space or `#`.
fn unquote(raw: &str) -> String {
    let raw = raw.trim_start_matches([' ', '\t']);
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(d) = chars.next() {
                    match d {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            out.push(chars.next().unwrap_or('\\'));
                        }
                        _ => out.push(d),
                    }
                }
            }
            '\'' => {
                for d in chars.by_ref() {
                    if d == '\'' {
                        break;
                    }
                    out.push(d);
                }
            }
            ' ' | '\t' | '#' => break,
            '\\' => out.push(chars.next().unwrap_or('\\')),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_bare_and_exported_keys() {
        let c = Conf::parse(
            "# a comment\nVOICE_FLOW_METHOD=auto\nVOICE_PASTE_REMOTE_HOSTS=\"127.0.0.1:8093 192.0.2.7:8093\"\n\
             export OLLAMA_HOSTS='http://192.0.2.7:11434'\nEMPTY=\nNOTE=on # why\nbad line\n",
        );
        assert_eq!(c.get("VOICE_FLOW_METHOD"), Some("auto"));
        assert_eq!(
            c.get("VOICE_PASTE_REMOTE_HOSTS"),
            Some("127.0.0.1:8093 192.0.2.7:8093")
        );
        assert_eq!(c.get("OLLAMA_HOSTS"), Some("http://192.0.2.7:11434"));
        assert_eq!(c.get("EMPTY"), Some(""));
        assert_eq!(c.get("NOTE"), Some("on"));
        assert_eq!(c.get("missing"), None);
    }

    #[test]
    fn the_last_assignment_wins() {
        let c = Conf::parse("K=1\nK=2\n");
        assert_eq!(c.get("K"), Some("2"));
    }

    #[test]
    fn escaped_quotes_inside_double_quotes() {
        let c = Conf::parse("K=\"a \\\"b\\\" c\"\n");
        assert_eq!(c.get("K"), Some("a \"b\" c"));
    }
}
