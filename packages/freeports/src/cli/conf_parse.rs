//! Parsing a document specifier: the `<url>:<path>:<name>` grammar.
//!
//! One string names where a report comes from, where it goes on disk, and what to call it — any of
//! the three being optional.
//!
//! # The grammar
//!
//! - a URL scheme (`http://`, `https://`) is detected at the head of the string, after stripping a leading quote if there is one, so that a whole specifier may be quoted;
//! - the rest is split into segments on `:` **outside quotes**; a segment may be wrapped in quotes to protect a `:` inside it — a URL port, a Windows path, a name containing a colon;
//! - **one segment**: with no scheme, a path (made absolute against the working directory) whose text is also the name; with a scheme, a URL that is also the name;
//! - **two segments**: `<url>:<name>` or `<path>:<name>`, depending on whether a scheme was found;
//! - **two segments and a trailing colon** (`<url>:<path>:`): requires a scheme;
//! - **three segments** (`<url>:<path>:<name>`): requires a scheme.
//!
//! Requiring the scheme in the last two cases is what turns "no URL to prepend" into a typed error
//! instead of a value built from nothing.
//!
//! **Zero, or four or more segments** is an error, and an **empty string** parses into a spec with
//! no fields set — not an error here, only when validated as an actual input.
//!
//! Parsing never panics on any input, whatever its shape, which the stress test below exercises.
use std::path::PathBuf;
use crate::core::tracing_setup::log_error;

/// The separator between several document specifiers, shared by batch rows and the environment: one
/// constant rather than two literals that could drift apart.
pub const DOC_SPEC_SEPARATOR: char = '|';

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocumentSpec {
    pub url: Option<String>,
    pub path: Option<PathBuf>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentSpecError {
    /// A form requiring a URL scheme was used, but no scheme was found at the head of the
    /// specifier.
    #[error("document specifier {specifier:?} has {segment_count} segments but no http(s):// scheme was found")]
    MissingUrlScheme { specifier: String, segment_count: usize },
    /// Zero, or four or more segments: no form of the grammar accepts them.
    #[error("document specifier {specifier:?} has {segment_count} segments, expected 1 to 3")]
    InvalidSegmentCount { specifier: String, segment_count: usize },
    /// Da `input_should_be_specified`: né `url` né `path` sono specificati.
    #[error("you have to specify at least one of: the url, the pdf file path, or both")]
    InputNotSpecified,
}

const URL_SCHEMES: [&str; 2] = ["http://", "https://"];

/// Resolves a non-URL segment to an absolute path against the working directory.
///
/// If the working directory cannot be read — a system limit, not a parsing error — the segment
/// stays relative rather than failing the whole parse for a reason unrelated to the grammar.
fn abspath(segment: &str) -> PathBuf {
    let path = PathBuf::from(segment);
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or_else(|e| {
            tracing::warn!(error = log_error(&e), segment, "cannot read the current directory, keeping this document path relative: {e}");
            path
        })
}

/// Splits the body, already stripped of its URL scheme, into segments separated by unquoted colons,
/// removing the quotes that wrap a whole segment.
///
/// Also reports whether the last character consumed was an unquoted colon, which is what tells
/// `<url>:<path>:` from `<url>:<name>` in the two-segment case. A quote that is not at the head of
/// a segment stays a literal character and neither opens nor closes a quoted region.
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
    /// Never panics, whatever the input.
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

    /// Validation kept separate from parsing: a spec with all three fields unset — from the empty
    /// string, say — is a valid parse but not a specified document. Keeping the two apart lets a
    /// caller parse a value it does not yet require.
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
            // Without the scheme requirement, there would be nothing to build a URL from here.
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
            // The same failure mode as the two-segment case with a trailing colon.
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
