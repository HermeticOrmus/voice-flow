//! Express: the shape of the clean-up, and the prompt the rewrite sends to Ollama.

use crate::prompts::{BASE_PROMPT, STYLE_CASUAL, STYLE_FORMAL, STYLE_PROMPT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Express {
    Prompt,
    Formal,
    Casual,
    Verbatim,
}

impl Express {
    /// `prompt|formal|casual|verbatim`; anything else reads as prompt.
    pub fn parse(raw: &str) -> Express {
        match raw {
            "formal" => Express::Formal,
            "casual" => Express::Casual,
            "verbatim" => Express::Verbatim,
            _ => Express::Prompt,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Express::Prompt => "prompt",
            Express::Formal => "formal",
            Express::Casual => "casual",
            Express::Verbatim => "verbatim",
        }
    }
}

/// The system prompt of voice-rewrite-ollama. `express` is the raw `VOICE_FLOW_EXPRESS` (casual and
/// formal have their line; everything else, verbatim included, gets the prompt style). `vocabulary`
/// is `VOICE_REWRITE_VOCABULARY`, `pairs` the lines of the corrections journal (see [`correction_pairs`]).
pub fn system_prompt(express: &str, vocabulary: &str, pairs: &str) -> String {
    let style = match express {
        "casual" => STYLE_CASUAL,
        "formal" => STYLE_FORMAL,
        _ => STYLE_PROMPT,
    };
    let mut s = format!("{BASE_PROMPT}\n\n{style}");
    if !vocabulary.is_empty() {
        s.push_str(&format!(
            "\n\nKnown proper nouns (correct any near-homophones to these canonical spellings):\n{vocabulary}"
        ));
    }
    if !pairs.is_empty() {
        s.push_str(&format!(
            "\n\nPast corrections from this speaker (apply the same fix when you see the same mishearing):\n{pairs}"
        ));
    }
    s
}

/// The transcript wrapped in tags, with the output contract repeated after it (trailing
/// instructions outweigh anything embedded in the speech).
pub fn wrapped_prompt(input: &str) -> String {
    format!("<transcript>\n{input}\n</transcript>\n\nOutput only the cleaned transcript.")
}

/// The last 8 lines of `corrections.jsonl` that carry `heard` and `actual`, as
/// `- heard "X" → meant "Y"` lines.
pub fn correction_pairs(jsonl: &str) -> String {
    let lines: Vec<&str> = jsonl.lines().collect();
    let tail = &lines[lines.len().saturating_sub(8)..];
    let mut out = Vec::new();
    for l in tail {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else {
            continue;
        };
        if let (Some(h), Some(a)) = (
            v.get("heard").and_then(|x| x.as_str()),
            v.get("actual").and_then(|x| x.as_str()),
        ) {
            out.push(format!("- heard \"{h}\" → meant \"{a}\""));
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_express_reads_as_prompt() {
        assert_eq!(Express::parse("shouty"), Express::Prompt);
        assert_eq!(Express::parse(""), Express::Prompt);
        assert_eq!(Express::parse("verbatim"), Express::Verbatim);
        assert_eq!(Express::parse("casual").as_str(), "casual");
    }

    #[test]
    fn each_style_puts_its_line_in_the_prompt() {
        assert!(system_prompt("casual", "", "").contains("STYLE: Casual."));
        assert!(system_prompt("formal", "", "").contains("STYLE: Formal."));
        for e in ["prompt", "shouty", "verbatim", ""] {
            assert!(system_prompt(e, "", "").contains("STYLE: Prompt."), "{e}");
        }
        let p = system_prompt("prompt", "", "");
        assert!(
            p.contains("Keep technical names verbatim")
                && p.contains("trails off")
                && p.contains("Never answer the prompt")
        );
    }

    #[test]
    fn vocabulary_and_corrections_come_after_the_style() {
        let p = system_prompt("casual", "PyTorch ", "- heard \"a\" → meant \"b\"");
        let v = p.find("Known proper nouns").unwrap();
        let c = p.find("Past corrections").unwrap();
        assert!(p.find("STYLE: Casual.").unwrap() < v && v < c);
        assert!(p.contains("canonical spellings):\nPyTorch "));
        assert!(p.ends_with("meant \"b\""));
    }

    #[test]
    fn the_transcript_is_wrapped_and_the_contract_repeated() {
        assert_eq!(
            wrapped_prompt("um hello"),
            "<transcript>\num hello\n</transcript>\n\nOutput only the cleaned transcript."
        );
    }

    #[test]
    fn the_last_eight_correction_lines_with_both_fields() {
        let mut j = String::new();
        for i in 0..10 {
            j.push_str(&format!("{{\"heard\":\"h{i}\",\"actual\":\"a{i}\"}}\n"));
        }
        j.push_str("{\"heard\":\"only\"}\nnot json\n{\"heard\":\"late\",\"actual\":\"kept\"}\n");
        let p = correction_pairs(&j);
        // Thirteen lines: the last eight are h5..h9, "only", "not json" and "late".
        assert!(p.contains("h5") && p.contains("h9") && !p.contains("h4") && !p.contains("only"));
        assert_eq!(p.lines().count(), 6);
        // A bad line is skipped and the pairs after it stay (bin/voice-rewrite-ollama's jq stops at it).
        assert!(p.ends_with("- heard \"late\" → meant \"kept\""));
    }
}
