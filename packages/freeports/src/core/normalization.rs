//! String normalisation for comparing fund and company names.
//!
//! Three pure functions of increasing aggressiveness, there to recognise as "the same name"
//! spellings that differ only by accents, punctuation, case or spacing. They are the basis of
//! [`crate::core::match_fund::MatchFund`] and of the company matching in
//! `formats_utils::text_filter`.
//!
//! The module is total — no function can fail — so it has no error type of its own.

/// Appends to `out` the normalised replacement of one already-lowercased character.
///
/// Accented Latin letters collapse onto their ASCII equivalent, and a few of them expand into more
/// than one character (`ß` → `ss`, `œ` → `oe`, `æ` → `ae`, `&` → `and`). Separating punctuation
/// (`,-–+`) becomes a space; noise punctuation (`!?{}[]()"'’/.`) disappears; everything else
/// passes through untouched.
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

/// Deep normalisation: lowercase, accents and noise punctuation removed, separators turned into
/// spaces, runs of spaces collapsed into one.
///
/// This is the form used for a fund's *identity*: two names that normalise to the same string are
/// taken to be the same fund. Being the most aggressive of the three, it is also the one that can
/// merge two genuinely different funds whose names differ only by punctuation — a trade accepted
/// because the opposite failure, missing a fund because a dash moved, is the common one in
/// practice.
///
/// # Examples
///
/// ```
/// use freeports::core::normalization::deep_normalize_string;
///
/// assert_eq!(deep_normalize_string("Éclair  Fund (EUR)"), "eclair fund eur");
/// assert_eq!(deep_normalize_string("Alpha-Beta"), deep_normalize_string("Alpha Beta"));
/// ```
pub fn deep_normalize_string(input: &str) -> String {
    let lowered = input.to_lowercase();
    let mut translated = String::with_capacity(lowered.len());
    for c in lowered.chars() {
        push_translated(&mut translated, c);
    }
    translated.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Light normalisation: trim, optional lowercasing, runs of spaces collapsed.
///
/// Unlike [`deep_normalize_string`] it touches neither accents nor punctuation, so it preserves
/// distinctions that matter when the text is meant to be shown rather than matched.
///
/// # Examples
///
/// ```
/// use freeports::core::normalization::normalize_string;
///
/// assert_eq!(normalize_string("  Éclair   Fund ", false), "Éclair Fund");
/// assert_eq!(normalize_string("  Éclair   Fund ", true), "éclair fund");
/// ```
pub fn normalize_string(input: &str, lower: bool) -> String {
    let trimmed = input.trim();
    let cased = if lower { trimmed.to_lowercase() } else { trimmed.to_string() };
    cased.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Single-word normalisation: removes *every* inner space instead of collapsing runs of them, as
/// [`normalize_string`] does, and optionally lowercases.
///
/// Meant for tokens that a PDF may have broken apart mid-word — an ISIN or a ticker split across
/// two spans comes back whole.
///
/// # Examples
///
/// ```
/// use freeports::core::normalization::normalize_word;
///
/// assert_eq!(normalize_word("LU 012 3456789", false), "LU0123456789");
/// ```
pub fn normalize_word(input: &str, lower: bool) -> String {
    let concatenated: String = input.split_whitespace().collect();
    if lower { concatenated.to_lowercase() } else { concatenated }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod deep {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("Hello World", "hello world"; "basic lowercasing")]
        #[test_case("  Hello   World  ", "hello world"; "trim and collapse spaces")]
        #[test_case("Café", "cafe"; "one accented letter")]
        #[test_case("MÜLLER", "muller"; "accented uppercase lowered first")]
        #[test_case("Straße", "strasse"; "eszett expands into double s")]
        #[test_case("Rock & Roll", "rock and roll"; "ampersand expands into and")]
        #[test_case("A,B-C–D+E", "a b c d e"; "separators become spaces")]
        #[test_case("Don't say \"no\"!", "dont say no"; "noise removed without spacing")]
        #[test_case("café œuf æon", "cafe oeuf aeon"; "multi character expansions")]
        #[test_case("It’s fine", "its fine"; "typographic apostrophe removed")]
        #[test_case("Øresund", "oresund"; "scandinavian letters")]
        #[test_case("ÁÀÂÄ ÍÌÎÏ ÓÒÔÖ ÚÙÛÜ Ñ Ç", "aaaa iiii oooo uuuu n c"; "all accent classes")]
        #[test_case("{a}[b](c)?d/e.f", "abcdef"; "all noise punctuation")]
        #[test_case("", ""; "empty string")]
        #[test_case("   ", ""; "spaces only")]
        #[test_case("...", ""; "noise only")]
        #[test_case("---", ""; "separators only")]
        fn normalizes_as_expected(input: &str, expected: &str) {
            assert_eq!(deep_normalize_string(input), expected);
        }

        #[test]
        fn is_idempotent() {
            for input in ["Café  Fund–A", "Rock & Roll", "Straße 1", "  ", "ØMEGA/AB"] {
                let once = deep_normalize_string(input);
                assert_eq!(deep_normalize_string(&once), once, "input: {input:?}");
            }
        }

        #[test]
        fn never_leaves_double_spaces_or_edges() {
            for input in ["  a - b  ", "a,,,b", "a + + b", "-a-", "  Café  ,  Fund  "] {
                let out = deep_normalize_string(input);
                assert!(!out.contains("  "), "double spaces in {out:?} (input {input:?})");
                assert_eq!(out.trim(), out, "unclean edges in {out:?}");
            }
        }
    }

    mod string {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("  Hello   World  ", true, "hello world"; "default lowercasing")]
        #[test_case("  Hello   World  ", false, "Hello World"; "keeps the case")]
        #[test_case("", true, ""; "empty string")]
        #[test_case("NoWhitespace", false, "NoWhitespace"; "single token unchanged")]
        #[test_case("Café", false, "Café"; "accents are not touched")]
        #[test_case("a\tb\nc", false, "a b c"; "tab and newline count as spaces")]
        #[test_case("A, B", false, "A, B"; "punctuation stays")]
        fn normalizes_as_expected(input: &str, lower: bool, expected: &str) {
            assert_eq!(normalize_string(input, lower), expected);
        }

        #[test]
        fn is_idempotent_for_both_casings() {
            for input in ["  A  B  ", "Café Fund", ""] {
                for lower in [true, false] {
                    let once = normalize_string(input, lower);
                    assert_eq!(normalize_string(&once, lower), once, "input {input:?} lower {lower}");
                }
            }
        }
    }

    mod word {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("  Hello World  ", false, "HelloWorld"; "removes all spaces, keeps the case")]
        #[test_case("  Hello World  ", true, "helloworld"; "removes all spaces and lowercases")]
        #[test_case("Test", false, "Test"; "single word unchanged")]
        #[test_case("Test", true, "test"; "single word lowered")]
        #[test_case("", false, ""; "empty string")]
        #[test_case("   ", false, ""; "spaces only")]
        #[test_case("a\tb\nc", false, "abc"; "tab and newline removed like spaces")]
        fn normalizes_as_expected(input: &str, lower: bool, expected: &str) {
            assert_eq!(normalize_word(input, lower), expected);
        }

        #[test]
        fn never_contains_spaces() {
            for input in ["  a b  c ", "\t\n", "x", ""] {
                assert!(!normalize_word(input, false).contains(char::is_whitespace), "input {input:?}");
            }
        }
    }

    /// The three levels are not interchangeable: this module pins down how they differ, so that
    /// accidentally changing one of them breaks a test instead of passing unnoticed.
    mod differences_between_levels {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn only_deep_removes_accents_and_punctuation() {
            let input = "Café, S.p.A.";
            assert_eq!(deep_normalize_string(input), "cafe spa");
            assert_eq!(normalize_string(input, true), "café, s.p.a.");
        }

        #[test]
        fn only_word_removes_spaces_instead_of_collapsing_them() {
            let input = "A  B";
            assert_eq!(normalize_string(input, false), "A B");
            assert_eq!(normalize_word(input, false), "AB");
        }
    }
}
