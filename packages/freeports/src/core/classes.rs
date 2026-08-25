//! `PdfBlock` e `TextBlock`: le due unita' di lavoro che attraversano la pipeline.
//!
//! Un `PdfBlock` e' cio' che il segmento *pdf_extract* ritaglia da una pagina; un `TextBlock` e'
//! cio' che il segmento *text_filter* ne ricava dopo aver deciso che il testo e' rilevante, e
//! conserva un riferimento al blocco PDF da cui proviene. Il segmento *deserialize* legge poi i
//! `TextBlock` e produce le entita' di `output`.
//!
//! Rispetto al riferimento cambiano tre cose, tutte volute (`PLAN.md` §4.2):
//!
//! - `metadata` e `content` non sono piu' `dict`/`Any` Python ma [`BlockValue`] (vedi
//!   `classes::value`), quindi serde funziona per derivazione e non serve un `serialization.py`;
//! - `type_block` e' un newtype [`BlockType`] invece di una `String` nuda: i tipi di blocco li
//!   estendono i repo formati, quindi un enum chiuso non funzionerebbe (decisione D2), ma il
//!   newtype da' comunque un tipo distinto e un posto dove tenere le costanti standard;
//! - `Hash` e' derivato. Nel riferimento `__hash__` *mutava* `metadata` per riuscire a calcolare
//!   l'hash, e siccome `__eq__` era definito come uguaglianza di hash, anche un semplice `==`
//!   mutava entrambi gli operandi (decisione D3). Qui non serve: [`BlockValue`] e' gia'
//!   `Hash + Ord` di suo.
//!
//! Le cinque eccezioni marcatrici del riferimento (`ExpectedPdfBlockNotFound`,
//! `ExpectedTextBlockNotFound`, `PageParseFail`, `LineParseFail`, `ExtractionFieldFail`) **non**
//! stanno qui: diventano varianti di `PipeError`/`PageError` insieme al motore (`PLAN.md` §8),
//! perche' descrivono fallimenti dell'esecuzione di un pipe, non del modello dati.

pub mod value;

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use value::{BlockValue, BlockValueError};

/// Il tipo di un blocco, come stringa: `"FUND"`, `"TABLE_BODY"`, un nome inventato da un repo
/// formati, ...
///
/// E' un newtype su [`Cow<'static, str>`] e non su `String` per una ragione precisa: `Cow`
/// permette di dichiarare i tipi standard come vere costanti associate
/// (`const FUND: BlockType = ...`), cosa impossibile con `String`, senza pagare
/// un'allocazione ogni volta che si nomina un tipo standard. Un tipo costruito a runtime da un
/// repo formati usa il ramo `Owned` e si confronta normalmente con uno costante.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlockType(Cow<'static, str>);

impl BlockType {
    /// Blocco PDF marcato come rilevante dal classificatore standard
    /// (`OnePdfBlockType::RELEVANT_BLOCK` / `OneTextBlockType::RELEVANT_BLOCK` nel riferimento).
    pub const RELEVANT_BLOCK: BlockType = BlockType(Cow::Borrowed("RELEVANT_BLOCK"));
    /// Nome del fondo ritagliato dalla pagina (`ResultStandardExtraction::FUND_NAME`).
    pub const FUND_NAME: BlockType = BlockType(Cow::Borrowed("FUND_NAME"));
    /// Riga che dichiara la valuta del prospetto (`ResultStandardExtraction::CURRENCY_STATEMENT`).
    pub const CURRENCY_STATEMENT: BlockType = BlockType(Cow::Borrowed("CURRENCY_STATEMENT"));
    /// Corpo della tabella degli investimenti (`ResultStandardExtraction::TABLE_BODY`).
    pub const TABLE_BODY: BlockType = BlockType(Cow::Borrowed("TABLE_BODY"));
    /// Articolo SFDR dichiarato (`ResultStandardExtraction::SFDR_ARTICLE`).
    pub const SFDR_ARTICLE: BlockType = BlockType(Cow::Borrowed("SFDR_ARTICLE"));
    /// Indicatore di classe della pagina (`ResultStandardExtraction::PAGE_CLASS`).
    pub const PAGE_CLASS: BlockType = BlockType(Cow::Borrowed("PAGE_CLASS"));
    /// Fondo, come `TextBlock` prodotto da `standard_fund_txt_blk`.
    pub const FUND: BlockType = BlockType(Cow::Borrowed("FUND"));
    /// Societa' di gestione, come `TextBlock` prodotto da `standard_management_company_txt_blk`.
    pub const MANAGEMENT_COMPANY: BlockType = BlockType(Cow::Borrowed("MANAGEMENT_COMPANY"));
    /// Gestore degli investimenti, come `TextBlock` prodotto da
    /// `standard_investmet_manager_txt_blk`.
    pub const INVESTMENTS_MANAGER: BlockType = BlockType(Cow::Borrowed("INVESTMENTS_MANAGER"));
    /// Riga di tabella riconosciuta come titolo azionario
    /// (`ResultStandardFiltering::EQUITY_TARGET` nel riferimento). Aggiunta in M5 insieme a
    /// `TextFilterInvestmentsStandard`, che è l'unico a produrla.
    pub const EQUITY_TARGET: BlockType = BlockType(Cow::Borrowed("EQUITY_TARGET"));
    /// Riga di tabella riconosciuta come obbligazione, cioè con un tasso d'interesse o una
    /// scadenza nel testo (`ResultStandardFiltering::BOND_TARGET` nel riferimento).
    pub const BOND_TARGET: BlockType = BlockType(Cow::Borrowed("BOND_TARGET"));

    /// I tipi standard, in un unico posto: serve ai test e ai messaggi diagnostici che vogliono
    /// suggerire "forse intendevi uno di questi".
    pub const STANDARD: &'static [BlockType] = &[
        BlockType::RELEVANT_BLOCK,
        BlockType::FUND_NAME,
        BlockType::CURRENCY_STATEMENT,
        BlockType::TABLE_BODY,
        BlockType::SFDR_ARTICLE,
        BlockType::PAGE_CLASS,
        BlockType::FUND,
        BlockType::MANAGEMENT_COMPANY,
        BlockType::INVESTMENTS_MANAGER,
        BlockType::EQUITY_TARGET,
        BlockType::BOND_TARGET,
    ];

    /// Costruisce un tipo di blocco arbitrario — la via che usano i repo formati.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        BlockType(name.into())
    }

    /// Il nome del tipo.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&'static str> for BlockType {
    fn from(name: &'static str) -> Self {
        BlockType(Cow::Borrowed(name))
    }
}

impl From<String> for BlockType {
    fn from(name: String) -> Self {
        BlockType(Cow::Owned(name))
    }
}

/// Confronto diretto con una stringa, per non costringere ogni `if` a costruire un `BlockType`.
impl PartialEq<str> for BlockType {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for BlockType {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Errori del modulo: (de)serializzazione JSON di un blocco e lettura tipizzata dei suoi campi.
#[derive(Debug, thiserror::Error)]
pub enum BlockError {
    #[error("block JSON (de)serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Value(#[from] BlockValueError),
}

/// Un ritaglio di pagina PDF: cosa e', da dove viene, cosa contiene.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PdfBlock {
    pub type_block: BlockType,
    pub metadata: BTreeMap<String, BlockValue>,
    pub content: BlockValue,
}

impl PdfBlock {
    pub fn new(
        type_block: impl Into<BlockType>,
        metadata: BTreeMap<String, BlockValue>,
        content: impl Into<BlockValue>,
    ) -> Self {
        PdfBlock { type_block: type_block.into(), metadata, content: content.into() }
    }

    /// Blocco senza metadati — il caso piu' comune nei pipe di estrazione semplici.
    pub fn bare(type_block: impl Into<BlockType>, content: impl Into<BlockValue>) -> Self {
        PdfBlock::new(type_block, BTreeMap::new(), content)
    }

    /// Legge un metadato tipizzato, distinguendo "assente" da "di tipo sbagliato".
    pub fn metadata_or_fail(&self, field: &str) -> Result<&BlockValue, BlockValueError> {
        self.metadata.get(field).ok_or_else(|| BlockValueError::MissingField { field: field.to_string() })
    }

    pub fn to_json(&self) -> Result<String, BlockError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self, BlockError> {
        Ok(serde_json::from_str(json)?)
    }
}

/// Testo giudicato rilevante, con il blocco PDF da cui e' stato ricavato.
///
/// `pdf_block` e' opzionale perche' un `TextBlock` puo' anche nascere da un contenuto costruito
/// (`TextBlock::from_content`), senza una porzione di pagina alle spalle — per esempio quando il
/// valore e' una costante di formato o una [`crate::core::promise::Promise`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextBlock {
    pub type_block: BlockType,
    pub metadata: BTreeMap<String, BlockValue>,
    pub content: BlockValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_block: Option<Box<PdfBlock>>,
}

impl TextBlock {
    /// Costruisce un `TextBlock` a partire da un `PdfBlock`, **ereditandone il contenuto**: e' la
    /// via normale, e la ragione per cui non esiste un costruttore che accetti sia `pdf_block`
    /// sia `content` (potrebbero contraddirsi).
    pub fn new(
        type_block: impl Into<BlockType>,
        metadata: BTreeMap<String, BlockValue>,
        pdf_block: PdfBlock,
    ) -> Self {
        TextBlock {
            type_block: type_block.into(),
            metadata,
            content: pdf_block.content.clone(),
            pdf_block: Some(Box::new(pdf_block)),
        }
    }

    /// Costruisce un `TextBlock` da un contenuto dato, senza blocco PDF di provenienza.
    pub fn from_content(
        type_block: impl Into<BlockType>,
        metadata: BTreeMap<String, BlockValue>,
        content: impl Into<BlockValue>,
    ) -> Self {
        TextBlock {
            type_block: type_block.into(),
            metadata,
            content: content.into(),
            pdf_block: None,
        }
    }

    /// Legge un metadato tipizzato, distinguendo "assente" da "di tipo sbagliato".
    pub fn metadata_or_fail(&self, field: &str) -> Result<&BlockValue, BlockValueError> {
        self.metadata.get(field).ok_or_else(|| BlockValueError::MissingField { field: field.to_string() })
    }

    pub fn to_json(&self) -> Result<String, BlockError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self, BlockError> {
        Ok(serde_json::from_str(json)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise::Promise;

    fn metadata(pairs: &[(&str, BlockValue)]) -> BTreeMap<String, BlockValue> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    mod block_type {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_standard_constants_carry_their_own_name() {
            assert_eq!(BlockType::FUND.as_str(), "FUND");
            assert_eq!(BlockType::TABLE_BODY.as_str(), "TABLE_BODY");
            assert_eq!(BlockType::INVESTMENTS_MANAGER.as_str(), "INVESTMENTS_MANAGER");
        }

        #[test]
        fn all_standard_constants_are_distinct() {
            let names: std::collections::BTreeSet<&str> =
                BlockType::STANDARD.iter().map(BlockType::as_str).collect();
            assert_eq!(names.len(), BlockType::STANDARD.len());
        }

        /// Il punto del newtype su `Cow`: una costante e un tipo costruito a runtime dallo stesso
        /// nome sono lo stesso `BlockType`, con lo stesso hash.
        #[test]
        fn constant_and_runtime_built_type_coincide() {
            let from_repo = BlockType::new(String::from("FUND"));
            assert_eq!(from_repo, BlockType::FUND);

            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let hash_of = |t: &BlockType| {
                let mut h = DefaultHasher::new();
                t.hash(&mut h);
                h.finish()
            };
            assert_eq!(hash_of(&from_repo), hash_of(&BlockType::FUND));
        }

        #[test]
        fn a_type_invented_by_a_format_repo_is_legitimate() {
            let custom = BlockType::new(String::from("ANIMA_TABELLA_STRANA"));
            assert_eq!(custom.as_str(), "ANIMA_TABELLA_STRANA");
            assert_ne!(custom, BlockType::TABLE_BODY);
        }

        #[test]
        fn compares_directly_with_a_string() {
            assert!(BlockType::FUND == "FUND");
            assert!(BlockType::FUND != "TABLE_BODY");
        }

        #[test]
        fn display_and_as_str_coincide() {
            for t in BlockType::STANDARD {
                assert_eq!(t.to_string(), t.as_str());
            }
        }

        #[test]
        fn serializes_as_a_bare_string() {
            assert_eq!(serde_json::to_string(&BlockType::FUND).unwrap(), "\"FUND\"");
            let back: BlockType = serde_json::from_str("\"FUND\"").unwrap();
            assert_eq!(back, BlockType::FUND);
        }
    }

    mod pdf_block {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn keeps_type_metadata_and_content() {
            let blk = PdfBlock::new(
                BlockType::FUND_NAME,
                metadata(&[("page", BlockValue::Int(3))]),
                "Acme Global Fund",
            );
            assert_eq!(blk.type_block, BlockType::FUND_NAME);
            assert_eq!(blk.metadata.get("page"), Some(&BlockValue::Int(3)));
            assert_eq!(blk.content.as_str(), Some("Acme Global Fund"));
        }

        #[test]
        fn bare_constructs_without_metadata() {
            let blk = PdfBlock::bare(BlockType::TABLE_BODY, "riga");
            assert!(blk.metadata.is_empty());
            assert_eq!(blk.content.as_str(), Some("riga"));
        }

        #[test]
        fn the_content_can_be_a_promise() {
            let blk = PdfBlock::bare(BlockType::FUND_NAME, Promise::new("fund!"));
            assert!(blk.content.is_promise());
            assert_eq!(blk.content.as_promise().map(Promise::id), Some("fund"));
        }

        #[test]
        fn metadata_or_fail_names_the_missing_field() {
            let blk = PdfBlock::bare(BlockType::TABLE_BODY, "riga");
            assert_eq!(
                blk.metadata_or_fail("row").unwrap_err(),
                BlockValueError::MissingField { field: "row".into() }
            );
        }
    }

    mod text_block {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn new_inherits_the_content_from_the_pdf_block() {
            let pdf = PdfBlock::bare(BlockType::FUND_NAME, "Café Fund");
            let txt = TextBlock::new(BlockType::FUND, BTreeMap::new(), pdf.clone());
            assert_eq!(txt.content, pdf.content);
            assert_eq!(txt.pdf_block.as_deref(), Some(&pdf));
        }

        #[test]
        fn new_does_not_inherit_the_pdf_blocks_metadata() {
            let pdf = PdfBlock::new(
                BlockType::FUND_NAME,
                metadata(&[("page", BlockValue::Int(3))]),
                "Café Fund",
            );
            let txt = TextBlock::new(BlockType::FUND, BTreeMap::new(), pdf);
            assert!(txt.metadata.is_empty());
        }

        #[test]
        fn from_content_has_no_pdf_block() {
            let txt = TextBlock::from_content(BlockType::MANAGEMENT_COMPANY, BTreeMap::new(), "Acme SGR");
            assert!(txt.pdf_block.is_none());
            assert_eq!(txt.content.as_str(), Some("Acme SGR"));
        }

        #[test]
        fn from_content_accepts_a_promise() {
            let txt = TextBlock::from_content(BlockType::FUND, BTreeMap::new(), Promise::new("fund[]"));
            assert!(txt.content.is_promise());
            assert!(txt.content.as_promise().unwrap().multiple());
        }
    }

    mod identity {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of<T: Hash>(v: &T) -> u64 {
            let mut h = DefaultHasher::new();
            v.hash(&mut h);
            h.finish()
        }

        #[test]
        fn identical_blocks_are_equal_and_hash_the_same() {
            let a = PdfBlock::new(BlockType::FUND_NAME, metadata(&[("p", BlockValue::Int(1))]), "x");
            let b = PdfBlock::new(BlockType::FUND_NAME, metadata(&[("p", BlockValue::Int(1))]), "x");
            assert_eq!(a, b);
            assert_eq!(hash_of(&a), hash_of(&b));
        }

        #[test]
        fn type_metadata_and_content_all_three_matter() {
            let base = PdfBlock::new(BlockType::FUND_NAME, metadata(&[("p", BlockValue::Int(1))]), "x");
            assert_ne!(base, PdfBlock::new(BlockType::TABLE_BODY, base.metadata.clone(), "x"));
            assert_ne!(base, PdfBlock::new(BlockType::FUND_NAME, BTreeMap::new(), "x"));
            assert_ne!(base, PdfBlock::new(BlockType::FUND_NAME, base.metadata.clone(), "y"));
        }

        /// L'ordine di inserimento dei metadati non conta, e — a differenza del riferimento —
        /// confrontare o hashare due blocchi non li modifica (`PLAN.md` D3).
        #[test]
        fn comparing_and_hashing_does_not_modify_the_blocks() {
            let mut m1 = BTreeMap::new();
            m1.insert("a".to_string(), BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]));
            m1.insert("b".to_string(), BlockValue::Int(9));
            let mut m2 = BTreeMap::new();
            m2.insert("b".to_string(), BlockValue::Int(9));
            m2.insert("a".to_string(), BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]));

            let a = PdfBlock::new(BlockType::FUND_NAME, m1, "x");
            let b = PdfBlock::new(BlockType::FUND_NAME, m2, "x");
            let a_before = a.clone();
            let b_before = b.clone();

            assert_eq!(a, b);
            assert_eq!(hash_of(&a), hash_of(&b));
            assert_eq!(a, a_before, "comparison modified the left operand");
            assert_eq!(b, b_before, "comparison modified the right operand");
        }

        #[test]
        fn a_text_block_with_and_without_pdf_block_are_not_equal() {
            let pdf = PdfBlock::bare(BlockType::FUND_NAME, "x");
            let with_pdf = TextBlock::new(BlockType::FUND, BTreeMap::new(), pdf);
            let without_pdf = TextBlock::from_content(BlockType::FUND, BTreeMap::new(), "x");
            assert_eq!(with_pdf.content, without_pdf.content);
            assert_ne!(with_pdf, without_pdf);
        }
    }

    mod serde_roundtrip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn pdf_block_survives_json() {
            let blk = PdfBlock::new(
                BlockType::TABLE_BODY,
                metadata(&[("row", BlockValue::Int(2)), ("promise", BlockValue::from(Promise::new("f!")))]),
                BlockValue::List(vec![BlockValue::from("a"), BlockValue::Null]),
            );
            assert_eq!(PdfBlock::from_json(&blk.to_json().unwrap()).unwrap(), blk);
        }

        #[test]
        fn text_block_with_pdf_block_survives_json() {
            let pdf = PdfBlock::new(BlockType::FUND_NAME, metadata(&[("p", BlockValue::Int(1))]), "Acme");
            let txt = TextBlock::new(BlockType::FUND, metadata(&[("m", BlockValue::from("v"))]), pdf);
            assert_eq!(TextBlock::from_json(&txt.to_json().unwrap()).unwrap(), txt);
        }

        #[test]
        fn text_block_without_pdf_block_omits_the_field_and_rereads_it() {
            let txt = TextBlock::from_content(BlockType::FUND, BTreeMap::new(), "Acme");
            let json = txt.to_json().unwrap();
            assert!(!json.contains("pdf_block"), "json: {json}");
            assert_eq!(TextBlock::from_json(&json).unwrap(), txt);
        }

        #[test]
        fn malformed_json_is_a_module_error() {
            let err = PdfBlock::from_json("{ non json").unwrap_err();
            assert!(matches!(err, BlockError::Json(_)), "{err:?}");
            assert!(err.to_string().starts_with("block JSON (de)serialization failed"));
        }

        #[test]
        fn json_with_missing_fields_is_an_error() {
            assert!(PdfBlock::from_json(r#"{"type_block":"FUND"}"#).is_err());
        }
    }
}
