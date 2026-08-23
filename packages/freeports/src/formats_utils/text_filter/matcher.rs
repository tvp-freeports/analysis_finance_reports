//! `CompanyMatchInfos` e `match_company` (matching societario, fuzzy/regex su nomi di aziende).
//!
//! Port pressoche' diretto di `freeports_core/src/formats_utils/text_filter/matcher.rs`, tolto
//! tutto cio' che e' confine PyO3 (`#[pyclass]`/`#[pymethods]`, `compile_from_rows`/
//! `compile_from_pandas_df`, `py_match_company`/`match_company_or_pyerr`) — vedi
//! `agent-memory/M4-implementation-plan.md` §1.
//!
//! **`normalize_string` non e' reimplementata qui**: e' carattere per carattere identica a
//! [`crate::core::normalization::deep_normalize_string`] (gia' portata in M2), quindi `matcher`
//! la riusa direttamente invece di duplicarla — `mod normalize_string_equivalence` sotto verifica
//! esplicitamente l'equivalenza sulla stessa tabella di casi che il vecchio `matcher.rs` usava per
//! la propria `normalize_string`, invece di limitarsi a fidarsi dell'affermazione.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct TargetCompanyInput {
//!     pub name: String,
//!     pub regexs: Vec<String>,
//!     pub symbols: Vec<String>,
//!     pub buds: Vec<String>,
//! }
//!
//! // Struct privata di supporto (stesso nome del riferimento): un pattern compilato piu' la sua
//! // stringa wrappata. Quest'ultima serve sia ai messaggi di errore di ambiguita' sia ai test
//! // sotto, che (come gia' fa commons::date::Date con i propri campi) leggono i campi privati
//! // direttamente invece di passare per accessori pubblici.
//! #[derive(Debug, Clone)]
//! struct Regex { pattern: String, reference: std::sync::Arc<onig::Regex> }
//!
//! #[derive(Debug, Clone)]
//! pub struct CompanyMatchInfos {
//!     name: String,        // nome originale, non normalizzato
//!     n_name: String,      // deep_normalize_string(name)
//!     buds: Vec<String>,   // verbatim, nessun wrapping
//!     regexs: Vec<Regex>,  // wrapping anchor-aware, vedi sotto
//!     symbols: Vec<Regex>, // wrapping word-boundary, vedi sotto
//! }
//!
//! impl CompanyMatchInfos {
//!     pub fn compile_from_target_companies(companies: Vec<TargetCompanyInput>)
//!         -> Result<Vec<Self>, PatternCompileError>;
//! }
//!
//! #[derive(Debug, thiserror::Error)]
//! #[error("invalid pattern `{pattern}`: {message}")]
//! pub struct PatternCompileError { pattern: String, message: String }
//!
//! // Wrapping delle Regexs (anchor-aware, condiviso da ogni pattern in `regexs`) — la stringa
//! // wrappata risultante e' quella pinnata dai test sotto tramite il campo privato `pattern`:
//! //   "bubu"    -> ".*bubu.*"   (non ancorato: wrappato su entrambi i lati)
//! //   "^bubu"   -> "bubu.*"     (l'ancora iniziale viene tolta, non ri-aggiunta: vedi nota sotto)
//! //   "bubu$"   -> ".*bubu"     (l'ancora finale viene tolta, non ri-aggiunta: vedi nota sotto)
//! //   "^bubu$"  -> "bubu"       (entrambe le ancore tolte)
//! //
//! // **Nota verificata sul riferimento** (comportamento da riprodurre, non da "correggere"): il
//! // carattere `^`/`$` viene rimosso dalla stringa del pattern e MAI reinserito nella forma
//! // compilata. Questo e' solo un dettaglio cosmetico della stringa di pattern, non un difetto
//! // funzionale di matching: ogni chiamata a `.is_match()` in questo modulo (mai `.find()`) usa
//! // la semantica *whole-string* di `onig::Regex::is_match` (tenta il match a partire
//! // esclusivamente dalla posizione 0 e richiede che copra l'intera stringa), che da sola
//! // riproduce l'effetto di ancoraggio iniziale/finale — vedi `mod match_companies` sotto per un
//! // test end-to-end che lo pinna tramite `match_company`.
//! //
//! // Wrapping dei Symbols (word-boundary, condiviso da ogni pattern in `symbols`):
//! //   "COC" -> ".*\bCOC\b.*"
//! // I `buds` restano verbatim (nessun wrapping, nessuna compilazione a regex).
//!
//! // match_fast/match_long restano privati, come nel riferimento, ma sono nello stesso modulo dei
//! // test, quindi testabili direttamente (mod fast/mod long sotto).
//! fn match_fast<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos])
//!     -> Result<Option<&'a str>, MatcherError<'a>>;
//! fn match_long<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos])
//!     -> Result<Option<&'a str>, MatcherError<'a>>;
//!
//! pub fn match_company<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos])
//!     -> Result<Option<&'a str>, MatcherError<'a>>;
//!
//! #[derive(Debug, Clone, PartialEq, thiserror::Error)]
//! pub enum MatcherError<'a> {
//!     #[error("ambiguous match for {text:?}: both {origin_company:?} ({origin_match:?}) and {other_company:?} ({other_match:?})")]
//!     AmbiguousRegex {
//!         text: &'a str,
//!         origin_company: &'a str,
//!         other_company: &'a str,
//!         origin_match: &'a str,
//!         other_match: &'a str,
//!     },
//! }
//! ```
//!
//! `match_fast`: se il testo normalizzato contiene il nome normalizzato di un'azienda, matcha
//! subito su quel nome; altrimenti, per ogni `bud` di ogni azienda presente nel testo, prova i
//! `regexs` di quell'azienda **nell'ordine dato**, fermandosi al primo che matcha. Se due aziende
//! diverse producono entrambe un match via regex, e' `MatcherError::AmbiguousRegex`. `match_fast`
//! non guarda mai `symbols`.
//!
//! `match_long`: se uno dei `symbols` di un'azienda matcha il testo (non normalizzato), matcha
//! subito su quell'azienda; altrimenti prova **tutti** i `regexs` di **tutte** le aziende (senza
//! richiedere un bud presente), stesso criterio di ambiguita' di `match_fast`.
//!
//! `match_company`: prova `match_fast`; se restituisce `Ok(None)` (nessun match, non errore),
//! prova `match_long`; altrimenti (match o errore) restituisce direttamente il risultato di
//! `match_fast`.

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

/// Wrapping anchor-aware di una pattern "Regexs", condiviso da ogni compilazione: `^foo`/`foo$`
/// perdono il carattere di ancora (rimosso, mai reinserito — vedi la nota nel doc-comment del
/// modulo) e ricevono un prefisso/suffisso `.*` solo sul lato *opposto*; una pattern non ancorata
/// viene wrappata su entrambi i lati.
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

/// Wrapping word-boundary di una pattern "Symbols" (un ticker, matchato come parola intera).
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
    /// Il nome originale della società, non normalizzato.
    ///
    /// Accessore aggiunto in M7 (modifica puramente additiva a codice M4): il confine Python di
    /// `formats_repo::unstructured` deve poter passare le società bersaglio a un pipe `text_filter`
    /// d'autore, e senza questo non c'è modo di leggerle da fuori del modulo.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// La forma profondamente normalizzata del nome, quella su cui si fanno i confronti.
    pub fn normalized_name(&self) -> &str {
        &self.n_name
    }

    pub fn compile_from_target_companies(
        companies: Vec<TargetCompanyInput>,
    ) -> Result<Vec<Self>, PatternCompileError> {
        companies
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
            .collect()
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

/// Se il testo normalizzato contiene il nome normalizzato di un'azienda, matcha subito su quel
/// nome; altrimenti, per ogni `bud` di ogni azienda presente nel testo, prova i `regexs` di
/// quell'azienda nell'ordine dato, fermandosi al primo che matcha. Non guarda mai `symbols`.
fn match_fast<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos]) -> MatchResult<'a> {
    let txt = deep_normalize_string(text);
    let mut last_matching_regex: Option<(&str, &str)> = None;
    let mut res: MatchResult<'a> = Ok(None);

    for c in target_companies {
        if txt.contains(&c.n_name) {
            return Ok(Some(&c.name));
        }
        for b in &c.buds {
            if txt.contains(b) {
                for Regex { pattern, reference: r } in &c.regexs {
                    if r.is_match(&txt) {
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

/// Se uno dei `symbols` di un'azienda matcha il testo (non normalizzato), matcha subito su
/// quell'azienda; altrimenti prova tutti i `regexs` di tutte le aziende (senza richiedere un bud
/// presente), stesso criterio di ambiguita' di [`match_fast`].
fn match_long<'a>(text: &'a str, target_companies: &'a [CompanyMatchInfos]) -> MatchResult<'a> {
    let txt = deep_normalize_string(text);
    let mut last_matching_regex: Option<(&str, &str)> = None;
    let mut res: MatchResult<'a> = Ok(None);

    for c in target_companies {
        if c.symbols.iter().any(|s| s.reference.is_match(text)) {
            return Ok(Some(&c.name));
        }
        for Regex { pattern, reference: r } in &c.regexs {
            if r.is_match(&txt) {
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

/// Prova [`match_fast`]; se restituisce `Ok(None)` (nessun match, non errore), prova
/// [`match_long`]; altrimenti (match o errore) restituisce direttamente il risultato di
/// `match_fast`.
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

        /// Pinna il comportamento discusso nel doc-comment del modulo: anche se il carattere `^`
        /// viene tolto dalla stringa del pattern (`"^bubu"` -> `"bubu.*"`, mai `"^bubu.*"`), il
        /// match resta effettivamente ancorato all'inizio perche' ogni chiamata a `.is_match()`
        /// tenta il match solo a partire dalla posizione 0 -- non e' un limite del matcher.
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
                // "Coca Cola" only has a symbol ("COC"), no bud/regex/name overlap with this
                // text -- match_fast must not find it (match_long does, see below).
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

    /// Verifica che `deep_normalize_string` (M2) si comporti esattamente come la vecchia
    /// `normalize_string` di `matcher.rs` sulla stessa tabella di casi — vedi il doc-comment del
    /// modulo per il perche' `matcher.rs` non ha una propria `normalize_string`.
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
