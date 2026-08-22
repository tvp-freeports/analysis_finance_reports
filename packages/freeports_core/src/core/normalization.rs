//! String normalization utilities used to compare fund/company names consistently
//! regardless of accents, punctuation, or casing.
//!
//! Rust port of `packages/freeports_core/src/freeports/_internals/core/normalization.py`.
//! This is the first slice of the incremental Rust rewrite described in
//! `analysis_finance_reports/agent-memory/rust-rewrite-plan.md`. The Python module now
//! delegates its public names to this crate instead of running its own implementation.

use pyo3::prelude::*;

/// Appends the normalized replacement for a single already-lowercased character to `out`.
///
/// Mirrors the `str.maketrans` table built in the Python original: accented Latin letters
/// collapse to their plain-ASCII equivalent (some, like `ß`/`œ`/`æ`, expand to more than one
/// character), separator punctuation (`,-–+`) becomes a space, noise punctuation
/// (`!?{}[]()"'’/.`) is dropped entirely, and anything else passes through unchanged.
fn push_translated(out: &mut String, c: char) {
    match c {
        'é' | 'è' | 'ê' | 'ë' => out.push('e'),
        'á' | 'à' | 'â' | 'ä' => out.push('a'),
        'í' | 'ì' | 'î' | 'ï' => out.push('i'),
        'ó' | 'ò' | 'ô' | 'ö' => out.push('o'),
        'ú' | 'ù' | 'û' | 'ü' => out.push('u'),
        'ñ' => out.push('n'),
        'ç' => out.push('c'),
        'ß' => out.push_str("ss"),
        'å' => out.push('a'),
        'ø' => out.push('o'),
        'œ' => out.push_str("oe"),
        'æ' => out.push_str("ae"),
        '&' => out.push_str("and"),
        ',' | '-' | '–' | '+' => out.push(' '),
        '!' | '?' | '{' | '}' | '[' | ']' | '(' | ')' | '"' | '\'' | '’' | '/' | '.' => {}
        other => out.push(other),
    }
}

/// Normalizes a string by lowercasing it, removing accents/punctuation noise, replacing
/// separator characters with spaces, and collapsing whitespace runs into single spaces.
pub fn deep_normalize_string(input: &str) -> String {
    let lowered = input.to_lowercase();
    let mut translated = String::with_capacity(lowered.len());
    for c in lowered.chars() {
        push_translated(&mut translated, c);
    }
    translated.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalizes a string by trimming, optionally lowercasing, and collapsing whitespace runs
/// into single spaces. Unlike [`deep_normalize_string`], no accent/punctuation handling.
pub fn normalize_string(input: &str, lower: bool) -> String {
    let trimmed = input.trim();
    let cased = if lower {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    };
    cased.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalizes a word by trimming, removing *all* internal whitespace (not just collapsing
/// it), and optionally lowercasing.
pub fn normalize_word(input: &str, lower: bool) -> String {
    let concatenated: String = input.split_whitespace().collect();
    if lower {
        concatenated.to_lowercase()
    } else {
        concatenated
    }
}

/// Normalize a string by making it lowercase and removing accents.
///
/// Converts to lowercase, removes diacritical marks, replaces separator characters
/// with spaces, drops punctuation noise, and collapses whitespace into single spaces.
// Python-visible wrapper for `deep_normalize_string`; this doc comment becomes `__doc__`.
#[pyfunction]
#[pyo3(name = "deep_normalize_string")]
pub fn py_deep_normalize_string(string: &str) -> String {
    deep_normalize_string(string)
}

/// Normalize a string by trimming, optionally lowercasing, and collapsing whitespace.
// Python-visible wrapper for `normalize_string`.
#[pyfunction]
#[pyo3(name = "normalize_string", signature = (string, lower = true))]
pub fn py_normalize_string(string: &str, lower: bool) -> String {
    normalize_string(string, lower)
}

/// Normalize a word by trimming, removing all internal whitespace, and optionally lowercasing.
// Python-visible wrapper for `normalize_word`.
#[pyfunction]
#[pyo3(name = "normalize_word", signature = (word, lower = false))]
pub fn py_normalize_word(word: &str, lower: bool) -> String {
    normalize_word(word, lower)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    #[test_case("Hello World", "hello world"; "basic lowercasing")]
    #[test_case("  Hello   World  ", "hello world"; "strips and collapses whitespace")]
    #[test_case("Café", "cafe"; "single accented letter")]
    #[test_case("MÜLLER", "muller"; "uppercase accented letters are lowered first")]
    #[test_case("Straße", "strasse"; "sharp s expands to double s")]
    #[test_case("Rock & Roll", "rock and roll"; "ampersand expands to and")]
    #[test_case("A,B-C–D+E", "a b c d e"; "separator characters become spaces")]
    #[test_case("Don't say \"no\"!", "dont say no"; "noise punctuation is deleted not spaced")]
    #[test_case("café œuf æon", "cafe oeuf aeon"; "multi character expansions")]
    #[test_case("It’s fine", "its fine"; "curly apostrophe is deleted")]
    #[test_case("Øresund", "oresund"; "scandinavian letters")]
    #[test_case("", ""; "empty string")]
    #[test_case("   ", ""; "whitespace only string")]
    fn test_deep_normalize_string(input: &str, expected: &str) {
        assert_eq!(deep_normalize_string(input), expected);
    }

    #[test_case("  Hello   World  ", true, "hello world"; "default lowering")]
    #[test_case("  Hello   World  ", false, "Hello World"; "preserves case when lower is false")]
    #[test_case("", true, ""; "empty string")]
    #[test_case("NoWhitespace", false, "NoWhitespace"; "single token unchanged")]
    fn test_normalize_string(input: &str, lower: bool, expected: &str) {
        assert_eq!(normalize_string(input, lower), expected);
    }

    #[test_case("  Hello World  ", false, "HelloWorld"; "removes all whitespace, keeps case")]
    #[test_case("  Hello World  ", true, "helloworld"; "removes all whitespace and lowers")]
    #[test_case("Test", false, "Test"; "single word unchanged")]
    #[test_case("Test", true, "test"; "single word lowered")]
    #[test_case("", false, ""; "empty string")]
    fn test_normalize_word(input: &str, lower: bool, expected: &str) {
        assert_eq!(normalize_word(input, lower), expected);
    }
}
