//! [`CompanyMatchInfos`] and [`match_company`]: deciding whether a piece of text names one of the
//! companies being looked for.
//!
//! A holding is written differently in every report — the full legal name, an abbreviation, a
//! ticker, a name split across two table cells — so a company is described by four things at once:
//!
//! - its **name**, matched on the deeply normalised form, so accents, case and punctuation do not matter;
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
//! company's regexes in order. It never looks at symbols. `match_long` drops the bud requirement:
//! it tries every symbol, then every regex of every company. The split matters because the second
//! pass is the expensive one, and on a table of hundreds of rows against hundreds of companies it
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

use crate::core::normalization::deep_normalize_string;

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

/// Tries the normalised name, then, for each bud present in the text, that company's regexes in the
/// order given, stopping at the first that matches. Never looks at symbols.
fn match_fast<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos]) -> MatchResult<'a> {
    let txt = deep_normalize_string(text);
    let mut last_matching_regex: Option<(&str, &str)> = None;
    let mut res: MatchResult<'a> = Ok(None);

    for c in target_companies {
        // Only the **positive** outcome is logged, with the text that produced it. Tracing every
        // comparison meant hundreds of companies against every fragment of text on every page,
        // nearly all of them saying "no", which is not information. The text in the clear is the
        // useful part: it can be found again with a search inside the PDF.
        if txt.contains(&c.n_name) {
            tracing::trace!(coord_ref_1 = %c.name, found = %text, "company matched by its name");
            return Ok(Some(&c.name));
        }
        for b in &c.buds {
            if txt.contains(b) {
                for Regex { pattern, reference: r } in &c.regexs {
                    if r.is_match(&txt) {
                        tracing::trace!(
                            coord_ref_1 = %c.name,
                            pattern,
                            found = %text,
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
            tracing::trace!(coord_ref_1 = %c.name, found = %text, "company matched by one of its symbols");
            return Ok(Some(&c.name));
        }
        for Regex { pattern, reference: r } in &c.regexs {
            if r.is_match(&txt) {
                tracing::trace!(
                    coord_ref_1 = %c.name,
                    pattern,
                    found = %text,
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

            #[test_case(" The Pimpa CompanyMatchInfos", "pimpa Co."; "just name")]
            #[test_case("One BLACK ROCK'n ROLL", "BlackRock"; "regex reached through a bud")]
            fn matches(provided: &str, expected: &str) {
                let res = match_fast(provided, &COMPANY_LIST).unwrap().unwrap();
                assert_eq!(res, expected);
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
