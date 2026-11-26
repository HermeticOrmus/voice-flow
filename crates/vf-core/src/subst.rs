//! `substitutions.tsv`: `<heard phrase> TAB <canonical form>`, applied on word boundaries,
//! case-insensitive, longest heard phrase first (the same rules as bin/voice-substitute).

use regex::{NoExpand, Regex, RegexBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub heard: String,
    pub canonical: String,
}

/// The rules of a file, in the order they apply: longest heard phrase first, file order among equals.
pub fn parse_rules(tsv: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    for line in tsv.split('\n') {
        if line.is_empty() || line.starts_with('#') || !line.contains('\t') {
            continue;
        }
        let Some((heard, canonical)) = line.split_once('\t') else {
            continue;
        };
        let (heard, canonical) = (heard.trim(), canonical.trim());
        if !heard.is_empty()
            && !canonical.is_empty()
            && heard.to_lowercase() != canonical.to_lowercase()
        {
            rules.push(Rule {
                heard: heard.to_string(),
                canonical: canonical.to_string(),
            });
        }
    }
    // A stable sort keeps the file order of equal lengths, as Python's sort does.
    rules.sort_by_key(|r| std::cmp::Reverse(r.heard.chars().count()));
    rules
}

fn pattern(heard: &str) -> Option<Regex> {
    RegexBuilder::new(&format!(r"\b{}\b", regex::escape(heard)))
        .case_insensitive(true)
        .build()
        .ok()
}

/// Applies the rules one after the other. A text with no non-space character is returned as is.
pub fn apply(rules: &[Rule], text: &str) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for r in rules {
        if let Some(p) = pattern(&r.heard) {
            out = p.replace_all(&out, NoExpand(&r.canonical)).into_owned();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(tsv: &str, text: &str) -> String {
        apply(&parse_rules(tsv), text)
    }

    #[test]
    fn replaces_on_word_boundaries_ignoring_case() {
        assert_eq!(
            run("pie torch\tPyTorch\n", "I use Pie Torch daily"),
            "I use PyTorch daily"
        );
        assert_eq!(run("cat\tdog\n", "concat cat. Cat!"), "concat dog. dog!");
    }

    #[test]
    fn the_longest_phrase_wins() {
        let tsv = "pie torch\tPyTorch\npie torch lightning\tPyTorch Lightning\n";
        assert_eq!(
            run(tsv, "pie torch lightning and pie torch"),
            "PyTorch Lightning and PyTorch"
        );
    }

    #[test]
    fn comments_blank_lines_and_rows_without_a_tab_are_skipped() {
        assert_eq!(run("# c\n\nno tab here\nfoo\tbar\n", "foo"), "bar");
    }

    #[test]
    fn a_rule_that_only_changes_case_is_dropped() {
        assert!(parse_rules("Foo\tfoo\n").is_empty());
    }

    #[test]
    fn a_backslash_in_the_canonical_form_stays_literal() {
        assert_eq!(run("path\tC:\\dir\n", "the path"), "the C:\\dir");
    }

    #[test]
    fn a_dollar_in_the_canonical_form_stays_literal() {
        assert_eq!(run("price\t$1\n", "the price"), "the $1");
    }

    #[test]
    fn special_characters_in_heard_are_literal() {
        // Python's \b needs a word character beside the edge too: "c++ " does not match, "a.b" does.
        assert_eq!(run("a.b\tX\n", "see A.B now and aXb"), "see X now and aXb");
        assert_eq!(run("c++\tCpp\n", "I like c++ a lot"), "I like c++ a lot");
    }

    #[test]
    fn blank_text_comes_back_unchanged() {
        assert_eq!(run("a\tb\n", "  \n"), "  \n");
    }
}
