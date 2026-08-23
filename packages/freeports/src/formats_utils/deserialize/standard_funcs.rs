//! Pipe `deserialize` standard — sottoinsieme autosufficiente di
//! `freeports_core/src/formats_utils/deserialize/standard_funcs.rs`.
//!
//! Scope deciso dall'utente (`agent-memory/M4-implementation-plan.md` §0, opzione A): solo
//! `DeserializerPageClassifyStandard` e' costruibile senza `output::classes` (M8) — le altre
//! (`DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
//! `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
//! `DeserializerInvestmentsManagerStandard`) costruiscono entita' che non esistono ancora.
//! Dopo la chiusura di M5 questa e' l'**unica** dipendenza che tiene aperta M4: nessuna di queste
//! aspetta piu' il motore.
//!
//! Da M5 `DeserializerPageClassifyStandard` implementa anche
//! [`DeserializePipe`](crate::core::pipeline::DeserializePipe): `call` resta l'API diretta che
//! restituisce il `BlockValue` grezzo, `call_page_class` lo traduce nella
//! [`PageClass`](crate::core::schedule::PageClass) tipizzata, e il trait e' la forma che il
//! motore usa.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct DeserializerPageClassifyStandard;
//! impl DeserializerPageClassifyStandard {
//!     pub fn call(&self, txt_blk: &TextBlock) -> Result<BlockValue, DeserializeStandardFuncsError>;
//! }
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum DeserializeStandardFuncsError { /* un enum locale, stesso trattamento provvisorio di
//!     CommonsError (M3, pdf_extract::commons) — verra' assorbito da PipeError in M5 */ }
//! ```
//!
//! `call` legge `txt_blk.metadata["page_type"]` e lo restituisce cosi' com'e' — anche
//! `BlockValue::Null`, che e' un `Ok`, non un errore (un `TextBlock` di
//! `TextFilterPageClassifyStandard` porta sempre quella chiave, valorizzata a `Null` quando
//! nessun blocco pdf era classificato). Il riferimento Python legge il campo con un subscript
//! (`metadata["page_type"]`), che solleva se la chiave manca del tutto — qui l'equivalente e'
//! `metadata_or_fail("page_type")`: una chiave **assente** (non semplicemente valorizzata a
//! `Null`) e' quindi un `Err`, non un `Ok(BlockValue::Null)`.

use crate::core::classes::value::{BlockValue, BlockValueError};
use crate::core::classes::{BlockType, TextBlock};
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::core::schedule::PageClass;
use crate::formats_utils::deserialize::cast::{self, CastError};
use crate::output::classes::OutputClassError;
use crate::output::classes::fund::Fund;
use crate::output::classes::investment::{Bond, Equity, InvestmentFields};

#[derive(Debug, thiserror::Error)]
pub enum DeserializeStandardFuncsError {
    #[error(transparent)]
    Value(#[from] BlockValueError),
    #[error("page_type is a {found}, not a string naming a page class")]
    PageTypeNotAString { found: &'static str },
    /// Un campo obbligatorio dei metadati manca o ha un tipo inutilizzabile.
    #[error("required field '{field}' is missing")]
    MissingField { field: &'static str },
    /// Un campo obbligatorio non si converte: la riga è persa (`LineParseFail` del riferimento).
    #[error("field '{field}': {source}")]
    LineParseFail {
        field: &'static str,
        #[source]
        source: CastError,
    },
    /// Una validazione di dominio di un'entità di output ha rifiutato il valore.
    #[error(transparent)]
    OutputClass(#[from] OutputClassError),
}

impl DeserializeStandardFuncsError {
    /// Traduzione nell'errore del motore. Il nome del pipe non è ricavabile dall'errore, quindi
    /// lo passa il chiamante — stessa forma di [`PipeError::from_commons`].
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            DeserializeStandardFuncsError::Value(source) => PipeError::value(pipe, source),
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

pub struct DeserializerPageClassifyStandard;

impl DeserializerPageClassifyStandard {
    pub fn call(&self, txt_blk: &TextBlock) -> Result<BlockValue, DeserializeStandardFuncsError> {
        Ok(txt_blk.metadata_or_fail("page_type")?.clone())
    }

    /// Il `page_type` letto da [`DeserializerPageClassifyStandard::call`], tradotto nella page
    /// class tipizzata che il motore si aspetta.
    ///
    /// `BlockValue::Null` è la classificazione "nessuna class" — un `Ok(None)`, non un errore:
    /// `TextFilterPageClassifyStandard` mette sempre quella chiave, valorizzata a `Null` quando
    /// nessun blocco della pagina era classificato. Qualunque altro tipo è invece un errore di
    /// configurazione del repo formati.
    pub fn call_page_class(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<PageClass>, DeserializeStandardFuncsError> {
        match self.call(txt_blk)? {
            BlockValue::Null => Ok(None),
            BlockValue::Str(name) => Ok(Some(PageClass::new(name))),
            other => {
                Err(DeserializeStandardFuncsError::PageTypeNotAString { found: other.kind() })
            }
        }
    }
}

impl DeserializePipe for DeserializerPageClassifyStandard {
    fn name(&self) -> &str {
        "DeserializerPageClassifyStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let class = self.call_page_class(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::PageClass(class)])
    }
}

// ---------------------------------------------------------------------------------------------
// DeserializerFundStandard / DeserializerInvestmentStandard (M7, decisione D-M7-2)
// ---------------------------------------------------------------------------------------------

/// Costruisce un [`Fund`] dal contenuto di un blocco di tipo `FUND`.
///
/// Un blocco di tipo diverso non è un errore: il pipe non ha nulla da dire e restituisce una
/// lista vuota. Nel riferimento è il `return None` che i decoratori
/// `deserialize_block_type*` filtrano via.
pub struct DeserializerFundStandard;

impl DeserializerFundStandard {
    pub fn call(&self, txt_blk: &TextBlock) -> Result<Option<Fund>, DeserializeStandardFuncsError> {
        if txt_blk.type_block != BlockType::FUND {
            return Ok(None);
        }
        Ok(Some(Fund::from_value(&txt_blk.content)?))
    }
}

impl DeserializePipe for DeserializerFundStandard {
    fn name(&self) -> &str {
        "DeserializerFundStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let fund = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(fund.map(Extracted::Fund).into_iter().collect())
    }
}

/// Costruisce un [`Equity`] o un [`Bond`] dai metadati di un blocco `EQUITY_TARGET`/`BOND_TARGET`.
///
/// **Due politiche di errore diverse, come nel riferimento.** I campi obbligatori (`company`,
/// `company match`, `fund`, `market value`, `currency`) fanno fallire l'intera riga se non si
/// convertono — è il `LineParseFail` del riferimento. I campi opzionali (`quantity`,
/// `% net assets`, `acquisition cost`, `acquisition currency`) sono invece "provati": se non ci
/// sono, sono `Null`, o non si convertono, il campo resta vuoto e la riga sopravvive, con un
/// `tracing::error!` che lo segnala. È il comportamento del `try_cast` del riferimento, e la
/// ragione per cui una singola cella illeggibile non fa perdere l'intera posizione.
pub struct DeserializerInvestmentStandard {
    cost_and_value_interpret_int: bool,
    quantity_interpret_float: bool,
}

impl Default for DeserializerInvestmentStandard {
    /// Gli stessi default del riferimento: importi interi, quantità intera.
    fn default() -> Self {
        Self { cost_and_value_interpret_int: true, quantity_interpret_float: false }
    }
}

impl DeserializerInvestmentStandard {
    pub fn new(cost_and_value_interpret_int: bool, quantity_interpret_float: bool) -> Self {
        Self { cost_and_value_interpret_int, quantity_interpret_float }
    }

    /// Importi e costi: interi o float a seconda della configurazione del formato.
    fn cast_amount(&self, data: &str) -> Result<f64, CastError> {
        if self.cost_and_value_interpret_int { cast::to_int(data, false).map(|v| v as f64) } else { cast::to_float(data, false) }
    }

    /// Quantità nominale: float o intero a seconda della configurazione del formato.
    fn cast_quantity(&self, data: &str) -> Result<f64, CastError> {
        if self.quantity_interpret_float { cast::to_float(data, false) } else { cast::to_int(data, false).map(|v| v as f64) }
    }

    /// Applica `cast` a un valore obbligatorio, lasciando passare intatta una promessa e
    /// accettando un valore già tipizzato.
    fn required<T>(
        field: &'static str,
        value: Option<&BlockValue>,
        already_typed: impl FnOnce(&BlockValue) -> Option<T>,
        cast: impl FnOnce(&str) -> Result<T, CastError>,
    ) -> Result<BlockValue, DeserializeStandardFuncsError>
    where
        BlockValue: From<T>,
    {
        let value = value.ok_or(DeserializeStandardFuncsError::MissingField { field })?;
        match value {
            BlockValue::Promise(_) => Ok(value.clone()),
            BlockValue::Str(text) => cast(text)
                .map(BlockValue::from)
                .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field, source }),
            other => already_typed(other)
                .map(BlockValue::from)
                .ok_or(DeserializeStandardFuncsError::MissingField { field }),
        }
    }

    /// Come [`Self::required`], ma un fallimento lascia il campo vuoto invece di far fallire la
    /// riga: è il `try_cast` del riferimento.
    fn optional<T>(
        field: &'static str,
        value: Option<&BlockValue>,
        already_typed: impl FnOnce(&BlockValue) -> Option<T>,
        cast: impl FnOnce(&str) -> Result<T, CastError>,
    ) -> Option<BlockValue>
    where
        BlockValue: From<T>,
    {
        match value {
            None | Some(BlockValue::Null) => None,
            Some(promise @ BlockValue::Promise(_)) => Some(promise.clone()),
            Some(BlockValue::Str(text)) => match cast(text) {
                Ok(v) => Some(BlockValue::from(v)),
                Err(err) => {
                    tracing::error!(field, data = text.replace('\n', "\\n"), "Error casting, skipping field: {err}");
                    None
                }
            },
            Some(other) => already_typed(other).map(BlockValue::from),
        }
    }

    pub fn call(&self, txt_blk: &TextBlock) -> Result<Option<Extracted>, DeserializeStandardFuncsError> {
        let is_equity = txt_blk.type_block == BlockType::EQUITY_TARGET;
        let is_bond = txt_blk.type_block == BlockType::BOND_TARGET;
        if !is_equity && !is_bond {
            return Ok(None);
        }
        let md = &txt_blk.metadata;

        let company = cast::to_str(md.get("company").and_then(BlockValue::as_str).unwrap_or_default());
        let company_match = cast::to_str(md.get("company match").and_then(BlockValue::as_str).unwrap_or_default());

        let fields = InvestmentFields {
            company,
            company_match,
            fund: md.get("fund").cloned().ok_or(DeserializeStandardFuncsError::MissingField { field: "fund" })?,
            // `nominal_quantity` è l'unico campo non promissibile dell'entità (`Option<f64>`,
            // non `Option<Promised<f64>>`, come nel riferimento): una promessa qui non ha dove
            // essere conservata e il campo resta vuoto, invece di far fallire la riga come
            // farebbe il `float(to_int(promise))` del riferimento.
            nominal_quantity: Self::optional("quantity", md.get("quantity"), BlockValue::as_float, |t| {
                self.cast_quantity(t)
            })
            .and_then(|v| v.as_float()),
            market_value: Self::required("market value", md.get("market value"), BlockValue::as_float, |t| {
                self.cast_amount(t)
            })?,
            currency: Self::required("currency", md.get("currency"), BlockValue::as_currency, cast::to_currency)?,
            perc_net_assets: Self::optional("% net assets", md.get("% net assets"), BlockValue::as_float, |t| {
                cast::perc_to_float(t, true, false)
            }),
            acquisition_cost: Self::optional("acquisition cost", md.get("acquisition cost"), BlockValue::as_float, |t| {
                self.cast_amount(t)
            }),
            acquisition_currency: Self::optional(
                "acquisition currency",
                md.get("acquisition currency"),
                BlockValue::as_currency,
                cast::to_currency,
            ),
        };

        if is_equity {
            return Ok(Some(Extracted::Equity(Equity::build(fields)?)));
        }
        // Maturity e interest rate seguono la regola del riferimento: si convertono **solo** se la
        // chiave c'è, e un fallimento di conversione fa fallire la riga (non sono `try_cast`).
        let maturity = match md.get("maturity") {
            None | Some(BlockValue::Null) => None,
            Some(BlockValue::Date(date)) => Some(*date),
            Some(value) => Some(
                cast::to_date(value.str_or_fail("maturity")?)
                    .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field: "maturity", source })?,
            ),
        };
        let interest_rate = match md.get("interest rate") {
            None | Some(BlockValue::Null) => None,
            Some(BlockValue::Float(rate)) => Some(rate.into_inner()),
            Some(value) => Some(cast::perc_to_float(value.str_or_fail("interest rate")?, true, false).map_err(
                |source| DeserializeStandardFuncsError::LineParseFail { field: "interest rate", source },
            )?),
        };
        Ok(Some(Extracted::Bond(Bond::build(fields, maturity, interest_rate)?)))
    }
}

impl DeserializePipe for DeserializerInvestmentStandard {
    fn name(&self) -> &str {
        "DeserializerInvestmentStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let extracted = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(extracted.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::value::BlockValue;
    use crate::core::classes::{BlockType, TextBlock};
    use std::collections::BTreeMap;

    mod deserializer_fund {
        use super::*;
        use crate::core::promise::Promise;

        fn fund_block(content: BlockValue) -> TextBlock {
            TextBlock::from_content(BlockType::FUND, BTreeMap::new(), content)
        }

        #[test]
        fn builds_a_fund_from_the_block_content() {
            let fund = DeserializerFundStandard.call(&fund_block(BlockValue::from("Alpha Fund"))).unwrap();
            assert_eq!(fund.unwrap().name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn a_block_of_another_type_is_skipped_rather_than_rejected() {
            let other = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "whatever");
            assert!(DeserializerFundStandard.call(&other).unwrap().is_none());
        }

        #[test]
        fn a_skipped_block_produces_no_extracted_result() {
            let other = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "whatever");
            assert!(DeserializerFundStandard.deserialize(&other).unwrap().is_empty());
        }

        #[test]
        fn a_matching_block_produces_exactly_one_fund_result() {
            let out = DeserializerFundStandard.deserialize(&fund_block(BlockValue::from("Alpha"))).unwrap();
            assert_eq!(out.len(), 1);
            assert!(out[0].as_fund().is_some());
        }

        #[test]
        fn a_promised_name_is_carried_through_unresolved() {
            let block = fund_block(BlockValue::Promise(Promise::new("fund-id")));
            let fund = DeserializerFundStandard.call(&block).unwrap().unwrap();
            assert!(fund.pending_name().is_some());
        }

        #[test]
        fn a_non_string_content_fails_the_pipe() {
            assert!(DeserializerFundStandard.call(&fund_block(BlockValue::from(1i64))).is_err());
        }
    }

    mod deserializer_investment {
        use super::*;
        use crate::commons::consts::Currency;
        use crate::commons::date::Date;
        use crate::core::promise::Promise;

        fn base_metadata() -> BTreeMap<String, BlockValue> {
            BTreeMap::from([
                ("company".to_string(), BlockValue::from("Acme Corp")),
                ("company match".to_string(), BlockValue::from("Acme")),
                ("fund".to_string(), BlockValue::from("Alpha Fund")),
                ("market value".to_string(), BlockValue::from("1.000")),
                ("currency".to_string(), BlockValue::from(Currency::EUR)),
            ])
        }

        fn block(type_block: BlockType, metadata: BTreeMap<String, BlockValue>) -> TextBlock {
            TextBlock::from_content(type_block, metadata, "")
        }

        fn equity_block(metadata: BTreeMap<String, BlockValue>) -> TextBlock {
            block(BlockType::EQUITY_TARGET, metadata)
        }

        mod dispatch {
            use super::*;

            #[test]
            fn an_equity_target_block_becomes_an_equity() {
                let out = DeserializerInvestmentStandard::default().call(&equity_block(base_metadata())).unwrap();
                assert!(out.unwrap().as_equity().is_some());
            }

            #[test]
            fn a_bond_target_block_becomes_a_bond() {
                let out = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, base_metadata()))
                    .unwrap();
                assert!(out.unwrap().as_bond().is_some());
            }

            #[test]
            fn any_other_block_type_is_skipped() {
                let out = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::FUND, base_metadata()))
                    .unwrap();
                assert!(out.is_none());
            }

            #[test]
            fn a_skipped_block_produces_no_extracted_result() {
                let out = DeserializerInvestmentStandard::default()
                    .deserialize(&block(BlockType::FUND, base_metadata()))
                    .unwrap();
                assert!(out.is_empty());
            }
        }

        mod required_fields {
            use super::*;

            #[test]
            fn the_company_and_its_match_are_normalized_strings() {
                let mut md = base_metadata();
                md.insert("company".to_string(), BlockValue::from("  Acme   Corp  "));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(extracted.as_equity().unwrap().data.company, "Acme Corp");
            }

            #[test]
            fn the_market_value_is_cast_as_an_integer_by_default() {
                let extracted =
                    DeserializerInvestmentStandard::default().call(&equity_block(base_metadata())).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.market_value.resolved().map(|v| v.into_inner());
                assert_eq!(value, Some(1000.0));
            }

            #[test]
            fn the_market_value_is_cast_as_a_float_when_configured_so() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("1.000,50"));
                let deserializer = DeserializerInvestmentStandard::new(false, false);
                let extracted = deserializer.call(&equity_block(md)).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.market_value.resolved().map(|v| v.into_inner());
                assert_eq!(value, Some(1000.5));
            }

            #[test]
            fn an_unreadable_market_value_loses_the_whole_line() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("not a number"));
                let err = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::LineParseFail { field: "market value", .. }));
            }

            #[test]
            fn a_missing_market_value_key_loses_the_whole_line() {
                let mut md = base_metadata();
                md.remove("market value");
                let err = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::MissingField { field: "market value" }));
            }

            #[test]
            fn a_currency_written_as_text_is_cast() {
                let mut md = base_metadata();
                md.insert("currency".to_string(), BlockValue::from("usd"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(extracted.as_equity().unwrap().data.currency.resolved(), Some(&Currency::USD));
            }

            #[test]
            fn an_unknown_currency_loses_the_whole_line() {
                let mut md = base_metadata();
                md.insert("currency".to_string(), BlockValue::from("XYZ"));
                assert!(DeserializerInvestmentStandard::default().call(&equity_block(md)).is_err());
            }

            #[test]
            fn a_promised_fund_stays_pending() {
                let mut md = base_metadata();
                md.insert("fund".to_string(), BlockValue::Promise(Promise::new("fund-id")));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.fund.is_pending());
            }

            #[test]
            fn a_null_fund_loses_the_whole_line_like_in_the_reference() {
                let mut md = base_metadata();
                md.insert("fund".to_string(), BlockValue::Null);
                assert!(DeserializerInvestmentStandard::default().call(&equity_block(md)).is_err());
            }
        }

        mod optional_fields {
            use super::*;

            #[test]
            fn an_absent_optional_field_simply_stays_empty() {
                let extracted =
                    DeserializerInvestmentStandard::default().call(&equity_block(base_metadata())).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.perc_net_assets.is_none());
            }

            #[test]
            fn a_null_optional_field_stays_empty_too() {
                let mut md = base_metadata();
                md.insert("% net assets".to_string(), BlockValue::Null);
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.perc_net_assets.is_none());
            }

            #[test]
            fn a_percentage_is_normalized_to_a_fraction() {
                let mut md = base_metadata();
                md.insert("% net assets".to_string(), BlockValue::from("12,5 %"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.perc_net_assets.as_ref().and_then(|p| p.resolved());
                assert_eq!(value.map(|v| v.into_inner()), Some(0.125));
            }

            #[test]
            fn an_unreadable_optional_field_is_skipped_and_the_line_survives() {
                let mut md = base_metadata();
                md.insert("acquisition cost".to_string(), BlockValue::from("garbage"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.acquisition_cost.is_none());
            }

            #[test]
            fn the_quantity_is_an_integer_by_default() {
                let mut md = base_metadata();
                md.insert("quantity".to_string(), BlockValue::from("1.042"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(
                    extracted.as_equity().unwrap().data.nominal_quantity.map(|v| v.into_inner()),
                    Some(1042.0)
                );
            }

            #[test]
            fn a_fractional_quantity_is_skipped_when_the_format_declares_it_integer() {
                // `to_int` rifiuta una mantissa non nulla; essendo un campo opzionale, la riga
                // sopravvive e la quantità resta vuota — esattamente come il `try_cast` del
                // riferimento, che cattura il `ValueError` di `float(to_int(...))`.
                let mut md = base_metadata();
                md.insert("quantity".to_string(), BlockValue::from("42,7"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.nominal_quantity.is_none());
            }

            #[test]
            fn the_quantity_keeps_its_decimals_when_configured_so() {
                let mut md = base_metadata();
                md.insert("quantity".to_string(), BlockValue::from("42,7"));
                let extracted =
                    DeserializerInvestmentStandard::new(true, true).call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(
                    extracted.as_equity().unwrap().data.nominal_quantity.map(|v| v.into_inner()),
                    Some(42.7)
                );
            }

            #[test]
            fn an_acquisition_currency_written_as_text_is_cast() {
                let mut md = base_metadata();
                md.insert("acquisition currency".to_string(), BlockValue::from("GBP"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.acquisition_currency.as_ref().and_then(|p| p.resolved());
                assert_eq!(value, Some(&Currency::GBP));
            }
        }

        mod bond_specific_fields {
            use super::*;

            #[test]
            fn the_maturity_is_cast_from_text() {
                let mut md = base_metadata();
                md.insert("maturity".to_string(), BlockValue::from("28/03/2025"));
                let extracted = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, md))
                    .unwrap()
                    .unwrap();
                assert_eq!(extracted.as_bond().unwrap().maturity, Some(Date::new(2025, 3, 28).unwrap()));
            }

            #[test]
            fn the_interest_rate_is_normalized_to_a_fraction() {
                let mut md = base_metadata();
                md.insert("interest rate".to_string(), BlockValue::from("3,5 %"));
                let extracted = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, md))
                    .unwrap()
                    .unwrap();
                assert_eq!(extracted.as_bond().unwrap().interest_rate.map(|v| v.into_inner()), Some(0.035));
            }

            #[test]
            fn a_bond_without_those_keys_gets_none_for_both() {
                let extracted = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, base_metadata()))
                    .unwrap()
                    .unwrap();
                let bond = extracted.as_bond().unwrap();
                assert!(bond.maturity.is_none() && bond.interest_rate.is_none());
            }

            #[test]
            fn an_unreadable_maturity_loses_the_whole_line_unlike_an_optional_field() {
                let mut md = base_metadata();
                md.insert("maturity".to_string(), BlockValue::from("not a date"));
                let err = DeserializerInvestmentStandard::default().call(&block(BlockType::BOND_TARGET, md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::LineParseFail { field: "maturity", .. }));
            }

            #[test]
            fn an_out_of_range_interest_rate_is_rejected_by_the_entity() {
                let mut md = base_metadata();
                md.insert("interest rate".to_string(), BlockValue::from("150 %"));
                let err = DeserializerInvestmentStandard::default().call(&block(BlockType::BOND_TARGET, md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::OutputClass(_)));
            }
        }

        mod as_a_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerInvestmentStandard::default().name(), "DeserializerInvestmentStandard");
            }

            #[test]
            fn a_line_parse_failure_is_a_fatal_pipe_error_not_a_skipped_page() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("nope"));
                let err = DeserializerInvestmentStandard::default().deserialize(&equity_block(md)).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    mod deserializer_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_a_present_page_type() {
            let mut metadata = BTreeMap::new();
            metadata.insert("page_type".to_string(), BlockValue::Str("investments".to_string()));
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "");

            let result = DeserializerPageClassifyStandard.call(&txt).unwrap();
            assert_eq!(result, BlockValue::Str("investments".to_string()));
        }

        #[test]
        fn a_present_but_null_page_type_is_ok_not_an_error() {
            let mut metadata = BTreeMap::new();
            metadata.insert("page_type".to_string(), BlockValue::Null);
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "");

            let result = DeserializerPageClassifyStandard.call(&txt).unwrap();
            assert_eq!(result, BlockValue::Null);
        }

        #[test]
        fn a_missing_page_type_key_is_an_error() {
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "");
            assert!(DeserializerPageClassifyStandard.call(&txt).is_err());
        }
    }

    /// M5: lo stesso pipe visto come [`DeserializePipe`], cioè come il motore lo usa.
    mod as_a_deserialize_pipe {
        use super::*;
        use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
        use crate::core::schedule::PageClass;
        use pretty_assertions::assert_eq;

        fn block_with(page_type: BlockValue) -> TextBlock {
            let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
            TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "")
        }

        #[test]
        fn a_string_page_type_becomes_a_page_class() {
            let out = DeserializerPageClassifyStandard
                .deserialize(&block_with(BlockValue::from("investments")))
                .unwrap();
            assert_eq!(out, vec![Extracted::PageClass(Some(PageClass::new("investments")))]);
        }

        #[test]
        fn a_null_page_type_becomes_an_explicitly_unclassified_page() {
            let out =
                DeserializerPageClassifyStandard.deserialize(&block_with(BlockValue::Null)).unwrap();
            assert_eq!(out, vec![Extracted::PageClass(None)]);
        }

        #[test]
        fn a_page_type_of_the_wrong_type_is_a_pipe_error_naming_the_pipe() {
            let err = DeserializerPageClassifyStandard
                .deserialize(&block_with(BlockValue::from(3i64)))
                .unwrap_err();
            assert_eq!(err.pipe(), "DeserializerPageClassifyStandard");
            assert!(matches!(err, PipeError::Extraction { .. }));
        }

        #[test]
        fn a_missing_page_type_key_is_a_value_error_not_a_page_failure() {
            let block = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "");
            let err = DeserializerPageClassifyStandard.deserialize(&block).unwrap_err();
            assert!(matches!(err, PipeError::Value { .. }));
            assert!(!err.is_page_failure());
        }
    }
}
