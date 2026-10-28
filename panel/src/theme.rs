//! The desktop theme. Omarchy keeps the current theme at
//! `$XDG_STATE_HOME/omarchy/current/theme/colors.toml`; the panel draws with
//! seven roles taken from it, and falls back to the built-in Ormus palette.
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct Theme {
    pub ground: String,
    pub raised: String,
    pub ink: String,
    pub muted: String,
    pub accent: String,
    pub warn: String,
    pub ok: String,
}

pub fn colors_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(".local").join("state")
        });
    base.join("omarchy")
        .join("current")
        .join("theme")
        .join("colors.toml")
}

/// `key = "#RRGGBB"` lines; anything else is skipped.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let is_hex = value.len() == 7
            && value.starts_with('#')
            && value[1..].chars().all(|c| c.is_ascii_hexdigit());
        if is_hex {
            map.insert(key.trim().to_string(), value.to_uppercase());
        }
    }
    map
}

pub fn from_map(map: &HashMap<String, String>) -> Theme {
    let pick = |keys: &[&str], fallback: &str| {
        keys.iter()
            .find_map(|k| map.get(*k).cloned())
            .unwrap_or_else(|| fallback.to_string())
    };
    Theme {
        ground: pick(&["background"], "#0B0B0D"),
        raised: pick(&["lighter_background"], "#141318"),
        ink: pick(&["foreground"], "#E8E4DA"),
        muted: pick(&["dark_foreground", "muted"], "#8A857A"),
        accent: pick(&["accent"], "#D29E3D"),
        warn: pick(&["red"], "#CC6633"),
        ok: pick(&["bright_green", "green"], "#7ABB9A"),
    }
}

pub fn load() -> Theme {
    from_map(&parse(
        &fs::read_to_string(colors_path()).unwrap_or_default(),
    ))
}

/// The accent marks one thing only: the tone that is set.
pub fn css(t: &Theme) -> String {
    format!(
        "window {{ background: {ground}; }}
         label {{ color: {ink}; }}
         .title {{ color: {ink}; font-weight: 700; font-size: 18px; }}
         .muted {{ color: {muted}; font-size: 12px; }}
         .section {{ color: {muted}; font-weight: 700; font-size: 11px; }}
         entry, textview, textview text {{ background: {raised}; color: {ink}; }}
         list, row {{ background: {raised}; color: {ink}; }}
         button {{ background: {raised}; color: {ink}; }}
         button:checked {{ background: {accent}; color: {ground}; }}
         .warn {{ color: {warn}; }}
         .ok {{ color: {ok}; }}",
        ground = t.ground,
        ink = t.ink,
        muted = t.muted,
        raised = t.raised,
        accent = t.accent,
        warn = t.warn,
        ok = t.ok,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORMUS_SUN: &str = "# Ormus Aurum\nmode = \"dark\"\naccent    = \"#FFD700\"\nbackground = \"#090A16\"\nforeground = \"#F5E6C8\"\ndark_foreground = \"#8A7434\"\nred = \"#CC6633\"\ngreen = \"#5A9A7A\"\nbright_green = \"#7ABB9A\"\nhyprland_active_border = \"rgba(FFD700ff) 45deg\"\n";

    #[test]
    fn desktop_colors_fill_the_roles() {
        let t = from_map(&parse(ORMUS_SUN));
        assert_eq!(t.ground, "#090A16");
        assert_eq!(t.accent, "#FFD700");
        assert_eq!(t.ok, "#7ABB9A");
        assert_eq!(t.raised, "#141318"); // absent key keeps the built-in
    }

    #[test]
    fn no_file_means_the_built_in_palette() {
        let t = from_map(&HashMap::new());
        assert_eq!(t.ground, "#0B0B0D");
        assert_eq!(t.accent, "#D29E3D");
    }

    #[test]
    fn css_carries_the_accent_once_for_the_checked_tone() {
        let css = css(&from_map(&parse(ORMUS_SUN)));
        assert_eq!(css.matches("#FFD700").count(), 1);
        assert!(css.contains("button:checked { background: #FFD700"));
    }
}
