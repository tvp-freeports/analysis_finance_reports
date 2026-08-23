//! La grammatica degli ID del repo formati e la derivazione di `(formato, pipeline, indice)`.
//!
//! Ogni tabella CSV structured è indicizzata da una colonna `ID` che identifica *quale pipe di
//! quale pipeline di quale formato* la riga configura. La forma piena è
//! `<formato>(<pipeline>)/<indice>`, ma pipeline e indice sono quasi sempre omessi e vanno
//! **derivati**: è ciò che in Python fanno `add_format_name`/`add_pipeline_name`/`add_pipe_index`
//! di `repo/algorithms/pipelines_definition.py` con `str.extract` e `groupby().cumcount()`.
//!
//! Questo modulo porta quelle funzioni e — a differenza del porting Rust parziale che ne esiste in
//! `freeports_core` — **anche la parte di gruppo**: `cumcount()` non è esprimibile riga per riga,
//! perché l'indice di una riga dipende da tutte le altre righe dello stesso `(formato,
//! pipeline)`. È [`computed_ids`], la funzione che le tabelle di `structured` usano davvero.
//!
//! `onig` e non il crate `regex` (`PLAN.md` §2 principio 6 / §12 D9): i pattern sono scritti con
//! la sintassi Python del riferimento e vanno interpretati con la stessa.

use once_cell::sync::Lazy;
use onig::Regex;
use std::collections::HashMap;
use std::fmt;

/// `FORMAT_NAME_REGEXP` di `repo/metadata.py`: un nome di formato è un prefisso qualsiasi seguito
/// da un trattino, due lettere maiuscole e due cifre, con un `@XX` e un `.qualcosa` opzionali.
const FORMAT_NAME_PATTERN: &str = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.\/]+)?";

/// `pipeline_name_regexp`: minuscole, cifre e underscore. Nota la `*`: il nome **vuoto** è
/// legittimo, ed è quello della pipeline di default.
const PIPELINE_NAME_PATTERN: &str = r"[0-9a-z_]*";

/// `index_regexp`: uno slash seguito da cifre.
const INDEX_PATTERN: &str = r"/([0-9]+)";

fn pipeline_pattern() -> String {
    format!(r"\(({PIPELINE_NAME_PATTERN})\)")
}

/// Il gruppo `(pipeline)`, ovunque compaia nella stringa.
static PIPELINE_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(&pipeline_pattern()).expect("pattern fisso e valido"));

/// `({pipeline})?({index})?$` di `add_format_name`: la coda opzionale da togliere per ottenere il
/// nome del formato. Ancorato solo a destra, come l'originale, che si affida alla ricerca non
/// ancorata di `str.replace` per trovare la posizione più a sinistra da cui la coda combacia.
static SUFFIX_STRIP_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"({})?({INDEX_PATTERN})?$", pipeline_pattern())).expect("pattern fisso e valido"));

/// `{index}$` di `add_pipe_index` in modalità esplicita.
static INDEX_AT_END_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"{INDEX_PATTERN}$")).expect("pattern fisso e valido"));

static EXPANDABLE_NO_INDEX_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}({})?$", pipeline_pattern())).expect("pattern fisso e valido")
});

static EXPANDABLE_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}({})?({INDEX_PATTERN})?$", pipeline_pattern()))
        .expect("pattern fisso e valido")
});

static COMPLETE_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}{}{INDEX_PATTERN}$", pipeline_pattern()))
        .expect("pattern fisso e valido")
});

/// Quanto rigorosa deve essere la forma di un ID, a seconda della tabella che lo contiene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdFormat {
    /// Nome del formato con `(pipeline)` facoltativa e **nessun** indice.
    ExpandableNoIndex,
    /// Nome del formato con `(pipeline)` e `/indice` entrambi facoltativi.
    Expandable,
    /// Nome del formato con `(pipeline)` e `/indice` entrambi **obbligatori**: è la forma di un
    /// [`ComputedId`], cioè di un ID già derivato.
    Complete,
}

/// Il tipo di relazione fra una tabella secondaria e la tabella principale del suo gruppo.
///
/// Decide sia quanto rigorosa è la forma dell'ID accettata, sia come si deriva l'indice mancante
/// — le due `PipeIndexMode`/`MissingIndexPolicy` del riferimento, che qui non sono due enum
/// separati perché nel riferimento non esiste alcuna combinazione che non sia derivata da questo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FkRelation {
    /// Una riga per pipe: l'indice **non** si legge dall'ID, si conta.
    OneToOne,
    /// Zero o una riga per pipe: l'indice si legge dall'ID; se manca, si conta fra le sole righe
    /// che lo omettono.
    OneToMaybe,
    /// Più righe per pipe: l'indice si legge dall'ID; se manca, vale zero.
    OneToMany,
}

impl FkRelation {
    /// La forma dell'ID che la colonna accetta.
    pub fn id_format(self) -> IdFormat {
        match self {
            FkRelation::OneToOne => IdFormat::ExpandableNoIndex,
            FkRelation::OneToMaybe | FkRelation::OneToMany => IdFormat::Expandable,
        }
    }
}

/// L'identità completa di un pipe: `(formato, pipeline, indice)`.
///
/// È la chiave su cui le quattro tabelle di `structured` si uniscono — la `MultiIndex` di pandas
/// del riferimento, che qui è una chiave di `HashMap` (`PLAN.md` §6.1 punto 3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComputedId {
    pub format: String,
    pub pipeline: String,
    pub index: u32,
}

impl fmt::Display for ComputedId {
    /// La stessa stringa che il riferimento costruisce nella colonna `Computed ID`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})/{}", self.format, self.pipeline, self.index)
    }
}

/// Il nome del formato: l'ID meno la coda `(pipeline)` e/o `/indice`.
pub fn derive_format_name(id: &str) -> String {
    match SUFFIX_STRIP_REGEXP.find(id) {
        Some((start, _end)) => id[..start].to_string(),
        None => id.to_string(),
    }
}

/// Il nome della pipeline dichiarato nell'ID, oppure `default` se l'ID non ne dichiara uno.
///
/// Un `()` esplicito è un nome **vuoto trovato**, non un nome assente: `default` non lo sostituisce
/// — è la differenza fra un `NaN` e una cella vuota che `fillna` di pandas rispetta.
pub fn derive_pipeline_name(id: &str, default: Option<&str>) -> Option<String> {
    match PIPELINE_REGEXP.captures(id) {
        Some(caps) => Some(caps.at(1).unwrap_or_default().to_string()),
        None => default.map(str::to_string),
    }
}

/// L'indice dichiarato in coda all'ID, se c'è.
pub fn derive_pipe_index(id: &str) -> Option<u32> {
    INDEX_AT_END_REGEXP.captures(id).and_then(|caps| caps.at(1)).and_then(|digits| digits.parse().ok())
}

/// Verifica che `id` rispetti la forma richiesta.
pub fn id_matches(id: &str, format: IdFormat) -> bool {
    let regexp = match format {
        IdFormat::ExpandableNoIndex => &EXPANDABLE_NO_INDEX_REGEXP,
        IdFormat::Expandable => &EXPANDABLE_REGEXP,
        IdFormat::Complete => &COMPLETE_REGEXP,
    };
    regexp.is_match(id)
}

/// Deriva l'identità completa di ogni riga di una tabella, a partire dalla sua colonna `ID`.
///
/// È il porting di `create_index_format_name_pipe`, compresa la parte di gruppo che nessuna
/// funzione riga-per-riga può esprimere:
///
/// - [`FkRelation::OneToOne`] — l'indice è la **posizione della riga** all'interno del suo gruppo
///   `(formato, pipeline)`, contando dall'alto (il `cumcount()` di pandas). L'ID non deve portare
///   un indice.
/// - [`FkRelation::OneToMany`] — l'indice si legge dall'ID; le righe che lo omettono valgono tutte
///   zero, quindi più righe possono condividere lo stesso [`ComputedId`] (è proprio il senso di
///   "one to many": più righe per pipe).
/// - [`FkRelation::OneToMaybe`] — l'indice si legge dall'ID; le righe che lo omettono sono
///   numerate fra loro, **ignorando** quelle che un indice ce l'hanno. Quirk del riferimento
///   (`df[mask_missing].groupby(...).cumcount()`), conservato: significa che un indice esplicito e
///   uno derivato possono collidere.
///
/// L'ordine del risultato è quello delle righe in ingresso, uno a uno.
pub fn computed_ids(ids: &[&str], pipeline_default: Option<&str>, relation: FkRelation) -> Vec<ComputedId> {
    let bases: Vec<(String, String)> = ids
        .iter()
        .map(|id| {
            (derive_format_name(id), derive_pipeline_name(id, pipeline_default).unwrap_or_default())
        })
        .collect();

    let explicit: Vec<Option<u32>> = match relation {
        FkRelation::OneToOne => vec![None; ids.len()],
        FkRelation::OneToMaybe | FkRelation::OneToMany => ids.iter().map(|id| derive_pipe_index(id)).collect(),
    };

    let mut counters: HashMap<(String, String), u32> = HashMap::new();
    let mut out = Vec::with_capacity(ids.len());
    for (base, explicit) in bases.into_iter().zip(explicit) {
        let index = match (explicit, relation) {
            (Some(index), _) => index,
            (None, FkRelation::OneToMany) => 0,
            // `OneToOne` conta tutte le righe, `OneToMaybe` solo quelle senza indice esplicito:
            // in entrambi i casi il contatore è incrementato **solo** quando lo si usa, che è
            // esattamente ciò che distingue le due politiche.
            (None, _) => {
                let counter = counters.entry(base.clone()).or_insert(0);
                let index = *counter;
                *counter += 1;
                index
            }
        };
        out.push(ComputedId { format: base.0, pipeline: base.1, index });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    mod format_name {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("AMUNDI-EN24", "AMUNDI-EN24"; "bare format name")]
        #[test_case("AMUNDI-EN24(investments)", "AMUNDI-EN24"; "with pipeline")]
        #[test_case("AMUNDI-EN24/3", "AMUNDI-EN24"; "with index")]
        #[test_case("AMUNDI-EN24(investments)/3", "AMUNDI-EN24"; "with both")]
        #[test_case("AMUNDI-EN24()", "AMUNDI-EN24"; "with an empty pipeline group")]
        #[test_case("MEDIOLANUM-IT24@ES", "MEDIOLANUM-IT24@ES"; "with a country suffix")]
        #[test_case("MEDIOLANUM-IT24.b", "MEDIOLANUM-IT24.b"; "with a variant suffix")]
        #[test_case("MEDIOLANUM-IT24@ES.b(x)/1", "MEDIOLANUM-IT24@ES.b"; "with everything at once")]
        fn strips_the_optional_tail(id: &str, expected: &str) {
            assert_eq!(derive_format_name(id), expected);
        }

        #[test]
        fn leaves_a_string_without_any_tail_untouched() {
            assert_eq!(derive_format_name("whatever"), "whatever");
        }
    }

    mod pipeline_name {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_the_declared_pipeline() {
            assert_eq!(derive_pipeline_name("X-EN24(investments)", None), Some("investments".to_string()));
        }

        #[test]
        fn falls_back_to_the_default_when_no_group_is_declared() {
            assert_eq!(derive_pipeline_name("X-EN24", Some("investments")), Some("investments".to_string()));
        }

        #[test]
        fn without_a_group_and_without_a_default_there_is_no_name() {
            assert_eq!(derive_pipeline_name("X-EN24", None), None);
        }

        #[test]
        fn an_explicit_empty_group_wins_over_the_default() {
            // È la differenza fra `NaN` e cella vuota che `fillna` rispetta: `()` è un nome
            // trovato, e vale la pipeline di default *del formato*, non quella della tabella.
            assert_eq!(derive_pipeline_name("X-EN24()", Some("investments")), Some(String::new()));
        }

        #[test]
        fn reads_the_group_even_when_an_index_follows_it() {
            assert_eq!(derive_pipeline_name("X-EN24(manco)/2", None), Some("manco".to_string()));
        }

        #[test]
        fn accepts_digits_and_underscores_in_the_name() {
            assert_eq!(derive_pipeline_name("X-EN24(fund_assets_2)", None), Some("fund_assets_2".to_string()));
        }
    }

    mod pipe_index {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("X-EN24/0", Some(0); "zero")]
        #[test_case("X-EN24/7", Some(7); "single digit")]
        #[test_case("X-EN24/42", Some(42); "several digits")]
        #[test_case("X-EN24(inv)/2", Some(2); "after a pipeline group")]
        #[test_case("X-EN24", None; "absent")]
        #[test_case("X-EN24(inv)", None; "absent with a pipeline group")]
        #[test_case("X-EN24/2/3", Some(3); "only the last one counts")]
        fn reads_the_trailing_index(id: &str, expected: Option<u32>) {
            assert_eq!(derive_pipe_index(id), expected);
        }
    }

    mod shape_validation {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("AMUNDI-EN24", true; "bare name")]
        #[test_case("AMUNDI-EN24(inv)", true; "with pipeline")]
        #[test_case("AMUNDI-EN24()", true; "with an empty pipeline")]
        #[test_case("AMUNDI-EN24/0", false; "an index makes it invalid")]
        #[test_case("AMUNDI-EN24(inv)/0", false; "pipeline plus index is invalid")]
        #[test_case("nonsense", false; "not a format name at all")]
        #[test_case("AMUNDI-EN24(INV)", false; "an uppercase pipeline name is invalid")]
        fn expandable_no_index(id: &str, expected: bool) {
            assert_eq!(id_matches(id, IdFormat::ExpandableNoIndex), expected);
        }

        #[test_case("AMUNDI-EN24", true; "bare name")]
        #[test_case("AMUNDI-EN24(inv)", true; "with pipeline")]
        #[test_case("AMUNDI-EN24/0", true; "with index")]
        #[test_case("AMUNDI-EN24(inv)/0", true; "with both")]
        #[test_case("AMUNDI-EN24/x", false; "a non numeric index is invalid")]
        #[test_case("nonsense", false; "not a format name at all")]
        fn expandable(id: &str, expected: bool) {
            assert_eq!(id_matches(id, IdFormat::Expandable), expected);
        }

        #[test_case("AMUNDI-EN24(inv)/0", true; "both parts present")]
        #[test_case("AMUNDI-EN24()/0", true; "empty pipeline still counts as present")]
        #[test_case("AMUNDI-EN24(inv)", false; "index missing")]
        #[test_case("AMUNDI-EN24/0", false; "pipeline missing")]
        #[test_case("AMUNDI-EN24", false; "both missing")]
        fn complete(id: &str, expected: bool) {
            assert_eq!(id_matches(id, IdFormat::Complete), expected);
        }

        #[test]
        fn a_format_name_needs_the_two_letter_two_digit_country_year_suffix() {
            assert!(!id_matches("AMUNDI", IdFormat::Expandable));
            assert!(!id_matches("AMUNDI-EN2", IdFormat::Expandable));
            assert!(id_matches("AMUNDI-EN24", IdFormat::Expandable));
        }

        #[test]
        fn the_computed_id_of_any_row_always_has_the_complete_shape() {
            let ids = computed_ids(&["AMUNDI-EN24", "AMUNDI-EN24(manco)"], Some("investments"), FkRelation::OneToOne);
            for id in ids {
                assert!(id_matches(&id.to_string(), IdFormat::Complete), "{id} is not complete");
            }
        }
    }

    mod fk_relation {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn one_to_one_forbids_an_explicit_index_in_the_id() {
            assert_eq!(FkRelation::OneToOne.id_format(), IdFormat::ExpandableNoIndex);
        }

        #[test]
        fn the_other_two_relations_allow_one() {
            assert_eq!(FkRelation::OneToMaybe.id_format(), IdFormat::Expandable);
            assert_eq!(FkRelation::OneToMany.id_format(), IdFormat::Expandable);
        }
    }

    mod computed_id_display {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn renders_as_the_reference_computed_id_column() {
            let id = ComputedId { format: "AMUNDI-EN24".to_string(), pipeline: "investments".to_string(), index: 3 };
            assert_eq!(id.to_string(), "AMUNDI-EN24(investments)/3");
        }

        #[test]
        fn an_empty_pipeline_name_renders_as_empty_parentheses() {
            let id = ComputedId { format: "X-EN24".to_string(), pipeline: String::new(), index: 0 };
            assert_eq!(id.to_string(), "X-EN24()/0");
        }
    }

    mod derivation_one_to_one {
        use super::*;
        use pretty_assertions::assert_eq;

        fn ids(rows: &[&str]) -> Vec<String> {
            computed_ids(rows, Some("investments"), FkRelation::OneToOne).iter().map(ComputedId::to_string).collect()
        }

        #[test]
        fn numbers_the_rows_of_each_group_from_zero() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1", "A-EN24(investments)/2"]
            );
        }

        #[test]
        fn each_format_has_its_own_counter() {
            assert_eq!(
                ids(&["A-EN24", "B-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "B-EN24(investments)/0", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn each_pipeline_of_a_format_has_its_own_counter() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24(manco)", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(manco)/0", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn an_index_written_in_the_id_is_ignored_in_this_mode() {
            // La forma `ExpandableNoIndex` lo vieta a monte; qui si pinna che, se arrivasse, la
            // derivazione conterebbe comunque, senza leggerlo.
            assert_eq!(ids(&["A-EN24/9"]), vec!["A-EN24(investments)/0"]);
        }

        #[test]
        fn an_empty_table_produces_no_identity() {
            assert!(ids(&[]).is_empty());
        }

        #[test]
        fn without_a_default_the_pipeline_name_is_empty() {
            let out = computed_ids(&["A-EN24"], None, FkRelation::OneToOne);
            assert_eq!(out[0].to_string(), "A-EN24()/0");
        }
    }

    mod derivation_one_to_many {
        use super::*;
        use pretty_assertions::assert_eq;

        fn ids(rows: &[&str]) -> Vec<String> {
            computed_ids(rows, Some("investments"), FkRelation::OneToMany).iter().map(ComputedId::to_string).collect()
        }

        #[test]
        fn reads_the_index_written_in_the_id() {
            assert_eq!(ids(&["A-EN24/2"]), vec!["A-EN24(investments)/2"]);
        }

        #[test]
        fn every_row_without_an_index_lands_on_zero() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/0", "A-EN24(investments)/0"]
            );
        }

        #[test]
        fn several_rows_may_legitimately_share_one_identity() {
            // È il senso stesso di "one to many": più righe configurano lo stesso pipe.
            let out = ids(&["A-EN24/1", "A-EN24/1"]);
            assert_eq!(out[0], out[1]);
        }

        #[test]
        fn explicit_and_implicit_indexes_coexist_in_the_same_table() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24/1", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1", "A-EN24(investments)/0"]
            );
        }
    }

    mod derivation_one_to_maybe {
        use super::*;
        use pretty_assertions::assert_eq;

        fn ids(rows: &[&str]) -> Vec<String> {
            computed_ids(rows, Some("investments"), FkRelation::OneToMaybe).iter().map(ComputedId::to_string).collect()
        }

        #[test]
        fn reads_the_index_written_in_the_id() {
            assert_eq!(ids(&["A-EN24/2"]), vec!["A-EN24(investments)/2"]);
        }

        #[test]
        fn rows_without_an_index_are_numbered_among_themselves() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn a_row_carrying_an_index_does_not_advance_the_counter_of_the_others() {
            // Quirk del riferimento pinnato di proposito: `df[mask_missing].groupby().cumcount()`
            // conta solo fra le righe mancanti, quindi un indice esplicito e uno derivato possono
            // collidere — qui la terza riga ricade su `/1`, che la seconda ha già dichiarato.
            assert_eq!(
                ids(&["A-EN24", "A-EN24/1", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn the_counter_is_per_format_and_pipeline() {
            assert_eq!(
                ids(&["A-EN24", "B-EN24", "A-EN24(manco)", "A-EN24"]),
                vec![
                    "A-EN24(investments)/0",
                    "B-EN24(investments)/0",
                    "A-EN24(manco)/0",
                    "A-EN24(investments)/1"
                ]
            );
        }
    }

    mod real_repository_shapes {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_page_classify_table_numbers_its_header_rows_by_explicit_index() {
            // Righe reali di `content/algorithms/structured/page_classify/args.csv`.
            let out = computed_ids(&["CARNE-EN23/0", "CARNE-EN23/0", "CARNE-EN23/1"], Some(""), FkRelation::OneToMany);
            assert_eq!(
                out.iter().map(ComputedId::to_string).collect::<Vec<_>>(),
                vec!["CARNE-EN23()/0", "CARNE-EN23()/0", "CARNE-EN23()/1"]
            );
        }

        #[test]
        fn the_investments_args_table_numbers_bare_ids_by_position() {
            let out = computed_ids(&["AMUNDI-EN24", "AMUNDI-IT24", "ANIMA-EN23"], Some("investments"), FkRelation::OneToOne);
            assert_eq!(
                out.iter().map(ComputedId::to_string).collect::<Vec<_>>(),
                vec!["AMUNDI-EN24(investments)/0", "AMUNDI-IT24(investments)/0", "ANIMA-EN23(investments)/0"]
            );
        }

        #[test]
        fn an_additional_args_row_with_a_full_id_matches_the_args_row_it_refers_to() {
            let principal = computed_ids(&["AMUNDI-IT24"], Some("investments"), FkRelation::OneToOne);
            let secondary =
                computed_ids(&["AMUNDI-IT24(investments)/0"], Some("investments"), FkRelation::OneToMaybe);
            assert_eq!(principal, secondary);
        }
    }
}
