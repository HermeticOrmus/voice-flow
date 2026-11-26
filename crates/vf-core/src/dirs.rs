//! The three directories every part uses. The environment is passed in, so this stays pure.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// `~/.config/ormus-voice`
    pub config: PathBuf,
    /// `~/.local/share/ormus-voice`
    pub data: PathBuf,
    /// `$XDG_RUNTIME_DIR/ormus-voice` (the pill and recording state files)
    pub state: PathBuf,
}

impl Dirs {
    /// `get` answers an environment variable; an empty value counts as unset, like `${V:-default}`.
    pub fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Dirs {
        let var = |k: &str| get(k).filter(|v| !v.is_empty());
        let home = var("HOME").unwrap_or_default();
        let config = var("XDG_CONFIG_HOME").unwrap_or_else(|| format!("{home}/.config"));
        let data = var("XDG_DATA_HOME").unwrap_or_else(|| format!("{home}/.local/share"));
        let run = var("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".to_string());
        Dirs {
            config: PathBuf::from(config).join("ormus-voice"),
            data: PathBuf::from(data).join("ormus-voice"),
            state: PathBuf::from(run).join("ormus-voice"),
        }
    }

    pub fn conf_file(&self) -> PathBuf {
        self.config.join("voice-flow.conf")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_wins_and_empty_counts_as_unset() {
        let get = |k: &str| match k {
            "HOME" => Some("/h".to_string()),
            "XDG_CONFIG_HOME" => Some(String::new()),
            "XDG_DATA_HOME" => Some("/d".to_string()),
            _ => None,
        };
        let d = Dirs::from_env(&get);
        assert_eq!(d.config, PathBuf::from("/h/.config/ormus-voice"));
        assert_eq!(d.data, PathBuf::from("/d/ormus-voice"));
        assert_eq!(d.state, PathBuf::from("/tmp/ormus-voice"));
        assert_eq!(
            d.conf_file(),
            PathBuf::from("/h/.config/ormus-voice/voice-flow.conf")
        );
    }
}
