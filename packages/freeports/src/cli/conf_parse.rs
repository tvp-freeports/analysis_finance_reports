//! Parsing dei valori di configurazione (report, url, path, rinomine): la grammatica
//! `<url>:<path>:<name>` di `targets/conf_parse.md`.
//!
//! `M9-implementation-plan.md` §1/§3 passo 3, §0 Q3/Q4. Porta la grammatica di
//! `DocumentSpec.from_str` (`packages/freeports_core/python/freeports/_internals/cli/
//! conf_parse.py`), **non** l'intera classe pydantic: un `parse` puro più una validazione
//! separata ("almeno url o path specificato"), chiamata da `cli::freeports_config`, non
//! incorporata in `parse` stesso.
//!
//! # Grammatica (identica al riferimento, tracciata riga per riga — vedi
//! # `M9-implementation-plan.md` per la derivazione completa)
//!
//! - Lo schema url (`http://`/`https://`) è rilevato in testa alla stringa (dopo aver tolto un
//!   eventuale `"` iniziale, se presente, per permettere di quotare l'intero specificatore).
//! - Il resto è spezzato in segmenti da `:` **non fra virgolette**; un segmento può essere
//!   racchiuso fra `"..."` per proteggere `:` al suo interno (es. porta di un url, path Windows,
//!   nome con `:`).
//! - **1 segmento**: solo path/nome (nessuno schema rilevato) → `path` = valore assoluto rispetto
//!   alla cwd, `name` = la sua rappresentazione testuale, `url` = `None`. Con schema rilevato →
//!   `url` = schema + segmento, `name` = url, `path` = `None`.
//! - **2 segmenti, senza `:` finale**: `<url>:<name>` (con schema) o `<path>:<name>` (senza).
//! - **2 segmenti, con `:` finale** (`<url>:<path>:`): richiede uno schema rilevato — `url` =
//!   schema + segmento 1, `path` = segmento 2 (assoluto), `name` = `url` (fallback). **Senza**
//!   schema rilevato è [`DocumentSpecError::MissingUrlScheme`] (§0 Q4: il riferimento Python
//!   concatenerebbe `None + str`, un `TypeError` mai catturato — qui un errore tipizzato, mai un
//!   panic).
//! - **3 segmenti** (`<url>:<path>:<name>`): richiede sempre uno schema rilevato per lo stesso
//!   motivo — senza, [`DocumentSpecError::MissingUrlScheme`] (stessa classe di problema di sopra,
//!   non esplicitamente nominata da §0 Q4 ma stessa causa: concatenare uno schema assente).
//! - **0, o 4+ segmenti**: [`DocumentSpecError::InvalidSegmentCount`] (il riferimento solleva
//!   `ValueError` nel proprio ramo `else` finale).
//! - **Stringa vuota** (dopo `.strip()`): tutti e tre i campi `None` — non è un errore di `parse`,
//!   lo diventa solo passando per `input_should_be_specified` (vedi sotto).
//!
//! **Nota per l'implementazione**: nessun `unwrap`/`expect`/indicizzazione che possa panicare su
//! un input arbitrario — verificato dal test di stress sotto (100+ stringhe generate).
//!
use std::path::PathBuf;

/// Separatore condiviso fra `cli::batch` (righe CSV) e `config_locations::env`
/// (`FREEPORTS_REPORTS`, `M9-implementation-plan.md` §0 Q3) -- un'unica costante, non due
/// letterali `'|'` duplicati che potrebbero divergere.
pub const DOC_SPEC_SEPARATOR: char = '|';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSpec {
    pub url: Option<String>,
    pub path: Option<PathBuf>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentSpecError {
    /// 2 o 3 segmenti che richiedono uno schema `http(s)://` rilevato, ma nessuno schema è
    /// stato trovato in testa allo specificatore (§0 Q4).
    #[error("document specifier {specifier:?} has {segment_count} segments but no http(s):// scheme was found")]
    MissingUrlScheme { specifier: String, segment_count: usize },
    /// 0, oppure 4 o più segmenti -- nessuna forma della grammatica li accetta.
    #[error("document specifier {specifier:?} has {segment_count} segments, expected 1 to 3")]
    InvalidSegmentCount { specifier: String, segment_count: usize },
    /// Da `input_should_be_specified`: né `url` né `path` sono specificati.
    #[error("you have to specify at least one of: the url, the pdf file path, or both")]
    InputNotSpecified,
}

const URL_SCHEMES: [&str; 2] = ["http://", "https://"];

/// Risolve un segmento non-url a path assoluto rispetto alla cwd (`os.path.abspath` del
/// riferimento). Se la cwd non è leggibile (limite di sistema, non un errore di parsing) il
/// segmento resta relativo piuttosto che far fallire l'intero parsing per una causa estranea
/// alla grammatica.
fn abspath(segment: &str) -> PathBuf {
    let path = PathBuf::from(segment);
    if path.is_absolute() { path } else { std::env::current_dir().map(|cwd| cwd.join(&path)).unwrap_or(path) }
}

/// Spezza `body` (già privato dello schema url, se presente) in segmenti separati da `:` non
/// fra virgolette, spogliando le virgolette che racchiudono un intero segmento. Ritorna anche
/// se l'ultimo carattere consumato è stato un `:` non fra virgolette (usato dal chiamante per
/// distinguere `<url>:<path>:` da `<url>:<name>` nel caso a 2 segmenti) -- porting diretto di
/// `DocumentSpec.from_str` (`conf_parse.py`), incluso il trattamento delle virgolette non in
/// testa a un segmento (restano caratteri letterali, non aprono/chiudono la zona quotata).
fn split_segments(body: &str) -> (Vec<String>, bool) {
    let mut segments: Vec<String> = Vec::new();
    let mut quote_area = false;
    let mut begin_segment = true;
    for ch in body.chars() {
        if ch == '"' && !quote_area && begin_segment {
            quote_area = true;
        } else if ch == '"' && quote_area {
            quote_area = false;
        } else {
            if begin_segment {
                segments.push(String::new());
                begin_segment = false;
            }
            if ch == ':' && !quote_area {
                begin_segment = true;
            } else if let Some(last) = segments.last_mut() {
                last.push(ch);
            }
        }
    }
    (segments, begin_segment)
}

impl DocumentSpec {
    /// Non panica mai, qualunque sia l'input (vedi `tests::stress`).
    pub fn parse(specifier: &str) -> Result<DocumentSpec, DocumentSpecError> {
        let trimmed = specifier.trim();
        if trimmed.is_empty() {
            return Ok(DocumentSpec { url: None, path: None, name: None });
        }

        let first_escaped = trimmed.starts_with('"');
        let mut rest = if first_escaped { &trimmed[1..] } else { trimmed };

        let mut url_scheme: Option<&str> = None;
        for scheme in URL_SCHEMES {
            if rest.starts_with(scheme) {
                url_scheme = Some(scheme);
                rest = &rest[scheme.len()..];
                break;
            }
        }

        let reconstructed;
        let body: &str = if first_escaped {
            reconstructed = format!("\"{rest}");
            &reconstructed
        } else {
            rest
        };

        let (segments, trailing_colon) = split_segments(body);

        match segments.len() {
            1 => match url_scheme {
                None => {
                    let path = abspath(&segments[0]);
                    let name = path.to_string_lossy().into_owned();
                    Ok(DocumentSpec { url: None, path: Some(path), name: Some(name) })
                }
                Some(scheme) => {
                    let url = format!("{scheme}{}", segments[0]);
                    Ok(DocumentSpec { url: Some(url.clone()), path: None, name: Some(url) })
                }
            },
            2 if trailing_colon => match url_scheme {
                None => Err(DocumentSpecError::MissingUrlScheme { specifier: specifier.to_string(), segment_count: 2 }),
                Some(scheme) => {
                    let url = format!("{scheme}{}", segments[0]);
                    let path = abspath(&segments[1]);
                    Ok(DocumentSpec { url: Some(url.clone()), path: Some(path), name: Some(url) })
                }
            },
            2 => {
                let name = segments[1].clone();
                match url_scheme {
                    None => {
                        let path = abspath(&segments[0]);
                        Ok(DocumentSpec { url: None, path: Some(path), name: Some(name) })
                    }
                    Some(scheme) => {
                        let url = format!("{scheme}{}", segments[0]);
                        Ok(DocumentSpec { url: Some(url), path: None, name: Some(name) })
                    }
                }
            }
            3 => match url_scheme {
                None => Err(DocumentSpecError::MissingUrlScheme { specifier: specifier.to_string(), segment_count: 3 }),
                Some(scheme) => {
                    let url = format!("{scheme}{}", segments[0]);
                    let path = abspath(&segments[1]);
                    let name = segments[2].clone();
                    Ok(DocumentSpec { url: Some(url), path: Some(path), name: Some(name) })
                }
            },
            segment_count => {
                Err(DocumentSpecError::InvalidSegmentCount { specifier: specifier.to_string(), segment_count })
            }
        }
    }

    /// Validazione separata, non incorporata in `parse` (a differenza del riferimento, dove è
    /// un `model_validator`): un `DocumentSpec` con tutti e tre i campi `None` (es. dalla
    /// stringa vuota) è un `parse` valido ma un documento non specificato.
    pub fn input_should_be_specified(&self) -> Result<(), DocumentSpecError> {
        if self.url.is_none() && self.path.is_none() { Err(DocumentSpecError::InputNotSpecified) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cwd_join(name: &str) -> PathBuf {
        std::env::current_dir().expect("cwd must be readable in a test process").join(name)
    }

    fn display(p: &PathBuf) -> String {
        p.to_string_lossy().into_owned()
    }

    mod single_segment {
        use super::*;

        #[test]
        fn only_a_name_with_no_url_scheme_becomes_an_absolute_path_and_its_own_name() {
            let spec = DocumentSpec::parse("report.pdf").unwrap();
            let expected_path = cwd_join("report.pdf");
            assert_eq!(spec.path, Some(expected_path.clone()));
            assert_eq!(spec.name, Some(display(&expected_path)));
            assert_eq!(spec.url, None);
        }

        #[test]
        fn only_a_url_becomes_url_and_its_own_name() {
            let spec = DocumentSpec::parse("https://example.com/report.pdf").unwrap();
            assert_eq!(spec.url, Some("https://example.com/report.pdf".to_string()));
            assert_eq!(spec.name, Some("https://example.com/report.pdf".to_string()));
            assert_eq!(spec.path, None);
        }

        #[test]
        fn http_scheme_is_recognized_alongside_https() {
            let spec = DocumentSpec::parse("http://example.com/report.pdf").unwrap();
            assert_eq!(spec.url, Some("http://example.com/report.pdf".to_string()));
        }
    }

    mod two_segments_without_trailing_colon {
        use super::*;

        #[test]
        fn url_and_name_no_path() {
            let spec = DocumentSpec::parse("https://example.com/x:myname").unwrap();
            assert_eq!(spec.url, Some("https://example.com/x".to_string()));
            assert_eq!(spec.name, Some("myname".to_string()));
            assert_eq!(spec.path, None);
        }

        #[test]
        fn path_and_name_no_url() {
            let spec = DocumentSpec::parse("report.pdf:myname").unwrap();
            assert_eq!(spec.path, Some(cwd_join("report.pdf")));
            assert_eq!(spec.name, Some("myname".to_string()));
            assert_eq!(spec.url, None);
        }
    }

    mod two_segments_with_trailing_colon {
        use super::*;

        #[test]
        fn url_and_path_name_falls_back_to_url() {
            let spec = DocumentSpec::parse("https://example.com/x:report.pdf:").unwrap();
            assert_eq!(spec.url, Some("https://example.com/x".to_string()));
            assert_eq!(spec.path, Some(cwd_join("report.pdf")));
            assert_eq!(spec.name, Some("https://example.com/x".to_string()), "name must fall back to the url");
        }

        #[test]
        fn without_a_url_scheme_it_is_a_typed_error_not_a_panic() {
            // §0 Q4: the reference would concatenate `None + str` here (uncaught `TypeError`).
            let result = std::panic::catch_unwind(|| DocumentSpec::parse("report.pdf:other:"));
            assert!(result.is_ok(), "must not panic");
            match result.unwrap() {
                Err(DocumentSpecError::MissingUrlScheme { segment_count, .. }) => assert_eq!(segment_count, 2),
                other => panic!("expected MissingUrlScheme, got {other:?}"),
            }
        }
    }

    mod three_segments {
        use super::*;

        #[test]
        fn url_path_and_name_all_specified() {
            let spec = DocumentSpec::parse("https://example.com/x:report.pdf:myname").unwrap();
            assert_eq!(spec.url, Some("https://example.com/x".to_string()));
            assert_eq!(spec.path, Some(cwd_join("report.pdf")));
            assert_eq!(spec.name, Some("myname".to_string()));
        }

        #[test]
        fn without_a_url_scheme_it_is_a_typed_error_not_a_panic() {
            // Same class of bug as the 2-segment trailing-colon case (§0 Q4): the reference would
            // also concatenate `None + str` here. Not explicitly named by Q4, but the identical
            // failure mode -- treated the same way, flagged in the test-writer's report.
            let result = std::panic::catch_unwind(|| DocumentSpec::parse("a:b:c"));
            assert!(result.is_ok(), "must not panic");
            match result.unwrap() {
                Err(DocumentSpecError::MissingUrlScheme { segment_count, .. }) => assert_eq!(segment_count, 3),
                other => panic!("expected MissingUrlScheme, got {other:?}"),
            }
        }
    }

    mod quoting {
        use super::*;

        #[test]
        fn a_colon_inside_a_quoted_url_segment_is_preserved_a_port_number() {
            let spec = DocumentSpec::parse(r#""https://example.com:8080/x""#).unwrap();
            assert_eq!(spec.url, Some("https://example.com:8080/x".to_string()));
            assert_eq!(spec.path, None);
        }

        #[test]
        fn an_unquoted_colon_after_the_url_scheme_is_misparsed_as_a_segment_delimiter() {
            // Documented, not a bug to fix: `targets/conf_parse.md` explicitly requires quoting
            // whenever the url/path/name itself contains a `:` (e.g. a port number).
            let spec = DocumentSpec::parse("https://example.com:8080/x").unwrap();
            assert_eq!(spec.url, Some("https://example.com".to_string()));
            assert_eq!(spec.name, Some("8080/x".to_string()));
        }

        #[test]
        fn a_colon_inside_a_quoted_path_segment_is_preserved() {
            let spec = DocumentSpec::parse(r#""weird:file.pdf":myname"#).unwrap();
            assert_eq!(spec.path, Some(cwd_join("weird:file.pdf")));
            assert_eq!(spec.name, Some("myname".to_string()));
            assert_eq!(spec.url, None);
        }

        #[test]
        fn a_colon_inside_a_quoted_name_segment_is_preserved() {
            let spec = DocumentSpec::parse(r#"https://example.com/x:report.pdf:"my:name""#).unwrap();
            assert_eq!(spec.url, Some("https://example.com/x".to_string()));
            assert_eq!(spec.path, Some(cwd_join("report.pdf")));
            assert_eq!(spec.name, Some("my:name".to_string()));
        }

        #[test]
        fn all_three_segments_quoted_together() {
            let spec = DocumentSpec::parse(r#""https://example.com:8080/x":"weird:file.pdf":"my:name""#).unwrap();
            assert_eq!(spec.url, Some("https://example.com:8080/x".to_string()));
            assert_eq!(spec.path, Some(cwd_join("weird:file.pdf")));
            assert_eq!(spec.name, Some("my:name".to_string()));
        }

        #[test]
        fn an_unclosed_quote_does_not_panic_and_still_produces_a_result() {
            let result = std::panic::catch_unwind(|| DocumentSpec::parse(r#""unclosed"#));
            assert!(result.is_ok());
            assert!(result.unwrap().is_ok());
        }
    }

    mod empty_and_whitespace {
        use super::*;

        #[test]
        fn an_empty_string_parses_to_all_none() {
            let spec = DocumentSpec::parse("").unwrap();
            assert_eq!(spec, DocumentSpec { url: None, path: None, name: None });
        }

        #[test]
        fn a_whitespace_only_string_is_stripped_to_the_same_all_none_result() {
            let spec = DocumentSpec::parse("   ").unwrap();
            assert_eq!(spec, DocumentSpec { url: None, path: None, name: None });
        }

        #[test]
        fn quote_characters_alone_produce_zero_segments_a_typed_error() {
            let result = std::panic::catch_unwind(|| DocumentSpec::parse("\"\""));
            assert!(result.is_ok(), "must not panic");
            match result.unwrap() {
                Err(DocumentSpecError::InvalidSegmentCount { segment_count, .. }) => assert_eq!(segment_count, 0),
                other => panic!("expected InvalidSegmentCount(0), got {other:?}"),
            }
        }
    }

    mod invalid_segment_count {
        use super::*;

        #[test]
        fn four_segments_is_a_typed_error_not_a_panic() {
            let result = std::panic::catch_unwind(|| DocumentSpec::parse("a:b:c:d"));
            assert!(result.is_ok(), "must not panic");
            match result.unwrap() {
                Err(DocumentSpecError::InvalidSegmentCount { segment_count, .. }) => assert_eq!(segment_count, 4),
                other => panic!("expected InvalidSegmentCount(4), got {other:?}"),
            }
        }

        #[test]
        fn many_segments_is_still_a_typed_error_not_a_panic() {
            let specifier = "a:b:c:d:e:f:g:h";
            let result = std::panic::catch_unwind(|| DocumentSpec::parse(specifier));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(DocumentSpecError::InvalidSegmentCount { .. })));
        }
    }

    mod input_should_be_specified {
        use super::*;

        #[test]
        fn neither_url_nor_path_is_an_error() {
            let spec = DocumentSpec { url: None, path: None, name: None };
            assert!(matches!(spec.input_should_be_specified(), Err(DocumentSpecError::InputNotSpecified)));
        }

        #[test]
        fn neither_url_nor_path_but_a_name_is_still_an_error() {
            // `name` alone never satisfies the requirement -- only `url`/`path` count.
            let spec = DocumentSpec { url: None, path: None, name: Some("whatever".to_string()) };
            assert!(matches!(spec.input_should_be_specified(), Err(DocumentSpecError::InputNotSpecified)));
        }

        #[test]
        fn only_url_is_fine() {
            let spec = DocumentSpec { url: Some("https://example.com".to_string()), path: None, name: None };
            assert!(spec.input_should_be_specified().is_ok());
        }

        #[test]
        fn only_path_is_fine() {
            let spec = DocumentSpec { url: None, path: Some(PathBuf::from("/tmp/x.pdf")), name: None };
            assert!(spec.input_should_be_specified().is_ok());
        }

        #[test]
        fn both_url_and_path_is_fine() {
            let spec =
                DocumentSpec { url: Some("https://example.com".to_string()), path: Some(PathBuf::from("/tmp/x.pdf")), name: None };
            assert!(spec.input_should_be_specified().is_ok());
        }
    }

    mod doc_spec_separator {
        #[test]
        fn is_the_pipe_character() {
            assert_eq!(super::DOC_SPEC_SEPARATOR, '|');
        }

        // The claim that `cli::batch` and `config_locations::env` share this exact constant
        // (rather than two independent `'|'` literals that could drift) is verified at the two
        // real call sites themselves: both `cli::batch`'s and `cli::config_locations::env`'s test
        // modules import `crate::cli::conf_parse::DOC_SPEC_SEPARATOR` directly and split on it,
        // so a change to this constant alone would move both, and duplicating the assertion here
        // would only re-check the literal, not the sharing.
    }

    mod stress {
        use super::*;

        /// 100+ combinatorially generated specifiers -- with/without a url scheme, 0-3 unquoted
        /// or quoted segments, internal colons -- verifying the one invariant that matters for
        /// arbitrary CLI/env/file input: `parse` never panics, only ever returns `Ok`/`Err`.
        #[test]
        fn parsing_never_panics_on_arbitrary_combinations() {
            let schemes = ["", "http://", "https://"];
            let pieces = ["a", "a:b", "\"a:b\"", "", "\"\"", "weird name", "1.2.3"];
            let joiners = ["", ":", "::", ":::"];

            let mut generated = 0usize;
            for scheme in schemes {
                for p1 in pieces {
                    for p2 in pieces {
                        for joiner in joiners {
                            let specifier = format!("{scheme}{p1}{joiner}{p2}");
                            let result = std::panic::catch_unwind(|| DocumentSpec::parse(&specifier));
                            assert!(result.is_ok(), "parse panicked on {specifier:?}");
                            generated += 1;
                        }
                    }
                }
            }
            assert!(generated >= 100, "expected at least 100 generated cases, got {generated}");
        }

        #[test]
        fn parsing_never_panics_on_three_segment_combinations_with_quoting_variety() {
            let bodies = ["plain", "with:colon", "\"quoted:colon\"", "\"\"", ""];
            let mut generated = 0usize;
            for scheme in ["", "http://", "https://"] {
                for a in bodies {
                    for b in bodies {
                        for c in bodies {
                            let specifier = format!("{scheme}{a}:{b}:{c}");
                            let result = std::panic::catch_unwind(|| DocumentSpec::parse(&specifier));
                            assert!(result.is_ok(), "parse panicked on {specifier:?}");
                            generated += 1;
                        }
                    }
                }
            }
            assert!(generated >= 100, "expected at least 100 generated cases, got {generated}");
        }
    }
}
