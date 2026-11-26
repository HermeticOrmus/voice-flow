//! The focused window decides how a take is treated and how it is pasted.

/// Window classes that paste with Ctrl+Shift+V. A new terminal app-id goes here.
/// `org.ormus.workboard.<session>`: workboard windows (foot) that show a remote machine's session.
/// `herdr-app`: the Herdr desktop app binds Ctrl+V to image paste on Linux; text is Ctrl+Shift+V.
pub fn terminal_class(class: &str) -> bool {
    matches!(
        class,
        "foot"
            | "kitty"
            | "Alacritty"
            | "com.mitchellh.ghostty"
            | "org.wezfurlong.wezterm"
            | "org.gnome.Terminal"
            | "org.kde.konsole"
            | "herdr-app"
            | "org.omarchy.herdr"
            | "org.omarchy.terminal"
    ) || class.starts_with("org.ormus.workboard.")
}

/// The raw keycodes for `ydotool key` (ydotool 1.0.4 takes only keycodes): 29 Ctrl, 42 Shift, 47 V.
pub fn paste_chord(terminal: bool) -> &'static [&'static str] {
    if terminal {
        &["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]
    } else {
        &["29:1", "47:1", "47:0", "29:0"]
    }
}

pub fn paste_key_name(terminal: bool) -> &'static str {
    if terminal {
        "Ctrl+Shift+V"
    } else {
        "Ctrl+V"
    }
}

/// How the transcript of one app is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Clean,
    Verbatim,
}

/// `app-profiles.conf`: `<app_id glob> <profile>` per line, first match wins, a miss is `clean`.
/// The text after the glob is the profile word: only `verbatim` changes anything.
pub fn app_profile(conf: &str, class: &str) -> Profile {
    for line in conf.lines() {
        let mut parts = line.trim_start().splitn(2, char::is_whitespace);
        let pat = parts.next().unwrap_or("");
        if pat.is_empty() || pat.starts_with('#') {
            continue;
        }
        if glob_match(pat, class) {
            let prof = parts.next().unwrap_or("").trim();
            return if prof == "verbatim" {
                Profile::Verbatim
            } else {
                Profile::Clean
            };
        }
    }
    Profile::Clean
}

/// Shell `case` patterns: `*`, `?` and `[...]` (with `!`/`^` negation and ranges).
pub fn glob_match(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob(&p, &t)
}

fn glob(p: &[char], t: &[char]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some('*') => (0..=t.len()).any(|i| glob(&p[1..], &t[i..])),
        Some('?') => !t.is_empty() && glob(&p[1..], &t[1..]),
        Some('[') => match (class_end(p), t.first()) {
            (Some(end), Some(&c)) => in_class(&p[1..end], c) && glob(&p[end + 1..], &t[1..]),
            (None, Some(&c)) => c == '[' && glob(&p[1..], &t[1..]),
            _ => false,
        },
        Some('\\') if p.len() > 1 => t.first() == Some(&p[1]) && glob(&p[2..], &t[1..]),
        Some(&c) => t.first() == Some(&c) && glob(&p[1..], &t[1..]),
    }
}

fn class_end(p: &[char]) -> Option<usize> {
    let mut i = 1;
    if matches!(p.get(i), Some('!' | '^')) {
        i += 1;
    }
    if p.get(i) == Some(&']') {
        i += 1;
    }
    while i < p.len() {
        if p[i] == ']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn in_class(body: &[char], c: char) -> bool {
    let (neg, body) = match body.first() {
        Some('!' | '^') => (true, &body[1..]),
        _ => (false, body),
    };
    let mut hit = false;
    let mut i = 0;
    while i < body.len() {
        if i + 2 < body.len() && body[i + 1] == '-' {
            hit |= body[i] <= c && c <= body[i + 2];
            i += 3;
        } else {
            hit |= body[i] == c;
            i += 1;
        }
    }
    hit != neg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_paste_with_ctrl_shift_v() {
        for c in [
            "foot",
            "kitty",
            "Alacritty",
            "herdr-app",
            "org.omarchy.herdr",
            "org.ormus.workboard.ormus",
        ] {
            assert!(terminal_class(c), "{c}");
        }
        for c in ["firefox", "", "org.ormus.workboard", "Foot"] {
            assert!(!terminal_class(c), "{c}");
        }
    }

    #[test]
    fn the_chords_are_the_raw_keycodes() {
        assert_eq!(paste_chord(false).join(" "), "29:1 47:1 47:0 29:0");
        assert_eq!(paste_chord(true).join(" "), "29:1 42:1 47:1 47:0 42:0 29:0");
        assert_eq!(paste_key_name(true), "Ctrl+Shift+V");
        assert_eq!(paste_key_name(false), "Ctrl+V");
    }

    #[test]
    fn app_profile_first_match_wins_and_a_miss_is_clean() {
        let conf = "# c\nfoot  verbatim\nkit*  clean\n*  verbatim\n";
        assert_eq!(app_profile(conf, "foot"), Profile::Verbatim);
        assert_eq!(app_profile(conf, "kitty"), Profile::Clean);
        assert_eq!(app_profile(conf, "firefox"), Profile::Verbatim);
        assert_eq!(app_profile("# only a comment\n", "foot"), Profile::Clean);
    }

    #[test]
    fn globs_follow_shell_case() {
        assert!(glob_match("org.ormus.*", "org.ormus.workboard.x"));
        assert!(glob_match("f?ot", "foot"));
        assert!(glob_match("[fk]*", "kitty"));
        assert!(!glob_match("[!fk]*", "kitty"));
        assert!(glob_match("[a-c]x", "bx"));
        assert!(!glob_match("foot", "foots"));
    }
}
