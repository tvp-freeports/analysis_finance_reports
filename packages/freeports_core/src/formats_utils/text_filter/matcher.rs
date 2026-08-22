use pyo3::prelude::*;
use std::sync::Arc;
use pyo3::types::{PyString,PyDict,PyList};
use pyo3::exceptions::{PyException};
use onig::{Regex as OnigurmaRegex, Syntax, RegexOptions as OnigurmaRegexOptions};



pub struct PdfBlockTable;



pub fn normalize_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = false;

    for ch in input.chars() {
        let replacement: Option<String> = match ch {
            'é' | 'è' | 'ê' | 'ë' => Some("e".into()),
            'á' | 'à' | 'â' | 'ä' => Some("a".into()),
            'í' | 'ì' | 'î' | 'ï' => Some("i".into()),
            'ó' | 'ò' | 'ô' | 'ö' => Some("o".into()),
            'ú' | 'ù' | 'û' | 'ü' => Some("u".into()),
            'ñ' => Some("n".into()),
            'ç' => Some("c".into()),
            'å' => Some("a".into()),
            'ø' => Some("o".into()),
            'œ' => Some("oe".into()),
            'æ' => Some("ae".into()),
            'ß' => Some("ss".into()),
            '&' => Some("and".into()),

            ',' | '-' | '–' | '+' => Some(" ".into()),

            '!' | '?' | '{' | '}' | '[' | ']' | '(' | ')' |
            '"' | '\'' | '’' | '/' | '.' => None,

            c if c.is_whitespace() => Some(" ".into()),
            c => Some(c.to_lowercase().to_string()),
        };

        if let Some(rep) = replacement {
            if rep == " " {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            } else {
                out.push_str(&rep);
                last_was_space = false;
            }
        }
    }

    out.trim().to_string()
}

#[derive(Debug,Clone)]
struct Regex {
    pattern: String,
    reference: Arc<OnigurmaRegex>
}

/// A pattern that failed to compile as an Oniguruma regex — the pattern itself plus Oniguruma's
/// own error description. Compiling a pattern never touches `Python<'_>`/any PyO3 type, so this
/// stays a native Rust error; only the genuine `#[pymethods]` boundaries that can raise it
/// (`CompanyMatchInfos::compile_from_rows`/`compile_from_pandas_df`) convert it via `From` below,
/// into the same two-arg `(pattern, message)` `PyException` shape this always raised as, so
/// nothing on the Python side of that boundary sees a behavior change.
#[derive(Debug, Clone)]
pub struct PatternCompileError {
    pattern: String,
    message: String,
}

impl std::fmt::Display for PatternCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid pattern `{}`: {}", self.pattern, self.message)
    }
}

impl std::error::Error for PatternCompileError {}

impl From<PatternCompileError> for PyErr {
    fn from(e: PatternCompileError) -> PyErr {
        PyErr::new::<PyException, _>((e.pattern, e.message))
    }
}

/// Anchor-aware wrapping for a "Regexs" pattern: `^foo`/`foo$` keep their anchor and get a `.*`
/// prefix/suffix added on the *other* end only; an unanchored pattern gets wrapped on both ends.
/// Shared by `compile_from_pandas_df` and `compile_from_target_companies` so there's exactly one
/// place this wrapping rule lives.
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
    Ok(Regex {
        reference: Arc::new(
            OnigurmaRegex::with_options(
                modified_pattern.as_str(),
                OnigurmaRegexOptions::REGEX_OPTION_IGNORECASE | OnigurmaRegexOptions::REGEX_OPTION_MULTILINE,
                Syntax::default(),
            )
            .map_err(|e| PatternCompileError { pattern: p.to_string(), message: e.description().to_string() })?,
        ),
        pattern: modified_pattern,
    })
}

/// Word-boundary wrapping for a "Symbols" pattern (a ticker symbol, matched as a whole word).
fn compile_symbol_pattern(p: &str) -> Result<Regex, PatternCompileError> {
    let modified_pattern = format!(r".*\b{p}\b.*");
    Ok(Regex {
        reference: Arc::new(
            OnigurmaRegex::with_options(modified_pattern.as_str(), OnigurmaRegexOptions::REGEX_OPTION_MULTILINE, Syntax::default())
                .map_err(|e| PatternCompileError { pattern: p.to_string(), message: e.description().to_string() })?,
        ),
        pattern: modified_pattern,
    })
}

#[pyclass]
#[derive(Debug,Clone)]
pub struct CompanyMatchInfos{
    name: String,
    n_name: String,
    buds: Vec<String>,
    regexs: Vec<Regex>,
    symbols: Vec<Regex>,
}

/// Plain-Rust input for [`CompanyMatchInfos::compile_from_target_companies`] — the native
/// counterpart of the `{"Regexs": [...], "Symbols": [...], "Buds": [...]}`-shaped dict rows
/// `compile_from_pandas_df` extracts via `df.to_dict(orient="index")`.
///
/// **History (this module used to live in a separate `freeports_lib` crate, merged into
/// `freeports_engine` in Fase E — `agent-memory/rust-native-binary-plan.md`)**: before the merge,
/// `input::companies_db.rs` could not call [`CompanyMatchInfos::compile_from_target_companies`]
/// directly even as a same-workspace Cargo dependency — PyO3 registers a `#[pyclass]` per
/// *compiled extension module*, so a `CompanyMatchInfos` built by code statically linked into
/// `freeports_engine.so` was a **different, incompatible Python type** from the one format-author
/// code got via `import freeports_lib` (the standalone `freeports_lib.so`), confirmed by a real
/// `TypeError: 'CompanyMatchInfos' object cannot be cast as 'CompanyMatchInfos'`. `compile_from_rows`
/// (below) was the fix, called through `py.import("freeports_lib")` so every `CompanyMatchInfos`
/// handed back to Python always came from that one standalone module's own type registration. Now
/// that both modules are the same compiled crate, `companies_db.rs` calls `compile_from_rows` as a
/// plain native function — see that module's doc comment. This function remains the shared,
/// tested implementation both `compile_from_rows` and `compile_from_pandas_df` delegate to.
#[derive(Debug, Clone)]
pub struct TargetCompanyInput {
    pub name: String,
    pub regexs: Vec<String>,
    pub symbols: Vec<String>,
    pub buds: Vec<String>,
}

impl CompanyMatchInfos {
    /// Native (non-PyO3) counterpart of `compile_from_pandas_df` — see the module-identity caveat
    /// on [`TargetCompanyInput`] above for why `companies_db.rs` calls `compile_from_rows`
    /// instead of this directly. Shares the exact same pattern-compilation rules as
    /// `compile_from_pandas_df` (via `compile_regex_pattern`/`compile_symbol_pattern`), so results
    /// are identical either way.
    pub fn compile_from_target_companies(companies: Vec<TargetCompanyInput>) -> Result<Vec<Self>, PatternCompileError> {
        companies
            .into_iter()
            .map(|company| {
                let regexs = company.regexs.iter().map(|p| compile_regex_pattern(p)).collect::<Result<Vec<_>, _>>()?;
                let symbols = company.symbols.iter().map(|p| compile_symbol_pattern(p)).collect::<Result<Vec<_>, _>>()?;
                Ok(CompanyMatchInfos {
                    n_name: normalize_string(&company.name),
                    name: company.name,
                    buds: company.buds,
                    regexs,
                    symbols,
                })
            })
            .collect()
    }
}

/// `(name, regexs, symbols, buds)` per company — the plain-tuple row shape `compile_from_rows`
/// takes, shared with `input::companies_db.rs` (which builds these rows and, since the Fase E
/// merge, calls `compile_from_rows` as a native same-crate function — see that module's doc
/// comment for the cross-module identity history this used to work around via `py.import`).
pub type CompanyRowForCompilation = (String, Vec<String>, Vec<String>, Vec<String>);

#[pymethods]
impl CompanyMatchInfos {
    /// The real PyO3-visible entry point for a caller that already has typed Rust data (like
    /// `input::companies_db.rs`) rather than a pandas-like object. Plain `String`/`Vec<String>`
    /// tuple marshaling — no custom pyclass in the argument, so this is also safe to call as a
    /// plain Python-callable method from code that still needs to cross the Python boundary.
    #[staticmethod]
    pub fn compile_from_rows(rows: Vec<CompanyRowForCompilation>) -> Result<Vec<Self>, PyErr> {
        let companies = rows
            .into_iter()
            .map(|(name, regexs, symbols, buds)| TargetCompanyInput { name, regexs, symbols, buds })
            .collect();
        Ok(Self::compile_from_target_companies(companies)?)
    }

    #[staticmethod]
    pub fn compile_from_pandas_df<'py>(py: Python<'py>, df: Bound<'py,PyAny>) -> Result<Vec<Self>,PyErr>{
        let mut res: Vec<Self> = Vec::new();
        let kwargs=PyDict::new(py);
        kwargs.set_item("orient","index")?;
        let dict = df.call_method("to_dict",(),Some(&kwargs))?.cast_into::<PyDict>()?.iter();
        for (py_name,py_company) in dict {
            let name: String = py_name.extract()?;
            let regexs_patterns: Vec<String> = py_company.get_item("Regexs")?.extract()?;
            let mut regexs: Vec<Regex> = Vec::with_capacity(regexs_patterns.len());
            for p in regexs_patterns.iter() {
                regexs.push(compile_regex_pattern(p)?);
            }
            let symbols_patterns: Vec<String> = py_company.get_item("Symbols")?.extract()?;
            let mut symbols: Vec<Regex> = Vec::with_capacity(symbols_patterns.len());
            for p in symbols_patterns.iter() {
                symbols.push(compile_symbol_pattern(p)?);
            }
            res.push(
                CompanyMatchInfos{
                    n_name: normalize_string(&name),
                    buds: py_company.get_item("Buds")?.extract()?,
                    name,
                    regexs,
                    symbols

                }
            )
        }
        Ok(res)
    } 
}


fn match_fast<'a>(text: &'a str, target_companies: &'a[CompanyMatchInfos]) -> MatchResult<'a> {
    use MatchingErrors::*;
    let txt=normalize_string(text);
    let mut last_matching_regex: Option<(&str,&str)> = None;
    let mut res: Result<Option<&str>,MatchingErrors> = Ok(None);

    for c in target_companies {
        if txt.contains(&c.n_name){
            return Ok(Some(&c.name))
        }
        for b in &c.buds {
            if txt.contains(b) {
                for Regex{
                    pattern,
                    reference: r
                } in &c.regexs {
                    if r.is_match(&txt) {
                        match &last_matching_regex{
                            None => {
                                last_matching_regex=Some((&c.name,pattern));
                                res=Ok(Some(&c.name));
                            },
                            Some((company,reg)) => {
                                return Err(AmbiguousRegex{
                                    text,
                                    origin_company: company,
                                    other_company: &c.name,
                                    origin_match: reg,
                                    other_match: pattern
                                })
                            }
                        }
                        break
                    }
                }
                break
            }
        }
    }
    res
}


type MatchResult<'a> = Result<Option<&'a str>,MatchingErrors<'a>>;


fn match_long<'a>(text: &'a str, target_companies: &'a[CompanyMatchInfos]) -> MatchResult<'a> {
    use MatchingErrors::*;
    let txt=normalize_string(text);
    let mut last_matching_regex: Option<(&str,&str)> = None;
    let mut res: Result<Option<&str>,MatchingErrors> = Ok(None);

    for c in target_companies {
        if c.symbols.iter().any(|s| s.reference.is_match(text)) {
            return Ok(Some(&c.name))
        }
        for Regex{
            pattern,
            reference: r
        } in &c.regexs {
            if r.is_match(&txt) {
                match &last_matching_regex{
                    None => {
                        last_matching_regex=Some((&c.name,pattern));
                        res=Ok(Some(&c.name));
                    },
                    Some((company,reg)) => {
                        return Err(AmbiguousRegex{
                            text,
                            origin_company: company,
                            other_company: &c.name,
                            origin_match: reg,
                            other_match: pattern
                        })
                    }
                }
                break
            }
        }
    }
    res
}

pub fn match_company<'a>(text: &'a str, target_companies: &'a[CompanyMatchInfos]) -> Result<Option<&'a str>,MatchingErrors<'a>> {
    match match_fast(text,target_companies) {
        Ok(None) => match_long(text,target_companies),
        res => res
    }
}



/// Native entry point sharing `py_match_company`'s error conversion (`AmbiguousRegex` →
/// the same Python `Exception` with the same info dict) without needing `Bound<PyString>`/
/// `Bound<PyList>` Python-facing arguments — for callers already inside this crate (e.g.
/// `formats_utils::text_filter::standard_funcs.rs`'s `run_loop`, a per-row hot path that used to
/// reach this via `py.import("freeports._native")...call_method1("match_company", ...)`, re-import
/// and re-extracting `target_companies` on every single row, purely wasted work once the caller
/// is compiled into the same module — Fase E's final simplification pass removed it).
pub fn match_company_or_pyerr(py: Python<'_>, text: &str, target_companies: &[CompanyMatchInfos]) -> PyResult<Option<String>> {
    use MatchingErrors::*;
    match match_company(text, target_companies) {
        Ok(res) => Ok(res.map(str::to_string)),
        Err(AmbiguousRegex { text, origin_company, other_company, origin_match, other_match }) => {
            let info = PyDict::new(py);
            info.set_item(PyString::new(py, "text"), PyString::new(py, text))?;
            info.set_item(PyString::new(py, "origin_company"), PyString::new(py, origin_company))?;
            info.set_item(PyString::new(py, "other_company"), PyString::new(py, other_company))?;
            info.set_item(PyString::new(py, "origin_match"), PyString::new(py, origin_match))?;
            info.set_item(PyString::new(py, "other_match"), PyString::new(py, other_match))?;
            Err(PyErr::new::<PyException, Py<PyDict>>(info.unbind()))
        }
    }
}

#[pyfunction]
#[pyo3(name = "match_company")]
pub fn py_match_company<'py>(py: Python<'py>,text: &Bound<'py, PyString>, target_companies: &Bound<'py,PyList>) -> PyResult<Option<Bound<'py,PyString>>> {
    let text: String = text.extract()?;
    let target_companies: Vec<CompanyMatchInfos> = target_companies.extract()?;
    let result = match_company_or_pyerr(py, &text, &target_companies)?;
    Ok(result.map(|s| PyString::new(py, &s)))
}


#[derive(Debug,Clone)]
pub enum MatchingErrors<'a>{
    AmbiguousRegex{
        text: &'a str,
        origin_company: &'a str,
        other_company: &'a str,
        origin_match: &'a str,
        other_match: &'a str
    }
}


impl PartialEq for MatchingErrors<'_> {
    fn eq(&self,other: &Self) -> bool {
        match (self,other) {
            (Self::AmbiguousRegex{
                text,
                origin_company,
                other_company,
                origin_match,
                other_match
            },Self::AmbiguousRegex{
                text: o_text,
                origin_company: o_origin_company,
                other_company: o_other_company,
                origin_match: o_origin_match,
                other_match: o_other_match                
            }) => {
                (
                    text,
                    origin_company,
                    other_company,
                    origin_match,
                    other_match
                ) == (
                    o_text,
                    o_origin_company,
                    o_other_company,
                    o_origin_match,
                    o_other_match
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use test_case::test_case;
    use std::sync::LazyLock;
    use super::*;
    use pretty_assertions::{assert_eq};
    
    #[test_case("Coca Cola","coca cola"; "lower")]
    #[test_case(" \thello  i am the\n\n fox\t","hello i am the fox"; "withespaces")]
    #[test_case("áàâäéèêëíìîïóòôöúùûü","aaaaeeeeiiiioooouuuu"; "axcents")]
    #[test_case("œæß&ñçåø","oeaessandncao"; "some unusual chars")]
    #[test_case("ooo,oo-o+oooo–o","ooo oo o oooo o"; "separating chars")]
    #[test_case("a!b?c{d}e[f]g(h)i\"j'k’l/m.n","abcdefghijklmn"; "chars to remove")]
    fn string_normalization(provided: &str, expected: &str) {
        assert_eq!(
            normalize_string(provided),
            expected
        )
    }

    mod compile_from_target_companies {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        fn one(name: &str, regexs: Vec<&str>, symbols: Vec<&str>, buds: Vec<&str>) -> TargetCompanyInput {
            TargetCompanyInput {
                name: name.to_string(),
                regexs: regexs.into_iter().map(String::from).collect(),
                symbols: symbols.into_iter().map(String::from).collect(),
                buds: buds.into_iter().map(String::from).collect(),
            }
        }

        #[test]
        fn normalizes_the_name_and_keeps_buds_verbatim() {
            let compiled = CompanyMatchInfos::compile_from_target_companies(vec![one("Coca Cola", vec![], vec![], vec!["rock bubu"])]).unwrap();
            assert_eq!(compiled[0].name, "Coca Cola");
            assert_eq!(compiled[0].n_name, normalize_string("Coca Cola"));
            assert_eq!(compiled[0].buds, vec!["rock bubu".to_string()]);
        }

        #[test_case("bubu", ".*bubu.*"; "unanchored gets wrapped on both ends")]
        #[test_case("^bubu", "bubu.*"; "start-anchored keeps ^, only end gets wrapped")]
        #[test_case("bubu$", ".*bubu"; "end-anchored keeps $, only start gets wrapped")]
        #[test_case("^bubu$", "bubu"; "fully anchored is left untouched (both anchors stripped)")]
        fn regexs_get_anchor_aware_wrapping(input: &str, expected_pattern: &str) {
            let compiled = CompanyMatchInfos::compile_from_target_companies(vec![one("X", vec![input], vec![], vec![])]).unwrap();
            assert_eq!(compiled[0].regexs[0].pattern, expected_pattern);
        }

        #[test]
        fn symbols_get_word_boundary_wrapping() {
            let compiled = CompanyMatchInfos::compile_from_target_companies(vec![one("X", vec![], vec!["COC"], vec![])]).unwrap();
            assert_eq!(compiled[0].symbols[0].pattern, r".*\bCOC\b.*");
        }

        #[test]
        fn compiles_multiple_companies_in_order() {
            let compiled = CompanyMatchInfos::compile_from_target_companies(vec![one("A", vec![], vec![], vec![]), one("B", vec![], vec![], vec![])]).unwrap();
            assert_eq!(compiled.len(), 2);
            assert_eq!(compiled[0].name, "A");
            assert_eq!(compiled[1].name, "B");
        }

        #[test]
        fn empty_input_yields_empty_output() {
            assert!(CompanyMatchInfos::compile_from_target_companies(vec![]).unwrap().is_empty());
        }

        #[test]
        fn an_invalid_regex_pattern_is_rejected() {
            // No `Python::attach` here (this crate's tests don't embed an interpreter) — a
            // `PyErr` can be constructed and matched on without a live GIL, only *displaying* or
            // downcasting it needs one, so this checks the message instead of the exception type.
            let err = CompanyMatchInfos::compile_from_target_companies(vec![one("X", vec!["("], vec![], vec![])]).unwrap_err();
            assert!(format!("{err:?}").contains('('), "error should mention the offending pattern: {err:?}");
        }
    }

    mod match_companies {
        use super::*;
        use pretty_assertions::{assert_eq};
        use test_case::test_case;
        const EMPTY_COMPANY: CompanyMatchInfos = CompanyMatchInfos{
            name: String::new(),
            n_name: String::new(),
            buds: Vec::<String>::new(),
            regexs: Vec::<Regex>::new(),
            symbols: Vec::<Regex>::new(),
        };
        static COMPANY_LIST: LazyLock<Vec<CompanyMatchInfos>> = LazyLock::new(
            || vec![
                CompanyMatchInfos{
                    name: "Coca Cola".to_string(),
                    n_name: normalize_string("Coca Cola"),
                    symbols: vec![
                        Regex{
                            pattern: r".*\bCOC\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bCOC\b.*").unwrap())
                        }
                    ],
                    ..EMPTY_COMPANY
                },
                CompanyMatchInfos{
                    name: "bubus".to_string(),
                    n_name: normalize_string("bubus"),
                    buds: vec![String::from("rock bubu")],
                    regexs: vec![
                        Regex{
                            pattern: r".*\bbubu\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bbubu\b.*").unwrap())
                        },
                        Regex{
                            pattern: r".*rock.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*rock.*").unwrap())
                        }
                    ],
                    ..EMPTY_COMPANY
                },
                CompanyMatchInfos{
                    name: "BlackRock".to_string(),
                    n_name: normalize_string("BlackRock"),
                    buds: vec![String::from("black"),String::from("rock")],
                    regexs: vec![
                        Regex{
                            pattern: r".*\bblack ?rock.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bblack ?rock.*").unwrap())
                        }
                    ],
                    ..EMPTY_COMPANY
                },
                CompanyMatchInfos{
                    name: "pimpa Co.".to_string(),
                    n_name: normalize_string("pimpa Co."),
                    buds: vec![String::from("pimpa")],
                    regexs: vec![
                        Regex{
                            pattern: r".*\bpimpa co\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bpimpa co\b.*").unwrap())
                        },
                        Regex{
                            pattern: r".*\bsecret\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bsecret\b.*").unwrap())
                        }
                    ],
                    symbols: Vec::new()
                },
                CompanyMatchInfos{
                    name: "almade".to_string(),
                    n_name: normalize_string("almade"),
                    buds: vec![String::from("almande")],
                    regexs: vec![
                        Regex{
                            pattern: r".*lman?de\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*lman?de\b.*").unwrap())
                        }
                    ],
                    symbols: vec![
                        Regex{
                            pattern: r".*\bALMD\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bALMD\b.*").unwrap()),
                        },
                        Regex{
                            pattern: r".*\bALM\b.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*\bALM\b.*").unwrap())
                        }
                    ]
                },
                CompanyMatchInfos{
                    name: "olemande part two".to_string(),
                    n_name: normalize_string("olemande part two"),
                    buds: vec![String::from("part")],
                    regexs: vec![
                        Regex{
                            pattern: r".*part two.*".to_string(),
                            reference: Arc::new(OnigurmaRegex::new(r".*part two.*").unwrap())
                        }
                    ],
                    ..EMPTY_COMPANY
                }
            ]
        );
        
        #[test_case("un ----BLACKROCK----","BlackRock";"just name")]
        #[test_case(" Na BUBU la troc","bubus";"regex")]
        #[test_case("302840128 ifl COC UUU]]]","Coca Cola";"symbol")]
        fn matched(provided: &str, expected: &str) {
            let res = match_company(provided,&COMPANY_LIST)
            .unwrap()
            .unwrap();
            assert_eq!(
                res,expected
            )
        }
        #[test_case("calimbone";"company")]
        #[test_case("One almd 1.2%";"lower symbol")]
        fn no_match(provided: &str) {
            let res = match_fast(provided,&COMPANY_LIST).unwrap();
            assert!(res.is_none())
        }

        #[test_case("Almande part two",MatchingErrors::AmbiguousRegex{
            text: "Almande part two",
            origin_company: "almade",
            other_company: "olemande part two",
            origin_match: ".*lman?de\\b.*",
            other_match: ".*part two.*",
        };"ambiguous_regex")]
        fn err(provided: &str, expected: MatchingErrors) {
            let res = match_long(provided,&COMPANY_LIST).unwrap_err();
            assert_eq!(
                res,expected
            )
        }
        
        mod fast {
            use super::*;
            use pretty_assertions::{assert_eq};
            use test_case::test_case;
            #[test_case(" The Pimpa CompanyMatchInfos","pimpa Co.";"just name")]
            #[test_case("One BLACK ROCK'n ROLL","BlackRock";"regex")]
            fn matched(provided: &str, expected: &str) {
                let res = match_fast(provided,&COMPANY_LIST)
                .unwrap()
                .unwrap();
                assert_eq!(
                    res,expected
                )
            }

            #[test]
            fn no_match() {
                let provided=&"calimbone";
                let res = match_fast(provided,&COMPANY_LIST).unwrap();
                assert!(res.is_none())
            }

            #[test_case("Almande part two",MatchingErrors::AmbiguousRegex{
                text: "Almande part two",
                origin_company: "almade",
                other_company: "olemande part two",
                origin_match: ".*lman?de\\b.*",
                other_match: ".*part two.*",
            };"ambiguous_regex")]
            fn err(provided: &str, expected: MatchingErrors) {
                let res = match_fast(provided,&COMPANY_LIST).unwrap_err();
                assert_eq!(
                    res,expected
                )
            }
        }
        mod long {
            use super::*;
            use pretty_assertions::{assert_eq};
            use test_case::test_case;
            #[test_case(" Secret company ","pimpa Co.";"regex")]
            #[test_case("One ALMD 1.2%","almade";"symbol")]
            fn matched(provided: &str, expected: &str) {
                let res = match_long(provided,&COMPANY_LIST)
                .unwrap()
                .unwrap();
                assert_eq!(
                    res,expected
                )
            }

            #[test_case("calimbone";"company")]
            #[test_case("One almd 1.2%";"lower symbol")]
            fn no_match(provided: &str) {
                let res = match_fast(provided,&COMPANY_LIST).unwrap();
                assert!(res.is_none())
            }

            #[test_case("Almande part two",MatchingErrors::AmbiguousRegex{
                text: "Almande part two",
                origin_company: "almade",
                other_company: "olemande part two",
                origin_match: ".*lman?de\\b.*",
                other_match: ".*part two.*",
            };"ambiguous_regex")]
            fn err(provided: &str, expected: MatchingErrors) {
                let res = match_long(provided,&COMPANY_LIST).unwrap_err();
                assert_eq!(
                    res,expected
                )
            }
        }
    }
}