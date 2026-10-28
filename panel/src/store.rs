use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Express {
    Formal,
    Casual,
    Verbatim,
}

impl Express {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Formal => "formal",
            Self::Casual => "casual",
            Self::Verbatim => "verbatim",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "casual" => Self::Casual,
            "verbatim" => Self::Verbatim,
            _ => Self::Formal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Substitution {
    pub heard: String,
    pub canonical: String,
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(".config")
        });
    base.join("ormus-voice")
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(".local").join("share")
        });
    base.join("ormus-voice")
}

pub fn vocab_path() -> PathBuf {
    config_dir().join("vocabulary.txt")
}

pub fn substitutions_path() -> PathBuf {
    config_dir().join("substitutions.tsv")
}

pub fn conf_path() -> PathBuf {
    config_dir().join("voice-flow.conf")
}

pub fn last_paste_path() -> PathBuf {
    data_dir().join("last-paste.txt")
}

pub fn load_vocab(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut terms = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for term in line.split(',') {
            let term = term.trim();
            if !term.is_empty() && !terms.iter().any(|have: &String| have == term) {
                terms.push(term.to_string());
            }
        }
    }
    terms
}

pub fn save_vocab(path: &Path, terms: &[String]) -> io::Result<()> {
    let mut body = String::from("# Ormus Voice vocabulary — biases whisper + rewriter.\n");
    for term in terms {
        let term = term.trim();
        if !term.is_empty() {
            body.push_str(term);
            body.push('\n');
        }
    }
    atomic_write(path, &body)
}

pub fn load_substitutions(path: &Path) -> Vec<Substitution> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((heard, canonical)) = line.split_once('\t') else {
            continue;
        };
        let heard = heard.trim();
        let canonical = canonical.trim();
        if heard.is_empty() || canonical.is_empty() {
            continue;
        }
        rows.push(Substitution {
            heard: heard.to_string(),
            canonical: canonical.to_string(),
        });
    }
    rows
}

pub fn save_substitutions(path: &Path, rows: &[Substitution]) -> io::Result<()> {
    let mut body = String::new();
    for row in rows {
        if row.heard.trim().is_empty() || row.canonical.trim().is_empty() {
            continue;
        }
        body.push_str(row.heard.trim());
        body.push('\t');
        body.push_str(row.canonical.trim());
        body.push('\n');
    }
    atomic_write(path, &body)
}

pub fn load_express(path: &Path) -> Express {
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("VOICE_FLOW_EXPRESS=") {
            return Express::parse(value.trim_matches('"'));
        }
    }
    Express::Formal
}

pub fn save_express(path: &Path, express: Express) -> io::Result<()> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let key = format!("VOICE_FLOW_EXPRESS={}", express.as_str());
    let mut found = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("VOICE_FLOW_EXPRESS=") {
            if !found {
                out.push_str(&key);
                out.push('\n');
                found = true;
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        if text.is_empty() {
            out.push_str("# voice-flow config — written by voice-flow-panel.\n");
            out.push_str("# Sourced by voice-flow on every dictation.\n");
        }
        out.push_str(&key);
        out.push('\n');
    }
    atomic_write(path, &out)
}

pub fn load_last_paste(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn atomic_write(path: &Path, body: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "voice-flow-panel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn vocab_round_trip_splits_commas() {
        let root = dir();
        let path = root.join("vocabulary.txt");
        fs::write(&path, "# keep\nOrmus, Hermetic Ormus\nEmbry0\n").unwrap();
        let terms = load_vocab(&path);
        assert_eq!(terms, vec!["Ormus", "Hermetic Ormus", "Embry0"]);
        save_vocab(&path, &terms).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.starts_with("# Ormus Voice vocabulary"));
        assert!(saved.contains("Hermetic Ormus\n"));
        assert!(!saved.contains("Ormus, Hermetic"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn substitutions_round_trip() {
        let root = dir();
        let path = root.join("substitutions.tsv");
        fs::write(&path, "# note\nembryo zero\tEmbry0\n\n").unwrap();
        let rows = load_substitutions(&path);
        assert_eq!(
            rows,
            vec![Substitution {
                heard: "embryo zero".into(),
                canonical: "Embry0".into(),
            }]
        );
        save_substitutions(&path, &rows).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "embryo zero\tEmbry0\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn express_preserves_other_keys() {
        let root = dir();
        let path = root.join("voice-flow.conf");
        fs::write(&path, "VOICE_FLOW_METHOD=auto\nVOICE_FLOW_PILL=1\n").unwrap();
        save_express(&path, Express::Casual).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("VOICE_FLOW_METHOD=auto\n"));
        assert!(text.contains("VOICE_FLOW_PILL=1\n"));
        assert!(text.contains("VOICE_FLOW_EXPRESS=casual\n"));
        save_express(&path, Express::Verbatim).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("VOICE_FLOW_EXPRESS=").count(), 1);
        assert_eq!(load_express(&path), Express::Verbatim);
        let _ = fs::remove_dir_all(root);
    }
}
