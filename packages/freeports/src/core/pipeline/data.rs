//! Il vocabolario condiviso dai pipe: cosa entra in un pipe ([`FilterData`]), cosa ne esce
//! ([`Extracted`]), come fallisce ([`PipeError`]).
//!
//! `PLAN.md` §5.4 e §8. Sta in un modulo suo, e non accanto a
//! [`Pipeline`](crate::core::pipeline::Pipeline), perché lo usano tutti e cinque i pezzi del
//! motore (i tre trait dei pipe, `Pipeline`, `PipelinesBundle`, `Algorithm`): `core::pipeline` lo
//! ri-esporta, quindi il percorso pubblico resta `core::pipeline::{FilterData, Extracted,
//! PipeError}`.
//!
//! **[`FilterData`] — decisione dell'utente (2026-08-23, `agent-memory/M5-implementation-plan.md`
//! D-M5-1)**, che era l'unica domanda a bloccare M5 (`PLAN.md` §13 punto 1). La semantica è quella
//! del riferimento, **non** una versione che mostra sempre tutto: al primo step dello schedule un
//! pipe vede **solo** le target companies, dagli step successivi vede **solo** l'accumulo dei
//! risultati di tutti gli step precedenti. Le due cose non sono mai visibili insieme — da qui
//! l'enum invece di una struct a due campi. Conseguenza accettata: un pipe che ha bisogno delle
//! target companies (oggi `TextFilterInvestmentsStandard`) funziona solo se schedulato al primo
//! step, esattamente come nel riferimento.
//!
//! **[`Extracted`] nasce parziale.** `PLAN.md` §5.4 elenca dieci varianti d'entità (`Equity`,
//! `Bond`, `Fund`, `FundAssets`, `FundSfdrClassification`, `FundEsgIndicator`, `FundRename`,
//! `FundMerge`, `ManagementCompany`, `InvestmentsManager`) che vivono in `output::classes`, cioè
//! in M8. Anticiparle qui significherebbe fare due volte il lavoro di M8 (validazioni,
//! `PromisableFields`, campi che il motore non ha motivo di conoscere), e l'utente ha chiesto
//! esplicitamente che sia `output::classes` — non altro — a restare l'ultima dipendenza aperta.
//! M5 definisce quindi le due varianti costruibili oggi; **M8 aggiunge le altre dieci**, e solo
//! allora il `match` esaustivo di `output::routines` diventa scrivibile.

use crate::core::classes::value::{BlockValue, BlockValueError};
use crate::core::page::PageError;
use crate::core::promise_resolution::PromiseMap;
use crate::core::schedule::PageClass;
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::classes::fund::Fund;
use crate::output::classes::fund_assets::FundAssets;
use crate::output::classes::fund_change_name::{FundMerge, FundRename};
use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
use crate::output::classes::investment::{Bond, Equity};
use crate::formats_utils::pdf_extract::commons::CommonsError;
use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;

/// Fallimento di un singolo pipe (`PLAN.md` §8).
///
/// Ogni variante nomina il pipe che l'ha prodotta: nel riferimento un pipe che fallisce non è
/// identificabile, ed è la ragione per cui i tre trait hanno un `name()` (`PLAN.md` §5.1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipeError {
    /// La pagina non è interpretabile. **Non fatale**: [`Algorithm`](crate::core::algorithm::Algorithm)
    /// logga e salta la pagina, come il `PageParseFail` del riferimento.
    #[error("pipe `{pipe}` could not parse the page: {source}")]
    PageParse {
        pipe: String,
        #[source]
        source: PageError,
    },
    /// Il pipe non ha trovato ciò che si aspettava di trovare.
    #[error("pipe `{pipe}` failed to extract: {message}")]
    Extraction { pipe: String, message: String },
    /// Una conversione di campo è fallita (le funzioni di `deserialize::cast`).
    #[error("pipe `{pipe}` could not cast field `{field}`: {message}")]
    Cast { pipe: String, field: String, message: String },
    /// Un pipe **definito dall'autore del formato** (Python) ha sollevato. È il confine di
    /// `PLAN.md` §3: nessun `PyErr` risale oltre `formats_repo`, diventa questa variante.
    #[error("author pipe `{pipe}` of pipeline `{pipeline}` failed: {message}")]
    Author { pipeline: String, pipe: String, message: String },
    /// Un campo di `metadata`/`content` non aveva il tipo atteso.
    #[error("pipe `{pipe}`: {source}")]
    Value {
        pipe: String,
        #[source]
        source: BlockValueError,
    },
}

impl PipeError {
    pub fn page_parse(pipe: impl Into<String>, source: PageError) -> Self {
        PipeError::PageParse { pipe: pipe.into(), source }
    }

    pub fn extraction(pipe: impl Into<String>, message: impl Into<String>) -> Self {
        PipeError::Extraction { pipe: pipe.into(), message: message.into() }
    }

    pub fn cast(
        pipe: impl Into<String>,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        PipeError::Cast { pipe: pipe.into(), field: field.into(), message: message.into() }
    }

    pub fn author(
        pipeline: impl Into<String>,
        pipe: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        PipeError::Author {
            pipeline: pipeline.into(),
            pipe: pipe.into(),
            message: message.into(),
        }
    }

    pub fn value(pipe: impl Into<String>, source: BlockValueError) -> Self {
        PipeError::Value { pipe: pipe.into(), source }
    }

    /// Vero solo per [`PipeError::PageParse`]: è il fallimento che l'algoritmo assorbe saltando
    /// la pagina invece di interrompere l'elaborazione.
    pub fn is_page_failure(&self) -> bool {
        matches!(self, PipeError::PageParse { .. })
    }

    /// Il nome del pipe che ha prodotto l'errore.
    pub fn pipe(&self) -> &str {
        match self {
            PipeError::PageParse { pipe, .. }
            | PipeError::Extraction { pipe, .. }
            | PipeError::Cast { pipe, .. }
            | PipeError::Author { pipe, .. }
            | PipeError::Value { pipe, .. } => pipe,
        }
    }

    /// Converte l'errore locale di `pdf_extract::commons` (M3) nell'errore definitivo del motore.
    ///
    /// Mantiene la promessa scritta nel doc-comment di quel modulo ("quando M5/M8 introdurranno i
    /// tipi definitivi, questo enum si convertirà in quello"). Non è un `impl From` perché il
    /// nome del pipe non è ricavabile dall'errore: inventare una stringa vuota renderebbe i
    /// messaggi peggiori di quelli che sostituisce.
    ///
    /// [`CommonsError::PageParseFail`] diventa il fallimento **non fatale** di pagina;
    /// [`CommonsError::ExpectedTextNotFound`], che il riferimento lascia risalire come errore
    /// vero, diventa [`PipeError::Extraction`].
    pub fn from_commons(pipe: impl Into<String>, error: CommonsError) -> Self {
        let pipe = pipe.into();
        match error {
            CommonsError::PageParseFail { ref source } => {
                PipeError::PageParse { source: PageError::ParseFail { message: source.to_string() }, pipe }
            }
            CommonsError::ExpectedTextNotFound { .. } => {
                PipeError::Extraction { message: error.to_string(), pipe }
            }
        }
    }
}

/// Le promesse che un pipe di deserializzazione deposita: coppie `id → contributo`, nell'ordine
/// in cui il pipe le ha prodotte.
///
/// È la forma tipizzata del dict che nel riferimento i deserializer restituiscono e che
/// `merge_into_multimap` versa nella multimappa. L'ordine conta: chi arriva dopo vince quando la
/// promessa non è *multiple* (vedi [`FlatPromiseMap::fulfill`](crate::core::promise_resolution::FlatPromiseMap::fulfill)).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct PromiseEntries(Vec<(String, BlockValue)>);

impl PromiseEntries {
    pub fn new() -> Self {
        PromiseEntries::default()
    }

    pub fn push(&mut self, id: impl Into<String>, value: impl Into<BlockValue>) {
        self.0.push((id.into(), value.into()));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &BlockValue)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Versa i contributi nella multimappa, in ordine.
    pub fn merge_into(&self, map: &mut PromiseMap) {
        // Gli id, non il conteggio: sono la chiave con cui una promessa si ritrova poi nel
        // registro delle promesse non risolte.
        tracing::debug!(
            ids = %self.0.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "),
            "promises deposited"
        );
        map.merge(self.0.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

impl<K: Into<String>, V: Into<BlockValue>> FromIterator<(K, V)> for PromiseEntries {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut entries = PromiseEntries::new();
        for (k, v) in iter {
            entries.push(k, v);
        }
        entries
    }
}

/// Il risultato di un pipe di deserializzazione.
///
/// Rimpiazza il dispatch per `isinstance` su liste Python eterogenee del riferimento: con un enum
/// il "ricomponi i risultati per tipo" di `run_documents` diventa un `match` che il compilatore
/// verifica (`PLAN.md` §5.4).
///
/// **Ora completo.** M5 l'aveva aperto con le sole `Promises`/`PageClass`; M7 ha aggiunto le tre
/// entità che la decisione D-M7-2 dell'utente ha anticipato da M8 (`Fund`, `Equity`, `Bond`, le
/// uniche che i pipe `deserialize` standard sapessero già costruire). M8 aggiunge le sette
/// varianti restanti di `PLAN.md` §5.4, chiudendo il modulo.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Extracted {
    /// Una partecipazione azionaria su una società bersaglio.
    Equity(Equity),
    /// Un'obbligazione di una società bersaglio.
    Bond(Bond),
    /// Un fondo, con il solo nome.
    Fund(Fund),
    /// Il patrimonio di un fondo a una certa data.
    FundAssets(FundAssets),
    /// La classificazione SFDR (art. 6/8/9) dichiarata di un fondo.
    FundSfdrClassification(FundSfdrClassification),
    /// Un indicatore ESG di un fondo.
    FundEsgIndicator(FundEsgIndicator),
    /// La rinomina di un fondo.
    FundRename(FundRename),
    /// La fusione di un fondo in un altro.
    FundMerge(FundMerge),
    /// La società di gestione di un fondo.
    ManagementCompany(ManagementCompany),
    /// Il gestore degli investimenti di un fondo.
    InvestmentsManager(InvestmentsManager),
    /// Le promesse depositate dal pipe, da versare nella multimappa di risoluzione.
    Promises(PromiseEntries),
    /// L'esito della pipeline di classificazione: la class della pagina, o `None` se il pipe non
    /// ha saputo classificarla.
    PageClass(Option<PageClass>),
}

impl Extracted {
    /// La page class, se questo risultato viene dalla pipeline di classificazione.
    ///
    /// `Some(None)` e `None` sono cose diverse: il primo è "una classificazione c'è stata, ed è
    /// 'nessuna class'", il secondo è "questo risultato non è una classificazione".
    #[allow(clippy::option_option)]
    pub fn as_page_class(&self) -> Option<&Option<PageClass>> {
        match self {
            Extracted::PageClass(class) => Some(class),
            _ => None,
        }
    }

    /// Le promesse, se questo risultato ne porta.
    pub fn as_promises(&self) -> Option<&PromiseEntries> {
        match self {
            Extracted::Promises(entries) => Some(entries),
            _ => None,
        }
    }

    /// Il fondo, se questo risultato ne è uno.
    pub fn as_fund(&self) -> Option<&Fund> {
        match self {
            Extracted::Fund(fund) => Some(fund),
            _ => None,
        }
    }

    /// La partecipazione azionaria, se questo risultato ne è una.
    pub fn as_equity(&self) -> Option<&Equity> {
        match self {
            Extracted::Equity(equity) => Some(equity),
            _ => None,
        }
    }

    /// L'obbligazione, se questo risultato ne è una.
    pub fn as_bond(&self) -> Option<&Bond> {
        match self {
            Extracted::Bond(bond) => Some(bond),
            _ => None,
        }
    }

    /// Il patrimonio del fondo, se questo risultato ne è uno.
    pub fn as_fund_assets(&self) -> Option<&FundAssets> {
        match self {
            Extracted::FundAssets(assets) => Some(assets),
            _ => None,
        }
    }

    /// La classificazione SFDR, se questo risultato ne è una.
    pub fn as_fund_sfdr_classification(&self) -> Option<&FundSfdrClassification> {
        match self {
            Extracted::FundSfdrClassification(classification) => Some(classification),
            _ => None,
        }
    }

    /// L'indicatore ESG, se questo risultato ne è uno.
    pub fn as_fund_esg_indicator(&self) -> Option<&FundEsgIndicator> {
        match self {
            Extracted::FundEsgIndicator(indicator) => Some(indicator),
            _ => None,
        }
    }

    /// La rinomina, se questo risultato ne è una.
    pub fn as_fund_rename(&self) -> Option<&FundRename> {
        match self {
            Extracted::FundRename(rename) => Some(rename),
            _ => None,
        }
    }

    /// La fusione, se questo risultato ne è una.
    pub fn as_fund_merge(&self) -> Option<&FundMerge> {
        match self {
            Extracted::FundMerge(merge) => Some(merge),
            _ => None,
        }
    }

    /// La società di gestione, se questo risultato ne è una.
    pub fn as_management_company(&self) -> Option<&ManagementCompany> {
        match self {
            Extracted::ManagementCompany(company) => Some(company),
            _ => None,
        }
    }

    /// Il gestore degli investimenti, se questo risultato ne è uno.
    pub fn as_investments_manager(&self) -> Option<&InvestmentsManager> {
        match self {
            Extracted::InvestmentsManager(manager) => Some(manager),
            _ => None,
        }
    }
}

/// Cosa un pipe `text_filter` sa del contesto in cui gira (`PLAN.md` §5.4).
///
/// Enum e non struct: le due cose non sono mai disponibili insieme — vedi la nota su D-M5-1 nel
/// doc-comment del modulo.
#[derive(Debug, Clone, Copy)]
pub enum FilterData<'a> {
    /// Primo step dello schedule: le società bersaglio con cui il pipe deve fare match.
    TargetCompanies(&'a [CompanyMatchInfos]),
    /// Step successivi: l'accumulo dei risultati di **tutti** gli step precedenti.
    Previous(&'a [Extracted]),
}

impl<'a> FilterData<'a> {
    /// Le target companies, se è il primo step; una slice vuota altrimenti.
    pub fn target_companies(&self) -> &'a [CompanyMatchInfos] {
        match self {
            FilterData::TargetCompanies(companies) => companies,
            FilterData::Previous(_) => &[],
        }
    }

    /// I risultati degli step precedenti, se non è il primo step; una slice vuota altrimenti.
    pub fn previous(&self) -> &'a [Extracted] {
        match self {
            FilterData::Previous(results) => results,
            FilterData::TargetCompanies(_) => &[],
        }
    }

    /// `FilterData` con cui girano le pipeline di classificazione, dove non c'è né uno step
    /// precedente né una lista di target companies (nel riferimento è `None`).
    pub const EMPTY: FilterData<'static> = FilterData::Previous(&[]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::text_filter::matcher::TargetCompanyInput;

    fn companies() -> Vec<CompanyMatchInfos> {
        CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
            name: "Acme".to_string(),
            regexs: vec![],
            symbols: vec![],
            buds: vec![],
        }])
        .expect("fixed, valid input")
    }

    mod pipe_error_classification {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn only_a_page_parse_failure_is_absorbed_by_the_algorithm() {
            let page_fail = PipeError::page_parse(
                "p",
                PageError::ParseFail { message: "no table".to_string() },
            );
            assert!(page_fail.is_page_failure());

            for other in [
                PipeError::extraction("p", "m"),
                PipeError::cast("p", "f", "m"),
                PipeError::author("pl", "p", "m"),
                PipeError::value("p", BlockValueError::MissingField { field: "f".to_string() }),
            ] {
                assert!(!other.is_page_failure(), "{other:?} must not be absorbed");
            }
        }

        #[test]
        fn every_variant_names_the_pipe_that_produced_it() {
            let errors = [
                PipeError::page_parse("a", PageError::ParseFail { message: String::new() }),
                PipeError::extraction("a", "m"),
                PipeError::cast("a", "f", "m"),
                PipeError::author("pl", "a", "m"),
                PipeError::value("a", BlockValueError::MissingField { field: "f".to_string() }),
            ];
            for err in errors {
                assert_eq!(err.pipe(), "a");
            }
        }
    }

    mod pipe_error_messages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_parse_failure_quotes_the_underlying_message() {
            let err =
                PipeError::page_parse("extract", PageError::ParseFail { message: "no table".into() });
            assert_eq!(err.to_string(), "pipe `extract` could not parse the page: no table");
        }

        #[test]
        fn an_extraction_failure_names_pipe_and_reason() {
            assert_eq!(
                PipeError::extraction("extract", "fund not found").to_string(),
                "pipe `extract` failed to extract: fund not found"
            );
        }

        #[test]
        fn a_cast_failure_names_the_field() {
            assert_eq!(
                PipeError::cast("deser", "market_value", "not a number").to_string(),
                "pipe `deser` could not cast field `market_value`: not a number"
            );
        }

        #[test]
        fn an_author_failure_names_both_pipeline_and_pipe() {
            assert_eq!(
                PipeError::author("investments", "custom_extract", "KeyError: 'x'").to_string(),
                "author pipe `custom_extract` of pipeline `investments` failed: KeyError: 'x'"
            );
        }

        #[test]
        fn a_value_failure_forwards_the_block_value_message() {
            let err =
                PipeError::value("deser", BlockValueError::MissingField { field: "fund".into() });
            assert_eq!(err.to_string(), "pipe `deser`: missing field 'fund'");
        }
    }

    mod pipe_error_from_commons {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_commons_page_failure_becomes_a_non_fatal_page_parse() {
            let commons = CommonsError::PageParseFail {
                source: Box::new(CommonsError::ExpectedTextNotFound { name: "fund".to_string() }),
            };
            let err = PipeError::from_commons("extract", commons);
            assert!(err.is_page_failure());
            assert_eq!(
                err,
                PipeError::page_parse(
                    "extract",
                    PageError::ParseFail {
                        message: "Pdf block during extraction of \"fund\" not found".to_string()
                    }
                )
            );
        }

        #[test]
        fn a_bare_not_found_becomes_a_fatal_extraction_failure() {
            let commons = CommonsError::ExpectedTextNotFound { name: "fund".to_string() };
            let err = PipeError::from_commons("extract", commons);
            assert!(!err.is_page_failure());
            assert_eq!(
                err,
                PipeError::extraction("extract", "Pdf block during extraction of \"fund\" not found")
            );
        }
    }

    mod promise_entries {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn keeps_the_order_in_which_the_pipe_produced_them() {
            let entries: PromiseEntries =
                [("id", BlockValue::from("first")), ("id", BlockValue::from("second"))]
                    .into_iter()
                    .collect();
            let values: Vec<&BlockValue> = entries.iter().map(|(_, v)| v).collect();
            assert_eq!(values, vec![&BlockValue::from("first"), &BlockValue::from("second")]);
        }

        #[test]
        fn merging_appends_every_contribution_under_its_id() {
            let entries: PromiseEntries =
                [("a", BlockValue::from(1i64)), ("b", BlockValue::from(2i64)), ("a", BlockValue::from(3i64))]
                    .into_iter()
                    .collect();
            let mut map = PromiseMap::new();
            entries.merge_into(&mut map);

            assert_eq!(map.get("a"), Some([BlockValue::from(1i64), BlockValue::from(3i64)].as_slice()));
            assert_eq!(map.get("b"), Some([BlockValue::from(2i64)].as_slice()));
        }

        #[test]
        fn merging_twice_accumulates_rather_than_replacing() {
            let entries: PromiseEntries = [("a", BlockValue::from(1i64))].into_iter().collect();
            let mut map = PromiseMap::new();
            entries.merge_into(&mut map);
            entries.merge_into(&mut map);
            assert_eq!(map.get("a").map(<[BlockValue]>::len), Some(2));
        }

        #[test]
        fn an_empty_set_of_entries_leaves_the_map_untouched() {
            let mut map = PromiseMap::new();
            PromiseEntries::new().merge_into(&mut map);
            assert!(map.is_empty());
            assert!(PromiseEntries::new().is_empty());
            assert_eq!(PromiseEntries::new().len(), 0);
        }
    }

    mod extracted_accessors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_class_result_exposes_its_class() {
            let e = Extracted::PageClass(Some(PageClass::new("investments")));
            assert_eq!(e.as_page_class(), Some(&Some(PageClass::new("investments"))));
        }

        #[test]
        fn an_unclassified_page_is_still_a_page_class_result() {
            // `Some(None)` (una classificazione che dice "nessuna class") non va confuso con
            // `None` (questo risultato non e' una classificazione).
            let e = Extracted::PageClass(None);
            assert_eq!(e.as_page_class(), Some(&None));
            assert!(e.as_promises().is_none());
        }

        #[test]
        fn a_promises_result_is_not_a_page_class() {
            let e = Extracted::Promises(PromiseEntries::new());
            assert!(e.as_page_class().is_none());
            assert!(e.as_promises().is_some());
        }
    }

    mod filter_data_semantics {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_first_step_sees_target_companies_and_no_previous_results() {
            let companies = companies();
            let data = FilterData::TargetCompanies(&companies);
            assert_eq!(data.target_companies().len(), 1);
            assert!(data.previous().is_empty());
        }

        #[test]
        fn a_later_step_sees_previous_results_and_no_target_companies() {
            let previous = vec![Extracted::PageClass(None)];
            let data = FilterData::Previous(&previous);
            assert_eq!(data.previous().len(), 1);
            assert!(data.target_companies().is_empty());
        }

        #[test]
        fn the_empty_filter_data_used_by_page_classification_sees_neither() {
            let data = FilterData::EMPTY;
            assert!(data.target_companies().is_empty());
            assert!(data.previous().is_empty());
        }
    }

    /// M8: le sette varianti d'entità che `output::classes` aggiunge (`agent-memory/
    /// M8-implementation-plan.md` §3 passo 7), più i loro accessori `as_*` e un `match`
    /// esaustivo che ne previene la regressione silenziosa.
    mod new_entity_variants {
        use super::*;
        use crate::commons::consts::{Currency, SfdrArticle};
        use crate::commons::date::Date;
        use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
        use crate::output::classes::fund_assets::FundAssets;
        use crate::output::classes::fund_change_name::{FundMerge, FundRename};
        use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
        use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
        use crate::output::classes::investment::InvestmentFields;
        use std::collections::BTreeSet;

        fn equity() -> Equity {
            Equity::build(InvestmentFields::new(
                "Acme Corp",
                "Acme",
                BlockValue::from("Alpha Fund"),
                BlockValue::from(1000.0),
                BlockValue::from(Currency::EUR),
            ))
            .expect("fixed, valid fixture")
        }

        fn bond() -> Bond {
            Bond::build(
                InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    BlockValue::from("Alpha Fund"),
                    BlockValue::from(1000.0),
                    BlockValue::from(Currency::EUR),
                ),
                None,
                None,
            )
            .expect("fixed, valid fixture")
        }

        fn fund_assets() -> FundAssets {
            FundAssets::build("Alpha Fund", 100.0, 40.0, 60.0, &BlockValue::from(Currency::EUR), None)
                .expect("fixed, valid fixture")
        }

        fn fund_sfdr_classification() -> FundSfdrClassification {
            FundSfdrClassification::build("Alpha Fund", &BlockValue::from(SfdrArticle::Art8))
                .expect("fixed, valid fixture")
        }

        fn fund_esg_indicator() -> FundEsgIndicator {
            FundEsgIndicator::build(&BlockValue::from("Alpha Fund"), "GHG intensity", "12.3")
                .expect("fixed, valid fixture")
        }

        fn fund_rename() -> FundRename {
            FundRename::build("Old Fund", "New Fund", &BlockValue::from(Date::new(2025, 1, 1).unwrap()))
                .expect("fixed, valid fixture")
        }

        fn fund_merge() -> FundMerge {
            FundMerge::build("Old Fund", "New Fund", &BlockValue::from(Date::new(2025, 1, 1).unwrap()))
                .expect("fixed, valid fixture")
        }

        fn management_company() -> ManagementCompany {
            ManagementCompany::build(&BlockValue::from("Acme AM"), &BlockValue::Set(BTreeSet::new()))
                .expect("fixed, valid fixture")
        }

        fn investments_manager() -> InvestmentsManager {
            InvestmentsManager::build(&BlockValue::from("Acme IM"), &BlockValue::Set(BTreeSet::new()))
                .expect("fixed, valid fixture")
        }

        /// Un esemplare per ciascuna delle dodici varianti di `Extracted`, nell'ordine di
        /// dichiarazione di `PLAN.md` §5.4. Le prove di esaustività sotto iterano su questa lista,
        /// stesso schema di `BlockValue::tests::one_of_each` (M2).
        fn one_of_each() -> Vec<Extracted> {
            vec![
                Extracted::Equity(equity()),
                Extracted::Bond(bond()),
                Extracted::Fund(Fund::new("Alpha Fund")),
                Extracted::FundAssets(fund_assets()),
                Extracted::FundSfdrClassification(fund_sfdr_classification()),
                Extracted::FundEsgIndicator(fund_esg_indicator()),
                Extracted::FundRename(fund_rename()),
                Extracted::FundMerge(fund_merge()),
                Extracted::ManagementCompany(management_company()),
                Extracted::InvestmentsManager(investments_manager()),
                Extracted::Promises(PromiseEntries::new()),
                Extracted::PageClass(None),
            ]
        }

        mod accessors {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn as_fund_assets_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundAssets(_));
                    assert_eq!(e.as_fund_assets().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_sfdr_classification_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundSfdrClassification(_));
                    assert_eq!(e.as_fund_sfdr_classification().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_esg_indicator_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundEsgIndicator(_));
                    assert_eq!(e.as_fund_esg_indicator().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_rename_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundRename(_));
                    assert_eq!(e.as_fund_rename().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_merge_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundMerge(_));
                    assert_eq!(e.as_fund_merge().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_management_company_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::ManagementCompany(_));
                    assert_eq!(e.as_management_company().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_investments_manager_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::InvestmentsManager(_));
                    assert_eq!(e.as_investments_manager().is_some(), expect_some, "{e:?}");
                }
            }

            /// Le sette varianti nuove non devono mai soddisfare gli accessori di M5
            /// (`as_fund`/`as_equity`/`as_bond`/`as_promises`/`as_page_class`).
            #[test]
            fn the_five_pre_existing_accessors_reject_every_new_variant_too() {
                for e in one_of_each() {
                    assert_eq!(e.as_fund().is_some(), matches!(e, Extracted::Fund(_)), "{e:?}");
                    assert_eq!(e.as_equity().is_some(), matches!(e, Extracted::Equity(_)), "{e:?}");
                    assert_eq!(e.as_bond().is_some(), matches!(e, Extracted::Bond(_)), "{e:?}");
                    assert_eq!(e.as_promises().is_some(), matches!(e, Extracted::Promises(_)), "{e:?}");
                    assert_eq!(e.as_page_class().is_some(), matches!(e, Extracted::PageClass(_)), "{e:?}");
                }
            }
        }

        /// P1 (`agent-memory/P1-implementation-plan.md` §3): un `Extracted` torna dal processo
        /// figlio al padre serializzato in JSON. Riusa `one_of_each`, cosi' una tredicesima variante
        /// entra automaticamente nel round-trip invece di restare scoperta in silenzio.
        mod serde_round_trip {
            use super::*;
            use pretty_assertions::assert_eq;

            use std::collections::BTreeMap;

            fn round_trip(e: &Extracted) -> Extracted {
                let json = serde_json::to_string(e).expect("an extracted value must serialize");
                serde_json::from_str(&json).expect("a serialized extracted value must deserialize back")
            }

            #[test]
            fn every_variant_survives_a_json_round_trip() {
                for e in one_of_each() {
                    assert_eq!(round_trip(&e), e, "variant did not survive: {e:?}");
                }
            }

            /// Le promesse sono l'unica variante che porta una struttura annidata arbitraria
            /// (`BlockValue`, ricorsivo). Un `PromiseEntries` vuoto, che `one_of_each` usa, non
            /// direbbe nulla sull'annidamento.
            #[test]
            fn promise_entries_survive_with_nested_block_values() {
                let entries: PromiseEntries = [
                    ("scalar", BlockValue::Int(42)),
                    (
                        "list",
                        BlockValue::List(vec![BlockValue::Str("a".to_string()), BlockValue::Null, BlockValue::Bool(true)]),
                    ),
                    (
                        "map",
                        BlockValue::Map(BTreeMap::from([(
                            "inner".to_string(),
                            BlockValue::Set(BTreeSet::from([BlockValue::Int(1), BlockValue::Int(2)])),
                        )])),
                    ),
                ]
                .into_iter()
                .collect();
                let e = Extracted::Promises(entries);
                assert_eq!(round_trip(&e), e);
            }

            /// L'ordine delle promesse decide chi vince quando la promessa non e' *multiple*
            /// (vedi `FlatPromiseMap::fulfill`): se il round-trip lo perdesse, un job in parallelo
            /// produrrebbe un risultato diverso dallo stesso job in sequenziale.
            #[test]
            fn the_order_of_promise_entries_is_preserved() {
                let entries: PromiseEntries =
                    [("k", BlockValue::Int(1)), ("k", BlockValue::Int(2)), ("k", BlockValue::Int(3))].into_iter().collect();
                let restored = round_trip(&Extracted::Promises(entries.clone()));
                assert_eq!(restored, Extracted::Promises(entries));
            }

            #[test]
            fn a_page_class_survives_both_when_present_and_when_absent() {
                for value in [None, Some(PageClass::new("investments"))] {
                    let e = Extracted::PageClass(value.clone());
                    assert_eq!(round_trip(&e), e, "page class {value:?} did not survive");
                }
            }
        }

        /// Il `match` esaustivo — senza `_ =>` — su tutte e dodici le varianti: se una tredicesima
        /// venisse aggiunta senza toccare questo test, il compilatore lo segnalerebbe qui.
        mod exhaustive_match {
            use super::*;
            use pretty_assertions::assert_eq;

            fn label(e: &Extracted) -> &'static str {
                match e {
                    Extracted::Equity(_) => "equity",
                    Extracted::Bond(_) => "bond",
                    Extracted::Fund(_) => "fund",
                    Extracted::FundAssets(_) => "fund_assets",
                    Extracted::FundSfdrClassification(_) => "fund_sfdr_classification",
                    Extracted::FundEsgIndicator(_) => "fund_esg_indicator",
                    Extracted::FundRename(_) => "fund_rename",
                    Extracted::FundMerge(_) => "fund_merge",
                    Extracted::ManagementCompany(_) => "management_company",
                    Extracted::InvestmentsManager(_) => "investments_manager",
                    Extracted::Promises(_) => "promises",
                    Extracted::PageClass(_) => "page_class",
                }
            }

            #[test]
            fn every_variant_matches_its_own_arm_in_declaration_order() {
                let expected = [
                    "equity",
                    "bond",
                    "fund",
                    "fund_assets",
                    "fund_sfdr_classification",
                    "fund_esg_indicator",
                    "fund_rename",
                    "fund_merge",
                    "management_company",
                    "investments_manager",
                    "promises",
                    "page_class",
                ];
                let labels: Vec<&str> = one_of_each().iter().map(label).collect();
                assert_eq!(labels, expected);
            }
        }
    }
}
