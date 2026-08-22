//! Rust port of `_internals/cli/conf_parse.py`'s `DocumentSpec` — the `--input`/`INPUT_REPORTS`
//! specifier type, ported one piece at a time as the rest of `conf_parse.py` (config precedence,
//! `FreeportsConfig`'s validators, the argparse-equivalent CLI flags) is designed. See
//! `agent-memory/rust-native-binary-plan.md`, Fase E, punto 3a.
//!
//! **Redesign, not a literal port (user confirmed, 2026-08-20)**: the Python original encodes
//! `url:path:name` as a hand-rolled state machine splitting on unquoted `:`, with an explicit
//! quoting escape so a literal `:` (e.g. a Windows drive letter, `"C:\Users\..."`) can appear
//! inside a segment. Verifying it empirically against the real Python (11 representative inputs)
//! surfaced two real bugs baked into that *design*, not just its implementation:
//! - A URL with an explicit port (`http://host:8080/path`) loses the port and corrupts `name`,
//!   because the port's own `:` is indistinguishable from the segment separator.
//! - A bare 3-segment specifier without a URL scheme (`a:b:c`) crashes with an unhandled
//!   `TypeError: unsupported operand type(s) for +: 'NoneType' and 'str'` instead of a clean
//!   error.
//!
//! Both share one root cause: `:` as the segment separator collides with `:`'s *other* meaning
//! inside a URL (scheme, port). No grep across this workspace (this repo, `freeports_dev`,
//! `analysis_finance_reports_formats`, docs) found a real caller relying on the multi-segment
//! form — no tests, no docs, no format-repo script. So rather than patch the state machine to
//! special-case ports, the separator itself changes to `|`: a character RFC 3986 excludes from
//! URLs outside percent-encoding, and rare enough in file paths (illegal in Windows filenames)
//! that the quoting escape becomes unnecessary and is dropped entirely — a Windows path's drive
//! colon is no longer special at all, so `C:\Users\me\report.pdf` now just works unquoted. This
//! makes both original bugs structurally impossible rather than patched around; existing
//! `--input`/`DOCUMENT_SPECS` invocations that used the old `:`-based multi-segment form need to
//! switch to `|` (single-segment bare URL/path specifiers, the overwhelmingly common case, are
//! unaffected — `:` inside them was never a delimiter to begin with).
//!
//! New grammar, replacing the old positional `len(segments)` branching (which also had a
//! genuinely confusing quirk: the same 2-segment count meant *different* fields depending on an
//! invisible trailing-colon parser state) with one unambiguous rule keyed on the number of `|`.
//! An empty segment anywhere means "not specified" (so trailing/leading `|` can leave a slot to
//! its default) rather than becoming a literal empty string, another footgun this redesign drops:
//! - `X`             — `X` is auto-detected as a URL (`http://`/`https://` prefix) or else a
//!   local path. `name` defaults to the url/path, stringified.
//! - `X|NAME`        — `X` auto-detected as above, `NAME` is an explicit display name.
//! - `URL|PATH|NAME` — `URL` must have a recognized scheme (a clear
//!   [`DocumentSpecError::NoSchemeForUrlSegment`], not a crash, if it doesn't); `PATH` is an
//!   explicit local override (e.g. where to save/read the PDF instead of a default location);
//!   `NAME` is an explicit display name. `PATH`/`NAME` may be left empty to take their default.
//! - Anything else (more than 2 `|`) is a [`DocumentSpecError::TooManySegments`].

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use url::Url;

/// Mirrors `DocumentSpec.from_str`'s `ValueError`s (segment-count/scheme errors) and
/// `input_should_be_specified`'s "at least one of url/path" `ValueError` — one Rust error type
/// covering both, since both can only surface at construction time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentSpecError {
    InvalidUrl { segment: String, message: String },
    NoSchemeForUrlSegment(String),
    TooManySegments(String),
    NoInputSpecified,
}

impl fmt::Display for DocumentSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentSpecError::InvalidUrl { segment, message } => {
                write!(f, "invalid URL `{segment}`: {message}")
            }
            DocumentSpecError::NoSchemeForUrlSegment(segment) => write!(
                f,
                "expected `{segment}` to start with http:// or https:// (3-segment form is URL|PATH|NAME)"
            ),
            DocumentSpecError::TooManySegments(specifier) => write!(
                f,
                "document specification parsing error: too many `|`-separated segments in `{specifier}`"
            ),
            DocumentSpecError::NoInputSpecified => write!(
                f,
                "you have to specify at least one input option: the url or the resource, the pdf file path or both"
            ),
        }
    }
}

impl std::error::Error for DocumentSpecError {}

/// A single document specification with optional url, path, and name — Rust port of
/// `DocumentSpec`. `name` is used as the `Report` column in output; if not given explicitly it
/// falls back to the url or path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSpec {
    pub url: Option<Url>,
    pub path: Option<PathBuf>,
    pub name: Option<String>,
}

/// `os.path.abspath` equivalent: makes `path` absolute by joining it onto the current directory
/// when relative, purely lexically — unlike `std::fs::canonicalize`, this never touches the
/// filesystem or requires the path to exist.
fn to_absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// `""` means "not specified" for any optional segment in the new grammar (a slot left empty to
/// take its default), not a literal empty string.
fn segment_or_none(segment: &str) -> Option<&str> {
    if segment.is_empty() { None } else { Some(segment) }
}

/// Detects and parses the `http://`/`https://` prefix that marks a segment as a URL rather than
/// a path. `Ok(None)` means the segment isn't URL-shaped at all (no error); `Err` means it looked
/// like a URL but didn't parse as one.
fn parse_url_prefixed(segment: &str) -> Result<Option<Url>, DocumentSpecError> {
    if segment.starts_with("http://") || segment.starts_with("https://") {
        Url::parse(segment).map(Some).map_err(|e| DocumentSpecError::InvalidUrl {
            segment: segment.to_string(),
            message: e.to_string(),
        })
    } else {
        Ok(None)
    }
}

impl DocumentSpec {
    /// Applies the same defaulting/validation every construction path needs: `name` falls back
    /// to `url` then `path` when not given explicitly, and at least one of `url`/`path` must be
    /// set. Mirrors `input_should_be_specified`.
    pub fn new(
        url: Option<Url>,
        path: Option<PathBuf>,
        name: Option<String>,
    ) -> Result<Self, DocumentSpecError> {
        if url.is_none() && path.is_none() {
            return Err(DocumentSpecError::NoInputSpecified);
        }
        let name = name.or_else(|| {
            url.as_ref()
                .map(|u| u.to_string())
                .or_else(|| path.as_ref().map(|p| p.display().to_string()))
        });
        Ok(DocumentSpec { url, path, name })
    }
}

impl FromStr for DocumentSpec {
    type Err = DocumentSpecError;

    fn from_str(specifier: &str) -> Result<Self, DocumentSpecError> {
        let specifier = specifier.trim();
        let segments: Vec<&str> = specifier.split('|').collect();
        match segments.as_slice() {
            [only] => {
                if only.is_empty() {
                    return Err(DocumentSpecError::NoInputSpecified);
                }
                match parse_url_prefixed(only)? {
                    Some(url) => DocumentSpec::new(Some(url), None, None),
                    None => DocumentSpec::new(None, Some(to_absolute(Path::new(only))), None),
                }
            }
            [source, name] => {
                let name = segment_or_none(name).map(str::to_string);
                if source.is_empty() {
                    return DocumentSpec::new(None, None, name);
                }
                match parse_url_prefixed(source)? {
                    Some(url) => DocumentSpec::new(Some(url), None, name),
                    None => DocumentSpec::new(None, Some(to_absolute(Path::new(source))), name),
                }
            }
            [url_segment, path_segment, name] => {
                let url = match parse_url_prefixed(url_segment)? {
                    Some(url) => url,
                    None => {
                        return Err(DocumentSpecError::NoSchemeForUrlSegment(
                            (*url_segment).to_string(),
                        ));
                    }
                };
                let path = segment_or_none(path_segment).map(|p| to_absolute(Path::new(p)));
                let name = segment_or_none(name).map(str::to_string);
                DocumentSpec::new(Some(url), path, name)
            }
            _ => Err(DocumentSpecError::TooManySegments(specifier.to_string())),
        }
    }
}

/// The single enum for every field-level validation failure shared across config sources
/// (command line, environment, config file, batch file row) — see the `cli::cmd_config`,
/// `cli::env_config`, `cli::file_config`, and `cli::job_config` modules' own `*Error` types, each
/// of which wraps one of these under an `InvalidField { <context>, source: ConfigError }` variant
/// instead of re-validating the value or re-deriving its own message. `DocumentSpecError` stays
/// its own type (it's also used standalone by [`DocumentSpec::from_str`]'s callers) and is
/// wrapped here rather than merged in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidVerbosity(String),
    VerbosityOutOfRange(i64),
    InvalidOutStructureMode(String),
    InvalidOutFlags(String),
    InvalidWorkers(String),
    InvalidBool(String),
    DocumentSpec(DocumentSpecError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::InvalidVerbosity(v) => write!(f, "`{v}` is not a valid integer"),
            ConfigError::VerbosityOutOfRange(v) => {
                write!(f, "verbosity must be between 0 and 5, got {v}")
            }
            ConfigError::InvalidOutStructureMode(v) => {
                write!(f, "`{v}` is not a valid out structure mode (expected one of REGULAR, SINGLE_FILE, STRUCTURED)")
            }
            ConfigError::InvalidOutFlags(message) => write!(f, "invalid out flags: {message}"),
            ConfigError::InvalidWorkers(v) => write!(f, "expected a positive integer, got `{v}`"),
            ConfigError::InvalidBool(v) => write!(f, "`{v}` is not castable to true/false"),
            ConfigError::DocumentSpec(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<DocumentSpecError> for ConfigError {
    fn from(e: DocumentSpecError) -> Self {
        ConfigError::DocumentSpec(e)
    }
}

/// Parses a raw string (env var, YAML scalar already read as a string, ...) into a [`Verbosity`],
/// the one place that turns free-form text into the type — every caller that has a `&str` rather
/// than an already-typed integer should go through this rather than rolling its own
/// `str::parse::<i64>()` + [`Verbosity::new`] with its own error message.
pub fn parse_verbosity(raw: &str) -> Result<Verbosity, ConfigError> {
    let trimmed = raw.trim();
    let n: i64 = trimmed.parse().map_err(|_| ConfigError::InvalidVerbosity(trimmed.to_string()))?;
    Verbosity::new(n)
}

/// Range-checks an already-parsed worker count (`--workers` on the command line arrives this way,
/// already through `clap`'s own integer parsing). Mirrors the Python original's "positive
/// integer, or it's an error" rule for `N_WORKERS`.
pub fn validate_workers(n: i64) -> Result<u32, ConfigError> {
    if n > 0 && n <= u32::MAX as i64 {
        Ok(n as u32)
    } else {
        Err(ConfigError::InvalidWorkers(n.to_string()))
    }
}

/// Parses a raw string (env var, YAML scalar, ...) into a worker count, combining integer parsing
/// with [`validate_workers`]'s range check under the one [`ConfigError::InvalidWorkers`] variant —
/// a non-numeric string and an out-of-range number are both "not a positive integer" to the
/// caller, and get the same message either way.
pub fn parse_workers(raw: &str) -> Result<u32, ConfigError> {
    let trimmed = raw.trim();
    let n: i64 = trimmed.parse().map_err(|_| ConfigError::InvalidWorkers(trimmed.to_string()))?;
    validate_workers(n)
}

/// Mirrors Pydantic's lax string-to-bool coercion (case-insensitive, trimmed) — the alias set
/// `SAVE_PDF`-like fields accept everywhere a boolean is read out of plain text (env vars, batch
/// file cells) rather than out of YAML's own native boolean scalar (see `file_config::as_bool`,
/// which doesn't need this at all).
pub fn parse_bool_alias(raw: &str) -> Result<bool, ConfigError> {
    match raw.trim().to_lowercase().as_str() {
        "true" | "yes" | "on" | "t" | "y" | "1" => Ok(true),
        "false" | "no" | "off" | "f" | "n" | "0" => Ok(false),
        other => Err(ConfigError::InvalidBool(other.to_string())),
    }
}

/// The verbosity level, `0..=5` — mirrors `Verbosity = conint(ge=0, le=5)`. `5 - verbosity` maps
/// onto Python `logging` level constants (`(5 - VERBOSITY) * 10`; see `cmd.py`), lower is louder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Verbosity(u8);

impl Verbosity {
    pub const MAX: u8 = 5;

    pub fn new(value: i64) -> Result<Self, ConfigError> {
        if (0..=Verbosity::MAX as i64).contains(&value) {
            Ok(Verbosity(value as u8))
        } else {
            Err(ConfigError::VerbosityOutOfRange(value))
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

/// Replaces `OutStructureNormalMode`/`OutStructureBatchMode`. The Python original defines these
/// as two separate `Enum` types, selected dynamically at parse time based on whether batch mode
/// is active (`SelectorOutProfile.cast_to_right_type`) — but both mode-specific extension lists
/// (`_out_structure_normal_mode`/`_out_structurebatch_mode`) are empty, so the two enums are
/// always, in practice, the exact same three values. The type duplication carries no behavior;
/// collapsed into one enum (Fase E simplification, see `agent-memory/rust-native-binary-plan.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutStructureMode {
    Regular,
    SingleFile,
    Structured,
}

impl fmt::Display for OutStructureMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            OutStructureMode::Regular => "REGULAR",
            OutStructureMode::SingleFile => "SINGLE_FILE",
            OutStructureMode::Structured => "STRUCTURED",
        };
        write!(f, "{name}")
    }
}

impl FromStr for OutStructureMode {
    type Err = ConfigError;

    /// Case-insensitive member-name lookup, mirroring `_cast_input_enum`'s
    /// `enum_cls[value.strip().upper()]`.
    fn from_str(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_uppercase().as_str() {
            "REGULAR" => Ok(OutStructureMode::Regular),
            "SINGLE_FILE" => Ok(OutStructureMode::SingleFile),
            "STRUCTURED" => Ok(OutStructureMode::Structured),
            other => Err(ConfigError::InvalidOutStructureMode(other.to_string())),
        }
    }
}

/// Replaces `OutFlagsNormalMode`/`OutFlagsBatchMode`. Unlike [`OutStructureMode`], these two
/// really do differ — Batch mode adds `SEPARATE_OUT_FILES` on top of `COMPRESSED` — but the
/// Python original enforces that distinction by using two different `Flag` *types*, selected the
/// same way as `OutStructureMode` (so parsing `SEPARATE_OUT_FILES` in Normal mode fails at cast
/// time, because that name simply isn't a member of `OutFlagsNormalMode`). Collapsed here into
/// one bitset; the same restriction is enforced explicitly instead of by type selection — see the
/// `FreeportsConfig` validation step (Fase E, punto 3b-vii) that checks `SEPARATE_OUT_FILES` is
/// only set when `BATCH_FILE` is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutFlags(u64);

impl OutFlags {
    pub const NONE: OutFlags = OutFlags(0);
    pub const COMPRESSED: OutFlags = OutFlags(1);
    pub const SEPARATE_OUT_FILES: OutFlags = OutFlags(2);

    pub fn contains(self, flag: OutFlags) -> bool {
        self.0 & flag.0 == flag.0
    }

    fn name_to_bit() -> std::collections::HashMap<String, u64> {
        std::collections::HashMap::from([
            ("COMPRESSED".to_string(), OutFlags::COMPRESSED.0),
            ("SEPARATE_OUT_FILES".to_string(), OutFlags::SEPARATE_OUT_FILES.0),
        ])
    }

    /// Mirrors `flag_from_string`: a `|`/`&`/`^`/`~` bitwise expression over flag names (case
    /// sensitive, matching the Rust expression evaluator this delegates to), or an empty/blank
    /// string for no flags set.
    pub fn parse(expression: &str) -> Result<Self, ConfigError> {
        let expression = expression.trim();
        if expression.is_empty() {
            return Ok(OutFlags::NONE);
        }
        crate::commons::flag_expr::evaluate(expression, &Self::name_to_bit())
            .map(OutFlags)
            .map_err(ConfigError::InvalidOutFlags)
    }
}

impl std::ops::BitOr for OutFlags {
    type Output = OutFlags;

    fn bitor(self, rhs: OutFlags) -> OutFlags {
        OutFlags(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    #[test]
    fn empty_specifier_is_no_input_specified() {
        assert_eq!("".parse::<DocumentSpec>(), Err(DocumentSpecError::NoInputSpecified));
    }

    #[test]
    fn whitespace_only_specifier_is_no_input_specified() {
        assert_eq!("   ".parse::<DocumentSpec>(), Err(DocumentSpecError::NoInputSpecified));
    }

    #[test]
    fn bare_relative_path_is_absolutized_and_becomes_its_own_name() {
        let spec = "report.pdf".parse::<DocumentSpec>().unwrap();
        assert!(spec.url.is_none());
        let path = spec.path.unwrap();
        assert!(path.is_absolute());
        assert!(path.ends_with("report.pdf"));
        assert_eq!(spec.name.unwrap(), path.display().to_string());
    }

    #[test]
    fn bare_absolute_path_is_preserved() {
        let spec = "/tmp/report.pdf".parse::<DocumentSpec>().unwrap();
        assert_eq!(spec.path.unwrap(), PathBuf::from("/tmp/report.pdf"));
        assert_eq!(spec.name.unwrap(), "/tmp/report.pdf");
    }

    #[test]
    fn leading_and_trailing_whitespace_is_trimmed() {
        let spec = "  /tmp/report.pdf  ".parse::<DocumentSpec>().unwrap();
        assert_eq!(spec.path.unwrap(), PathBuf::from("/tmp/report.pdf"));
    }

    #[test_case("http://example.com/report.pdf"; "http")]
    #[test_case("https://example.com/report.pdf"; "https")]
    fn bare_url_is_preserved_and_becomes_its_own_name(specifier: &str) {
        let spec = specifier.parse::<DocumentSpec>().unwrap();
        assert!(spec.path.is_none());
        assert_eq!(spec.url.as_ref().unwrap().to_string(), specifier);
        assert_eq!(spec.name.unwrap(), specifier);
    }

    /// Regression pin for bug A: the Python original dropped the port and corrupted `name` for
    /// this exact input.
    #[test]
    fn url_with_explicit_port_keeps_the_port_and_the_full_path() {
        let spec = "http://example.com:8080/report.pdf".parse::<DocumentSpec>().unwrap();
        let url = spec.url.unwrap();
        assert_eq!(url.port(), Some(8080));
        assert_eq!(url.to_string(), "http://example.com:8080/report.pdf");
        assert_eq!(spec.name.unwrap(), "http://example.com:8080/report.pdf");
    }

    #[test]
    fn two_segments_url_pipe_name_sets_explicit_name_no_path() {
        let spec = "http://example.com/report.pdf|MyReport".parse::<DocumentSpec>().unwrap();
        assert!(spec.path.is_none());
        assert_eq!(spec.url.unwrap().to_string(), "http://example.com/report.pdf");
        assert_eq!(spec.name.unwrap(), "MyReport");
    }

    #[test]
    fn two_segments_path_pipe_name_sets_explicit_name() {
        let spec = "local.pdf|MyReport".parse::<DocumentSpec>().unwrap();
        assert!(spec.url.is_none());
        assert!(spec.path.unwrap().ends_with("local.pdf"));
        assert_eq!(spec.name.unwrap(), "MyReport");
    }

    #[test]
    fn two_segments_empty_source_and_a_name_is_no_input_specified() {
        assert_eq!("|MyReport".parse::<DocumentSpec>(), Err(DocumentSpecError::NoInputSpecified));
    }

    #[test]
    fn two_segments_empty_name_defaults_like_the_bare_form() {
        let spec = "local.pdf|".parse::<DocumentSpec>().unwrap();
        assert_eq!(spec.name.as_ref().unwrap(), &spec.path.as_ref().unwrap().display().to_string());
    }

    #[test]
    fn three_segments_sets_url_path_and_name_explicitly() {
        let spec = "http://example.com/report.pdf|local.pdf|MyReport".parse::<DocumentSpec>().unwrap();
        assert_eq!(spec.url.unwrap().to_string(), "http://example.com/report.pdf");
        assert!(spec.path.unwrap().ends_with("local.pdf"));
        assert_eq!(spec.name.unwrap(), "MyReport");
    }

    #[test]
    fn three_segments_empty_path_leaves_path_unset() {
        let spec = "http://example.com/report.pdf||MyReport".parse::<DocumentSpec>().unwrap();
        assert!(spec.path.is_none());
        assert_eq!(spec.name.unwrap(), "MyReport");
    }

    #[test]
    fn three_segments_empty_name_defaults_to_the_url() {
        let spec = "http://example.com/report.pdf|local.pdf|".parse::<DocumentSpec>().unwrap();
        assert_eq!(spec.name.unwrap(), "http://example.com/report.pdf");
    }

    /// Regression pin for bug B: the Python original crashed with `TypeError: unsupported
    /// operand type(s) for +: 'NoneType' and 'str'` on exactly this shape (3 segments, no
    /// recognized scheme on the first one). The redesign turns it into a clean, typed error.
    #[test_case("a|b|c"; "plain letters")]
    #[test_case("report.pdf|out.pdf|MyReport"; "looks like a path, not a url")]
    fn three_segments_without_a_url_scheme_is_a_clean_error_not_a_crash(specifier: &str) {
        let first_segment = specifier.split('|').next().unwrap().to_string();
        assert_eq!(
            specifier.parse::<DocumentSpec>(),
            Err(DocumentSpecError::NoSchemeForUrlSegment(first_segment))
        );
    }

    #[test]
    fn malformed_url_segment_is_a_clean_invalid_url_error() {
        let result = "http://".parse::<DocumentSpec>();
        assert!(matches!(result, Err(DocumentSpecError::InvalidUrl { .. })));
    }

    #[test_case("a|b|c|d"; "four segments")]
    #[test_case("a|b|c|d|e"; "five segments")]
    fn more_than_three_segments_is_too_many_segments(specifier: &str) {
        assert_eq!(
            specifier.parse::<DocumentSpec>(),
            Err(DocumentSpecError::TooManySegments(specifier.to_string()))
        );
    }

    #[test]
    fn new_rejects_neither_url_nor_path() {
        assert_eq!(DocumentSpec::new(None, None, None), Err(DocumentSpecError::NoInputSpecified));
    }

    #[test]
    fn new_rejects_neither_url_nor_path_even_with_a_name() {
        assert_eq!(
            DocumentSpec::new(None, None, Some("MyReport".to_string())),
            Err(DocumentSpecError::NoInputSpecified)
        );
    }

    #[test]
    fn new_keeps_an_explicit_name_over_the_default() {
        let path = PathBuf::from("/tmp/report.pdf");
        let spec = DocumentSpec::new(None, Some(path.clone()), Some("MyReport".to_string())).unwrap();
        assert_eq!(spec.name.unwrap(), "MyReport");
        assert_eq!(spec.path.unwrap(), path);
    }

    #[test]
    fn new_defaults_name_from_url_when_both_url_and_path_are_given() {
        let url = Url::parse("http://example.com/report.pdf").unwrap();
        let path = PathBuf::from("/tmp/report.pdf");
        let spec = DocumentSpec::new(Some(url.clone()), Some(path), None).unwrap();
        assert_eq!(spec.name.unwrap(), url.to_string());
    }

    #[test_case(0; "min")]
    #[test_case(5; "max")]
    #[test_case(2; "middle")]
    fn verbosity_accepts_the_valid_range(value: i64) {
        assert_eq!(Verbosity::new(value).unwrap().get(), value as u8);
    }

    #[test_case(-1; "below min")]
    #[test_case(6; "above max")]
    fn verbosity_rejects_out_of_range(value: i64) {
        assert_eq!(Verbosity::new(value), Err(ConfigError::VerbosityOutOfRange(value)));
    }

    #[test_case("REGULAR", OutStructureMode::Regular; "regular upper")]
    #[test_case("regular", OutStructureMode::Regular; "regular lower")]
    #[test_case("  Single_File  ", OutStructureMode::SingleFile; "single file mixed case with whitespace")]
    #[test_case("STRUCTURED", OutStructureMode::Structured; "structured")]
    fn out_structure_mode_parses_case_insensitively(input: &str, expected: OutStructureMode) {
        assert_eq!(input.parse::<OutStructureMode>().unwrap(), expected);
    }

    #[test]
    fn out_structure_mode_rejects_unknown_values() {
        assert_eq!(
            "NOT_A_MODE".parse::<OutStructureMode>(),
            Err(ConfigError::InvalidOutStructureMode("NOT_A_MODE".to_string()))
        );
    }

    #[test]
    fn out_flags_empty_string_is_none() {
        assert_eq!(OutFlags::parse("").unwrap(), OutFlags::NONE);
        assert_eq!(OutFlags::parse("   ").unwrap(), OutFlags::NONE);
    }

    #[test]
    fn out_flags_single_name() {
        let flags = OutFlags::parse("COMPRESSED").unwrap();
        assert!(flags.contains(OutFlags::COMPRESSED));
        assert!(!flags.contains(OutFlags::SEPARATE_OUT_FILES));
    }

    #[test]
    fn out_flags_expression_combines_both() {
        let flags = OutFlags::parse("COMPRESSED | SEPARATE_OUT_FILES").unwrap();
        assert!(flags.contains(OutFlags::COMPRESSED));
        assert!(flags.contains(OutFlags::SEPARATE_OUT_FILES));
    }

    #[test]
    fn out_flags_bitor_combines() {
        let flags = OutFlags::COMPRESSED | OutFlags::SEPARATE_OUT_FILES;
        assert!(flags.contains(OutFlags::COMPRESSED));
        assert!(flags.contains(OutFlags::SEPARATE_OUT_FILES));
    }

    #[test]
    fn out_flags_rejects_unknown_name() {
        assert!(matches!(OutFlags::parse("NOT_A_FLAG"), Err(ConfigError::InvalidOutFlags(_))));
    }

    #[test_case("0", 0; "min")]
    #[test_case("5", 5; "max")]
    #[test_case("  3  ", 3; "whitespace trimmed")]
    fn parse_verbosity_accepts_the_valid_range(raw: &str, expected: i64) {
        assert_eq!(parse_verbosity(raw).unwrap(), Verbosity::new(expected).unwrap());
    }

    #[test]
    fn parse_verbosity_rejects_a_non_numeric_string() {
        assert_eq!(parse_verbosity("not-a-number"), Err(ConfigError::InvalidVerbosity("not-a-number".to_string())));
    }

    #[test]
    fn parse_verbosity_rejects_out_of_range_like_verbosity_new_does() {
        assert_eq!(parse_verbosity("6"), Err(ConfigError::VerbosityOutOfRange(6)));
        assert_eq!(parse_verbosity("-1"), Err(ConfigError::VerbosityOutOfRange(-1)));
    }

    #[test_case(1, 1; "min")]
    #[test_case(4_294_967_295, u32::MAX; "max u32")]
    fn validate_workers_accepts_the_valid_range(n: i64, expected: u32) {
        assert_eq!(validate_workers(n).unwrap(), expected);
    }

    #[test_case(0; "zero")]
    #[test_case(-1; "negative")]
    #[test_case(4_294_967_296; "above u32 max")]
    fn validate_workers_rejects_out_of_range(n: i64) {
        assert_eq!(validate_workers(n), Err(ConfigError::InvalidWorkers(n.to_string())));
    }

    #[test]
    fn parse_workers_accepts_a_valid_positive_integer_string() {
        assert_eq!(parse_workers(" 4 ").unwrap(), 4);
    }

    #[test_case("0"; "zero")]
    #[test_case("-1"; "negative")]
    fn parse_workers_rejects_the_same_out_of_range_values_as_validate_workers(raw: &str) {
        assert_eq!(parse_workers(raw), Err(ConfigError::InvalidWorkers(raw.to_string())));
    }

    #[test]
    fn parse_workers_rejects_a_non_numeric_string() {
        assert_eq!(parse_workers("not-a-number"), Err(ConfigError::InvalidWorkers("not-a-number".to_string())));
    }

    #[test_case("true"; "true literal")]
    #[test_case("YES"; "yes upper")]
    #[test_case(" on "; "on with whitespace")]
    #[test_case("t"; "t")]
    #[test_case("y"; "y")]
    #[test_case("1"; "one")]
    fn parse_bool_alias_accepts_every_truthy_alias(raw: &str) {
        assert_eq!(parse_bool_alias(raw), Ok(true));
    }

    #[test_case("false"; "false literal")]
    #[test_case("NO"; "no upper")]
    #[test_case(" off "; "off with whitespace")]
    #[test_case("f"; "f")]
    #[test_case("n"; "n")]
    #[test_case("0"; "zero")]
    fn parse_bool_alias_accepts_every_falsy_alias(raw: &str) {
        assert_eq!(parse_bool_alias(raw), Ok(false));
    }

    #[test]
    fn parse_bool_alias_rejects_anything_else() {
        assert_eq!(parse_bool_alias("maybe"), Err(ConfigError::InvalidBool("maybe".to_string())));
    }
}
