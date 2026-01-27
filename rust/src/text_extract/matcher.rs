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

#[pyclass]
#[derive(Debug,Clone)]
pub struct CompanyMatchInfos{
    name: String,
    n_name: String,
    buds: Vec<String>,
    regexs: Vec<Regex>,
    symbols: Vec<Regex>,
}
#[pymethods]
impl CompanyMatchInfos {
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
            for p in regexs_patterns.into_iter() {
                let mut modified_pattern: String;
                if p.starts_with('^') {
                    modified_pattern=p.clone();
                    modified_pattern.remove(0);
                } else if p.ends_with('$'){
                    modified_pattern=p.clone();
                    modified_pattern.pop();
                } else {
                    modified_pattern=format!(".*{}.*",p)
                }
                regexs.push(
                    Regex{
                        reference: Arc::new(
                            OnigurmaRegex::with_options(
                                modified_pattern.as_str(),
                                OnigurmaRegexOptions::REGEX_OPTION_IGNORECASE | OnigurmaRegexOptions::REGEX_OPTION_MULTILINE,
                                Syntax::default()
                            ).map_err(|e|
                                PyErr::new::<PyException, _>((p.clone(),e.description().to_string()))
                            )?
                        ),
                        pattern: modified_pattern
                    }
                )
            }
            let symbols_patterns: Vec<String> = py_company.get_item("Symbols")?.extract()?;
            let mut symbols: Vec<Regex> = Vec::with_capacity(symbols_patterns.len());
            for p in symbols_patterns.into_iter() {
                let modified_pattern = format!(r".*\b{}\b.*",p);
                symbols.push(
                    Regex{
                        reference: Arc::new(
                            OnigurmaRegex::with_options(
                                modified_pattern.as_str(),
                                OnigurmaRegexOptions::REGEX_OPTION_MULTILINE,
                                Syntax::default()
                            ).map_err(|e|
                                PyErr::new::<PyException, _>((p.clone(),e.description().to_string()))
                            )?
                        ),
                        pattern: modified_pattern
                    }
                )
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



#[pyfunction]
#[pyo3(name = "match_company")]
pub fn py_match_company<'py>(py: Python<'py>,text: &Bound<'py, PyString>, target_companies: &Bound<'py,PyList>) -> PyResult<Option<Bound<'py,PyString>>> {
    use MatchingErrors::*;
    let text: String = text.extract()?;
    let target_companies: Vec<CompanyMatchInfos> = target_companies.extract()?;
    match match_company(&text,&target_companies) {
        Ok(Some(res)) => Ok(Some(PyString::new(py,res))),
        Ok(None) => Ok(None),
        Err(AmbiguousRegex{
            text,
            origin_company,
            other_company,
            origin_match,
            other_match,
        }) => {
            let info=PyDict::new(py);
            info.set_item(PyString::new(py,"text"),PyString::new(py,text))?;
            info.set_item(PyString::new(py,"origin_company"),PyString::new(py,origin_company))?;
            info.set_item(PyString::new(py,"other_company"),PyString::new(py,other_company))?;
            info.set_item(PyString::new(py,"origin_match"),PyString::new(py,origin_match))?;
            info.set_item(PyString::new(py,"other_match"),PyString::new(py,other_match))?;
            Err(PyErr::new::<PyException, Py<PyDict>>(info.unbind()))
        }
    }
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