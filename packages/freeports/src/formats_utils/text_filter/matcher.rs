//! [`CompanyMatchInfos`] and [`match_company`]: deciding whether a piece of text names one of the
//! companies being looked for.
//!
//! A holding is written differently in every report — the full legal name, an abbreviation, a
//! ticker, a name split across two table cells — so a company is described by four things at once:
//!
//! - its **name**, matched on the deeply normalised form — so accents, case and punctuation do
//!   not matter — and only where no letter touches it on either side;
//! - **buds**, verbatim fragments that must occur in the text before its regexes are even tried;
//! - **regexs**, patterns for the shapes its name takes;
//! - **symbols**, tickers, matched as whole words.
//!
//! # Two passes, cheap first
//!
//! [`match_company`] runs `match_fast`, and only if that finds nothing — finding nothing, not
//! failing — falls back to `match_long`.
//!
//! `match_fast` tries the normalised name, then, for each bud actually present in the text, that
//! company's regexes in order. It never looks at symbols.
//!
//! The name step is a substring search and not a regex — that is what makes the pass fast — but a
//! bare substring search is not enough: `SSE` occurs inside `Other Assets`. So the occurrence has
//! to be delimited by something that is not a letter, checked in one character on each side, which
//! keeps the step as cheap as it was. [`text_names_the_company`] is that rule, and it also says why
//! the text is read in two ways rather than one.
//!
//! `match_long` drops the bud requirement: it tries every symbol, then every regex of every
//! company. The split matters because the second pass is the expensive one, and on a table of hundreds of rows against hundreds of companies it
//! is the difference between a fast run and an unusable one.
//!
//! If two different companies both match by regex, that is [`MatcherError::AmbiguousRegex`] rather
//! than an arbitrary winner: silently picking one would attribute a holding to the wrong company,
//! which is worse than refusing to guess.
//!
//! # Anchors
//!
//! A `^` or `$` in a pattern is **removed** from the pattern string and never put back; only the
//! opposite side receives a `.*`. The anchoring still holds, because every match here goes through
//! whole-string matching, which starts at position 0 and must cover the entire string. `mod
//! match_companies` pins that end to end.
//!
//! Normalisation is not reimplemented here: it is
//! [`crate::core::normalization::deep_normalize_string`], and `mod normalize_string_equivalence`
//! checks that rather than assuming it.

use onig::{Regex as OnigRegex, RegexOptions, Syntax};
use std::sync::Arc;

use crate::core::normalization::{deep_normalize_string, deep_normalize_string_split_on_punctuation};

#[derive(Debug, Clone)]
struct Regex {
    pattern: String,
    reference: Arc<OnigRegex>,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid pattern `{pattern}`: {message}")]
pub struct PatternCompileError {
    pattern: String,
    message: String,
}

/// Anchor-aware wrapping of a regex pattern: `^foo` and `foo$` lose the anchor character and gain
/// `.*` on the opposite side only; an unanchored pattern is wrapped on both.
fn compile_regex_pattern(p: &str) -> Result<Regex, PatternCompileError> {
    let mut modified_pattern: String;
    let start = p.starts_with('^');
    let end = p.ends_with('$');
    if start || end {
        modified_pattern = p.to_string();
        if start {
            modified_pattern.remove(0);
        } else {
            modified_pattern.insert_str(0, ".*");
        }
        if end {
            modified_pattern.pop();
        } else {
            modified_pattern.push_str(".*");
        }
    } else {
        modified_pattern = format!(".*{p}.*");
    }
    let reference = OnigRegex::with_options(
        modified_pattern.as_str(),
        RegexOptions::REGEX_OPTION_IGNORECASE | RegexOptions::REGEX_OPTION_MULTILINE,
        Syntax::default(),
    )
    .map_err(|e| PatternCompileError { pattern: p.to_string(), message: e.description().to_string() })?;
    Ok(Regex { pattern: modified_pattern, reference: Arc::new(reference) })
}

/// Word-boundary wrapping of a symbol pattern — a ticker, matched as a whole word.
fn compile_symbol_pattern(p: &str) -> Result<Regex, PatternCompileError> {
    let modified_pattern = format!(r".*\b{p}\b.*");
    let reference = OnigRegex::with_options(
        modified_pattern.as_str(),
        RegexOptions::REGEX_OPTION_MULTILINE,
        Syntax::default(),
    )
    .map_err(|e| PatternCompileError { pattern: p.to_string(), message: e.description().to_string() })?;
    Ok(Regex { pattern: modified_pattern, reference: Arc::new(reference) })
}

pub struct TargetCompanyInput {
    pub name: String,
    pub regexs: Vec<String>,
    pub symbols: Vec<String>,
    pub buds: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CompanyMatchInfos {
    name: String,
    n_name: String,
    buds: Vec<String>,
    regexs: Vec<Regex>,
    symbols: Vec<Regex>,
}

impl CompanyMatchInfos {
    /// The company's original name, unnormalised.
    ///
    /// Needed at the Python boundary, which has to hand the target companies to an author-written
    /// `text_filter` pipe.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The deeply normalised form of the name, the one comparisons are made against.
    pub fn normalized_name(&self) -> &str {
        &self.n_name
    }

    pub fn compile_from_target_companies(
        companies: Vec<TargetCompanyInput>,
    ) -> Result<Vec<Self>, PatternCompileError> {
        let n_companies = companies.len();
        let compiled = companies
            .into_iter()
            .map(|company| {
                let regexs = company
                    .regexs
                    .iter()
                    .map(|p| compile_regex_pattern(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let symbols = company
                    .symbols
                    .iter()
                    .map(|p| compile_symbol_pattern(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(CompanyMatchInfos {
                    n_name: deep_normalize_string(&company.name),
                    name: company.name,
                    buds: company.buds,
                    regexs,
                    symbols,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        tracing::debug!(companies = n_companies, "target company match patterns compiled");
        Ok(compiled)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MatcherError<'a> {
    #[error(
        "ambiguous match for {text:?}: both {origin_company:?} ({origin_match:?}) and {other_company:?} ({other_match:?})"
    )]
    AmbiguousRegex {
        text: &'a str,
        origin_company: &'a str,
        other_company: &'a str,
        origin_match: &'a str,
        other_match: &'a str,
    },
}

type MatchResult<'a> = Result<Option<&'a str>, MatcherError<'a>>;

/// Whether `reading` contains `n_name` as a name of its own: present as a substring, and with no
/// letter on either side of the occurrence.
///
/// **A letter, not a word character.** A digit is an acceptable boundary, and so is any punctuation
/// the normalisation leaves behind: reports write `3M`, `SSE 4.75% 2031`, `EDP 20-30`, and a rule
/// demanding whitespace would lose them. Only a letter touching the occurrence means the text is
/// saying a longer word, which is the failure being ruled out.
///
/// **Still the shortcut.** [`str::match_indices`] uses the same substring search [`str::contains`]
/// does, and the boundary test looks at one character on each side of an occurrence — of which
/// there is normally none or one. That keeps the fast pass fast, which is the whole reason it is
/// not a regex: on hundreds of table rows against hundreds of companies, compiling and running
/// `\bname\b` for every pair is exactly the cost this pass exists to avoid.
///
/// An empty `n_name` never matches. Plain containment says every text contains the empty string,
/// which would make a company whose name normalises away claim every row on the page.
fn contains_delimited(reading: &str, n_name: &str) -> bool {
    if n_name.is_empty() {
        return false;
    }
    reading.match_indices(n_name).any(|(start, occurrence)| {
        let before = reading[..start].chars().next_back();
        let after = reading[start + occurrence.len()..].chars().next();
        !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphabetic)
    })
}

/// Whether the text names the company outright — the name shortcut of [`match_fast`], and the one
/// step of the matcher that reaches a verdict without a regex.
///
/// Plain containment is not enough, and the case that made that clear is real: the company `SSE`
/// occurs inside `Other Assets`, which normalises to `other a·sse·ts`, so every row of every report
/// reading "Other Assets" was being booked as a holding of SSE. Hence [`contains_delimited`].
///
/// **Two readings of the same text**, because the boundary the report printed can be gone by the
/// time the check runs. [`deep_normalize_string`] *erases* the noise punctuation — right for
/// deciding that two strings name the same company, fatal here, since `AMAZON.COM INC` becomes
/// `amazoncom inc` and `Amazon` is then followed by a letter the report never wrote next to it. So
/// the occurrence is looked for both in the normalised text and in the reading where that
/// punctuation separates instead of vanishing, and one delimited occurrence in either is a match.
///
/// The company's name is used in its ordinary normalised form against both, and deliberately so:
/// the second reading is there to expose a boundary the erasure hid, not to respell the name.
fn text_names_the_company(readings: (&str, &str), n_name: &str) -> bool {
    let (normalized, split_on_punctuation) = readings;
    contains_delimited(normalized, n_name) || contains_delimited(split_on_punctuation, n_name)
}

/// Tries the name as a delimited whole (see [`text_names_the_company`]), then, for each bud present
/// in the text, that company's regexes in the order given, stopping at the first that matches.
/// Never looks at symbols.
fn match_fast<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos]) -> MatchResult<'a> {
    let txt = deep_normalize_string(text);
    // The second reading the name shortcut needs, computed once for the text rather than once per
    // company — the loop below runs it against every one of them.
    let split_txt = deep_normalize_string_split_on_punctuation(text);
    let mut last_matching_regex: Option<(&str, &str)> = None;
    let mut res: MatchResult<'a> = Ok(None);

    for c in target_companies {
        // Only the **positive** outcome is logged, with the text that produced it. Tracing every
        // comparison meant hundreds of companies against every fragment of text on every page,
        // nearly all of them saying "no", which is not information. The text in the clear is the
        // useful part — it can be found again with a search inside the PDF — which is why it is
        // the *first* anchor and the company only the second.
        if text_names_the_company((&txt, &split_txt), &c.n_name) {
            tracing::trace!(coord_ref_1 = %text, coord_ref_2 = %c.name, "company matched by its name");
            return Ok(Some(&c.name));
        }
        for b in &c.buds {
            if txt.contains(b) {
                for Regex { pattern, reference: r } in &c.regexs {
                    if r.is_match(&txt) {
                        tracing::trace!(
                            coord_ref_1 = %text,
                            coord_ref_2 = %c.name,
                            pattern,
                            "company matched by one of its regexes"
                        );
                        match &last_matching_regex {
                            None => {
                                last_matching_regex = Some((&c.name, pattern));
                                res = Ok(Some(&c.name));
                            }
                            Some((company, reg)) => {
                                return Err(MatcherError::AmbiguousRegex {
                                    text,
                                    origin_company: company,
                                    other_company: &c.name,
                                    origin_match: reg,
                                    other_match: pattern,
                                });
                            }
                        }
                        break;
                    }
                }
                break;
            }
        }
    }
    res
}

/// Tries every symbol against the unnormalised text, then every regex of every company without
/// requiring a bud to be present. Same ambiguity rule as `match_fast`.
fn match_long<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos]) -> MatchResult<'a> {
    let txt = deep_normalize_string(text);
    let mut last_matching_regex: Option<(&str, &str)> = None;
    let mut res: MatchResult<'a> = Ok(None);

    for c in target_companies {
        if c.symbols.iter().any(|s| s.reference.is_match(text)) {
            tracing::trace!(coord_ref_1 = %text, coord_ref_2 = %c.name, "company matched by one of its symbols");
            return Ok(Some(&c.name));
        }
        for Regex { pattern, reference: r } in &c.regexs {
            if r.is_match(&txt) {
                tracing::trace!(
                    coord_ref_1 = %text,
                    coord_ref_2 = %c.name,
                    pattern,
                    "company matched by one of its regexes"
                );
                match &last_matching_regex {
                    None => {
                        last_matching_regex = Some((&c.name, pattern));
                        res = Ok(Some(&c.name));
                    }
                    Some((company, reg)) => {
                        return Err(MatcherError::AmbiguousRegex {
                            text,
                            origin_company: company,
                            other_company: &c.name,
                            origin_match: reg,
                            other_match: pattern,
                        });
                    }
                }
                break;
            }
        }
    }
    res
}

/// Tries `match_fast`; if it returns `Ok(None)` — no match, not an error — tries `match_long`.
/// A match or an error from the first pass is returned directly.
pub fn match_company<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos]) -> MatchResult<'a> {
    match match_fast(text, target_companies) {
        Ok(None) => match_long(text, target_companies),
        res => res,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::normalization::deep_normalize_string;
    use std::sync::LazyLock;

    fn target(name: &str, regexs: &[&str], symbols: &[&str], buds: &[&str]) -> TargetCompanyInput {
        TargetCompanyInput {
            name: name.to_string(),
            regexs: regexs.iter().map(|s| s.to_string()).collect(),
            symbols: symbols.iter().map(|s| s.to_string()).collect(),
            buds: buds.iter().map(|s| s.to_string()).collect(),
        }
    }

    mod compile_from_target_companies {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test]
        fn normalizes_the_name_and_keeps_buds_verbatim() {
            let compiled = CompanyMatchInfos::compile_from_target_companies(vec![target(
                "Coca Cola",
                &[],
                &[],
                &["rock bubu"],
            )])
            .unwrap();
            assert_eq!(compiled[0].name, "Coca Cola");
            assert_eq!(compiled[0].n_name, deep_normalize_string("Coca Cola"));
            assert_eq!(compiled[0].buds, vec!["rock bubu".to_string()]);
        }

        #[test_case("bubu", ".*bubu.*"; "unanchored gets wrapped on both ends")]
        #[test_case("^bubu", "bubu.*"; "start anchor character is stripped, end gets wrapped")]
        #[test_case("bubu$", ".*bubu"; "end anchor character is stripped, start gets wrapped")]
        #[test_case("^bubu$", "bubu"; "fully anchored: both anchor characters stripped, no wrapping")]
        fn regexs_get_anchor_aware_wrapping(input: &str, expected_pattern: &str) {
            let compiled =
                CompanyMatchInfos::compile_from_target_companies(vec![target("X", &[input], &[], &[])])
                    .unwrap();
            assert_eq!(compiled[0].regexs[0].pattern, expected_pattern);
        }

        #[test]
        fn symbols_get_word_boundary_wrapping() {
            let compiled =
                CompanyMatchInfos::compile_from_target_companies(vec![target("X", &[], &["COC"], &[])])
                    .unwrap();
            assert_eq!(compiled[0].symbols[0].pattern, r".*\bCOC\b.*");
        }

        #[test]
        fn compiles_multiple_companies_in_order() {
            let compiled = CompanyMatchInfos::compile_from_target_companies(vec![
                target("A", &[], &[], &[]),
                target("B", &[], &[], &[]),
            ])
            .unwrap();
            assert_eq!(compiled.len(), 2);
            assert_eq!(compiled[0].name, "A");
            assert_eq!(compiled[1].name, "B");
        }

        #[test]
        fn empty_input_yields_empty_output() {
            assert!(CompanyMatchInfos::compile_from_target_companies(vec![]).unwrap().is_empty());
        }

        #[test]
        fn an_invalid_regex_pattern_is_rejected_and_named_in_the_error() {
            let err =
                CompanyMatchInfos::compile_from_target_companies(vec![target("X", &["("], &[], &[])])
                    .unwrap_err();
            let message = err.to_string();
            assert!(message.contains('('), "error should mention the offending pattern: {message}");
        }

        #[test]
        fn an_invalid_symbol_pattern_is_rejected() {
            assert!(
                CompanyMatchInfos::compile_from_target_companies(vec![target("X", &[], &["("], &[])])
                    .is_err()
            );
        }
    }

    mod name_shortcut {
        use super::*;
        use test_case::test_case;

        /// The two readings `match_fast` builds, from one piece of text.
        fn names_the_company(text: &str, name: &str) -> bool {
            let normalized = deep_normalize_string(text);
            let split = deep_normalize_string_split_on_punctuation(text);
            text_names_the_company((&normalized, &split), &deep_normalize_string(name))
        }

        #[test_case("SSE", "SSE"; "the whole text")]
        #[test_case("SSE PLC 2031", "SSE"; "at the start")]
        #[test_case("Scottish and Southern SSE", "SSE"; "at the end")]
        #[test_case("Bond SSE 4.75%", "SSE"; "surrounded by spaces")]
        #[test_case("Coca Cola Bottling", "Coca Cola"; "a name of several words")]
        fn accepts_an_occurrence_no_letter_touches(text: &str, name: &str) {
            assert!(names_the_company(text, name), "{name:?} should name a company in {text:?}");
        }

        /// The failure this rule exists for: `Other Assets` normalises to `other assets`, which
        /// contains `sse`, so every row reading "Other Assets" was attributed to SSE.
        #[test_case("Other Assets", "SSE"; "the case that motivated the rule")]
        #[test_case("Assets", "SSE"; "a letter on each side")]
        #[test_case("SSEN Transmission", "SSE"; "a letter after")]
        #[test_case("Classe A", "SSE"; "a letter before")]
        #[test_case("Intesa Sanpaolo", "Sanpaol"; "a prefix of a longer word")]
        fn rejects_an_occurrence_a_letter_touches(text: &str, name: &str) {
            assert!(!names_the_company(text, name), "{name:?} should not name a company in {text:?}");
        }

        /// Digits and the punctuation the normalisation keeps are boundaries, because reports write
        /// names against them.
        #[test_case("3M 2029", "3M"; "a name that starts with a digit")]
        #[test_case("SSE4.75% 2031", "SSE"; "a digit right after")]
        #[test_case("Bond 5SSE", "SSE"; "a digit right before")]
        #[test_case("SSE% of net assets", "SSE"; "a percent sign right after")]
        #[test_case("ENI-SPA", "ENI"; "a dash, which normalisation turns into a space")]
        fn treats_digits_and_kept_punctuation_as_boundaries(text: &str, name: &str) {
            assert!(names_the_company(text, name), "{name:?} should name a company in {text:?}");
        }

        /// The boundary the report printed survives the normalisation erasing it. Without the
        /// second reading every one of these is lost, `AMAZON.COM INC` having become `amazoncom
        /// inc` by the time the check runs.
        #[test_case("AMAZON.COM INC", "Amazon"; "a dot inside the holding name")]
        #[test_case("BOOKING.COM", "Booking"; "a dot at the end of the name")]
        #[test_case("SSE(PLC)", "SSE"; "parentheses")]
        #[test_case("L'OREAL SA", "L'Oreal"; "an apostrophe inside the name itself")]
        #[test_case("VOLKSWAGEN/AUDI", "Volkswagen"; "a slash")]
        fn treats_erased_punctuation_as_a_boundary_too(text: &str, name: &str) {
            assert!(names_the_company(text, name), "{name:?} should name a company in {text:?}");
        }

        /// The second reading widens the boundaries, not the names: it must not let a name through
        /// that no reading of the text delimits.
        #[test_case("Other Assets", "SSE"; "no punctuation to split on")]
        #[test_case("A.SSEMBLY", "SSE"; "split leaves a letter touching")]
        fn the_second_reading_does_not_readmit_an_undelimited_name(text: &str, name: &str) {
            assert!(!names_the_company(text, name), "{name:?} should not name a company in {text:?}");
        }

        /// One occurrence being delimited is enough, wherever the undelimited ones fall.
        #[test]
        fn one_delimited_occurrence_among_several_is_enough() {
            assert!(names_the_company("Classe A SSE PLC", "SSE"));
            assert!(names_the_company("SSE PLC Classe A", "SSE"));
        }

        #[test]
        fn a_name_not_present_at_all_is_not_a_match() {
            assert!(!names_the_company("Other Assets", "Eni"));
        }

        /// `str::contains` says every text contains the empty string; a company whose name
        /// normalised away would then claim every row on the page.
        #[test]
        fn an_empty_name_never_matches() {
            assert!(!contains_delimited("other assets", ""));
            assert!(!contains_delimited("", ""));
        }

        /// The boundary test reads one character on each side, and a non-ASCII letter is one
        /// character however many bytes it occupies. Normalisation folds the Latin accents but
        /// passes other scripts through untouched.
        #[test]
        fn a_non_ascii_letter_is_a_letter_and_not_a_boundary() {
            assert!(!names_the_company("привет", "иве"));
            assert!(names_the_company("привет иве", "иве"));
        }
    }

    mod match_companies {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        static COMPANY_LIST: LazyLock<Vec<CompanyMatchInfos>> = LazyLock::new(|| {
            CompanyMatchInfos::compile_from_target_companies(vec![
                target("Coca Cola", &[], &["COC"], &[]),
                target("bubus", &[r"\bbubu\b", "rock"], &[], &["rock bubu"]),
                target("BlackRock", &[r"\bblack ?rock"], &[], &["black", "rock"]),
                target("pimpa Co.", &[r"\bpimpa co\b", r"\bsecret\b"], &[], &["pimpa"]),
                target("almade", &[r"lman?de\b"], &["ALMD", "ALM"], &["almande"]),
                target("olemande part two", &["part two"], &[], &["part"]),
            ])
            .expect("fixture patterns are all valid onig regexes")
        });

        #[test_case("un ----BLACKROCK----", "BlackRock"; "just name")]
        #[test_case(" Na BUBU la troc", "bubus"; "regex reached through a bud")]
        #[test_case("302840128 ifl COC UUU]]]", "Coca Cola"; "symbol")]
        fn match_company_finds_the_expected_company(provided: &str, expected: &str) {
            let res = match_company(provided, &COMPANY_LIST).unwrap().unwrap();
            assert_eq!(res, expected);
        }

        #[test_case("calimbone"; "unrelated text")]
        #[test_case("One almd 1.2%"; "symbol pattern in the wrong case does not match")]
        fn match_company_finds_nothing(provided: &str) {
            assert!(match_company(provided, &COMPANY_LIST).unwrap().is_none());
        }

        /// Even though the `^` is removed from the pattern string, the match stays anchored to the
        /// start, because matching only ever begins at position 0. It is a property of how the
        /// patterns are used, not a limitation.
        static ANCHORED_COMPANY_LIST: LazyLock<Vec<CompanyMatchInfos>> = LazyLock::new(|| {
            CompanyMatchInfos::compile_from_target_companies(vec![target(
                "Bubu Inc.",
                &["^bubu"],
                &[],
                &["bubu"],
            )])
            .expect("fixture pattern is a valid onig regex")
        });

        #[test]
        fn anchor_stripped_regex_still_matches_only_at_the_correct_position() {
            assert_eq!(
                match_company("bubu", &ANCHORED_COMPANY_LIST).unwrap().unwrap(),
                "Bubu Inc."
            );
        }

        #[test]
        fn anchor_stripped_regex_does_not_match_when_the_content_is_out_of_position() {
            assert!(match_company("xbubu", &ANCHORED_COMPANY_LIST).unwrap().is_none());
        }

        #[test]
        fn match_company_reports_an_ambiguous_regex_match() {
            let expected = MatcherError::AmbiguousRegex {
                text: "Almande part two",
                origin_company: "almade",
                other_company: "olemande part two",
                origin_match: ".*lman?de\\b.*",
                other_match: ".*part two.*",
            };
            assert_eq!(match_company("Almande part two", &COMPANY_LIST).unwrap_err(), expected);
        }

        mod fast {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;

            #[test_case(" The Pimpa Co. 4.75% 2031", "pimpa Co."; "just name")]
            #[test_case("One BLACK ROCK'n ROLL", "BlackRock"; "regex reached through a bud")]
            fn matches(provided: &str, expected: &str) {
                let res = match_fast(provided, &COMPANY_LIST).unwrap().unwrap();
                assert_eq!(res, expected);
            }

            /// The name occurs, but as the head of a longer word, so neither the name shortcut nor
            /// the company's own regexes accept it.
            #[test]
            fn a_name_running_into_a_longer_word_is_not_a_match() {
                assert!(match_fast(" The Pimpa CompanyMatchInfos", &COMPANY_LIST).unwrap().is_none());
            }

            #[test]
            fn does_not_check_symbols_at_all() {
                // "Coca Cola" has only a symbol here, with no bud, regex or name overlapping this
                // text: `match_fast` must not find it, while `match_long` does.
                assert!(match_fast("302840128 ifl COC UUU]]]", &COMPANY_LIST).unwrap().is_none());
            }

            #[test]
            fn no_match_returns_ok_none_not_an_error() {
                assert!(match_fast("calimbone", &COMPANY_LIST).unwrap().is_none());
            }

            #[test]
            fn reports_an_ambiguous_regex_match() {
                let expected = MatcherError::AmbiguousRegex {
                    text: "Almande part two",
                    origin_company: "almade",
                    other_company: "olemande part two",
                    origin_match: ".*lman?de\\b.*",
                    other_match: ".*part two.*",
                };
                assert_eq!(match_fast("Almande part two", &COMPANY_LIST).unwrap_err(), expected);
            }
        }

        mod long {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;

            #[test_case(" Secret company ", "pimpa Co."; "regex, no bud required")]
            #[test_case("One ALMD 1.2%", "almade"; "symbol")]
            fn matches(provided: &str, expected: &str) {
                let res = match_long(provided, &COMPANY_LIST).unwrap().unwrap();
                assert_eq!(res, expected);
            }

            #[test_case("calimbone"; "unrelated text")]
            #[test_case("One almd 1.2%"; "symbol pattern in the wrong case does not match")]
            fn no_match_returns_ok_none_not_an_error(provided: &str) {
                assert!(match_long(provided, &COMPANY_LIST).unwrap().is_none());
            }

            #[test]
            fn reports_an_ambiguous_regex_match() {
                let expected = MatcherError::AmbiguousRegex {
                    text: "Almande part two",
                    origin_company: "almade",
                    other_company: "olemande part two",
                    origin_match: ".*lman?de\\b.*",
                    other_match: ".*part two.*",
                };
                assert_eq!(match_long("Almande part two", &COMPANY_LIST).unwrap_err(), expected);
            }
        }
    }

    /// Checks that `deep_normalize_string` behaves exactly like the normalisation this module used
    /// to carry, over the same table of cases — rather than taking the claim on trust.
    mod normalize_string_equivalence {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("Coca Cola", "coca cola"; "lowercasing")]
        #[test_case(" \thello  i am the\n\n fox\t", "hello i am the fox"; "mixed whitespace collapsed")]
        #[test_case("áàâäéèêëíìîïóòôöúùûü", "aaaaeeeeiiiioooouuuu"; "accented vowels")]
        #[test_case("œæß&ñçåø", "oeaessandncao"; "some unusual characters")]
        #[test_case("ooo,oo-o+oooo–o", "ooo oo o oooo o"; "separating characters become spaces")]
        #[test_case("a!b?c{d}e[f]g(h)i\"j'k’l/m.n", "abcdefghijklmn"; "noise characters are removed")]
        fn matches_the_expected_normalization(input: &str, expected: &str) {
            assert_eq!(deep_normalize_string(input), expected);
        }
    }
}
