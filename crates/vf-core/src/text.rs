//! Small text rules the shell scripts get from `sed`, `wc` and `$(...)`.

/// What `sed 's/^[[:space:]]*//;s/[[:space:]]*$//'` does inside `$(...)`: each line is trimmed on
/// both sides, and the trailing newlines are dropped.
pub fn sed_trim(s: &str) -> String {
    let joined = s
        .split('\n')
        .map(|l| l.trim_matches(is_posix_space))
        .collect::<Vec<_>>()
        .join("\n");
    joined.trim_end_matches('\n').to_string()
}

/// `[[:space:]]` in the C and UTF-8 locales: space, \t, \n, \v, \f, \r.
pub fn is_posix_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r')
}

/// What `$(cmd)` does to the output: the trailing newlines go.
pub fn strip_trailing_newlines(s: &str) -> &str {
    s.trim_end_matches('\n')
}

/// `${#text}`: characters, not bytes.
pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// `wc -w`.
pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sed_trim_trims_every_line_and_drops_trailing_newlines() {
        assert_eq!(sed_trim("  a  \n  b \n\n"), "a\nb");
        assert_eq!(sed_trim("\n\n x"), "\n\nx");
        assert_eq!(sed_trim("   "), "");
    }

    #[test]
    fn counts() {
        assert_eq!(char_len("¿Qué?"), 5);
        assert_eq!(word_count(" a  b\nc "), 3);
    }
}
