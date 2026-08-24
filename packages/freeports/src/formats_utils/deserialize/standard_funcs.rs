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
//!
//! ---
//!
//! **M8 (`agent-memory/M8-implementation-plan.md` §2/§3, passo 8): le cinque funzioni restanti.**
//! `output::classes` esiste ora, quindi queste sono costruibili — chiude anche M4.
//!
//! ```text
//! pub struct DeserializeSfdrArticleStandard;
//! impl DeserializeSfdrArticleStandard {
//!     // Non filtra per `type_block` (come il riferimento): costruisce sempre.
//!     pub fn call(&self, txt_blk: &TextBlock) -> Result<FundSfdrClassification, DeserializeStandardFuncsError>;
//! }
//!
//! pub struct DeserializerManagmentCompanyStandard;      // legge MANAGEMENT_COMPANY -> ManagementCompany
//! pub struct DeserializerInvestmentsManagerFromManco;   // legge MANAGEMENT_COMPANY -> InvestmentsManager
//! pub struct DeserializerInvestmentsManagerStandard;    // legge INVESTMENTS_MANAGER -> InvestmentsManager
//! // Le tre condividono un helper privato (`fn build_manager<T>(txt_blk, expected_type, ctor)`):
//! // se `txt_blk.type_block != expected_type`, `Ok(None)`; altrimenti il `content` viene
//! // collassato (`split_whitespace().collect::<Vec<_>>().join(" ")`, non un `.trim()`) e passato
//! // insieme a `metadata["managed_funds"]` al costruttore dell'entità.
//!
//! pub struct DeserializerAssetsStandard { /* interpret_int: bool, date_converter: fn(&str) -> Result<Date, CastError> */ }
//! impl Default for DeserializerAssetsStandard { /* interpret_int = true, date_converter = cast::to_date */ }
//! impl DeserializerAssetsStandard {
//!     pub fn new(interpret_int: bool, date_converter: fn(&str) -> Result<Date, CastError>) -> Self;
//!     pub fn call(&self, txt_blk: &TextBlock) -> Result<FundAssets, DeserializeStandardFuncsError>;
//! }
//! ```
//!
//! **`DeserializerAssetsStandard` — dettagli del contratto:**
//! - `fund`/`currency`/`tot_assets`/`net_assets`/`liabilities` sono obbligatori: chiave assente
//!   (o `Null`) -> `MissingField`.
//! - `currency`/`tot_assets`/`net_assets`/`liabilities` accettano **sia** un `BlockValue` già
//!   tipizzato (`Currency`/`Int`/`Float`) **sia** una stringa da convertire (`cast::to_currency`,
//!   `cast::to_int`/`cast::to_float` a seconda di `interpret_int`) — stesso pattern duale già
//!   usato da `DeserializerInvestmentStandard::required` in questo file, non un'invenzione nuova:
//!   `TextFilterAssetsStandard` (M8, `text_filter::standard_funcs`) produce già `currency` come
//!   `BlockValue::Currency`, quindi il percorso "già tipizzato" è quello davvero esercitato dalla
//!   pipeline reale, il percorso stringa resta per fixture costruite a mano/altri formati.
//! - **Quirk verbatim da preservare**: le parentesi vengono tolte dal testo di `liabilities`
//!   *prima* della conversione — `"(200)"` diventa `200.0`, non `-200.0`.
//! - `date` è opzionale: assente o `Null` -> `None`; una stringa viene passata a
//!   `date_converter`; un fallimento di conversione (quando la chiave c'è) fa perdere l'intera
//!   riga (`LineParseFail`), non è un campo "provato" come in `DeserializerInvestmentStandard`.
//! - **Giudizio non pienamente verificato, segnalato nel report**: il default proposto da
//!   `agent-memory/M8-implementation-plan.md` §2 per `Default` (`interpret_int = true`) non è
//!   mai stato verificato contro un formato reale in `analysis_finance_reports_formats` — è
//!   un'estrapolazione dal default di `DeserializerInvestmentStandard`, non un fatto accertato.

use crate::core::classes::value::{BlockValue, BlockValueError};
use crate::core::classes::{BlockType, TextBlock};
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::core::schedule::PageClass;
use crate::formats_utils::deserialize::cast::{self, CastError};
use crate::output::classes::OutputClassError;
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::classes::fund::Fund;
use crate::output::classes::fund_assets::FundAssets;
use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
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
            Some(BlockValue::Str(text)) => {
                // Lo span porta il nome del campo nella colonna `Field name` del `.log.csv` anche
                // sugli eventi che nascono **dentro** `cast` (per esempio il `forcing cast` di
                // `deserialize::cast`), che il nome del campo non lo conoscono.
                let field_span = tracing::info_span!("field", field);
                let _field_guard = field_span.enter();
                match cast(text) {
                    Ok(v) => Some(BlockValue::from(v)),
                    Err(err) => {
                        // Una riga sola che dice le tre cose che il riferimento spalmava su tre:
                        // il dato che non si converte, perché, e che il campo viene perso.
                        let data = text.replace('\n', "\\n");
                        tracing::error!("could not cast {data:?}: {err} - field skipped");
                        None
                    }
                }
            }
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

        // Le due colonne "societa'" del `.log.csv` vengono da qui. Sono messe su uno span e non
        // sui singoli eventi perche' devono valere per *tutto* cio' che la deserializzazione di
        // questa riga produce, compresi gli eventi che nascono dentro `deserialize::cast` e che
        // non hanno modo di conoscerle. Attenzione ai nomi: `company` e' il bersaglio
        // riconosciuto e finisce nella colonna `Matched Company`, `company_match` e' il testo
        // grezzo della riga e finisce in `Company` -- inversione ereditata dal riferimento, vedi
        // il doc-comment di `core::tracing_setup`.
        let row_span = tracing::info_span!(
            "investment",
            company = %company,
            company_match = %company_match,
        );
        let _row_guard = row_span.enter();

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

// ---------------------------------------------------------------------------------------------
// M8 (`agent-memory/M8-implementation-plan.md` §2/§3, passo 8): le cinque funzioni restanti.
// ---------------------------------------------------------------------------------------------

/// Costruisce una [`FundSfdrClassification`] da un blocco `SFDR_ARTICLE`. Non filtra per
/// `type_block` (come il riferimento): costruisce sempre, da `content` (nome del fondo) e
/// `metadata["article"]` (obbligatorio).
pub struct DeserializeSfdrArticleStandard;

impl DeserializeSfdrArticleStandard {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<FundSfdrClassification, DeserializeStandardFuncsError> {
        let fund = txt_blk.content.str_or_fail("content")?;
        let article = txt_blk.metadata_or_fail("article")?;
        Ok(FundSfdrClassification::build(fund, article)?)
    }
}

impl DeserializePipe for DeserializeSfdrArticleStandard {
    fn name(&self) -> &str {
        "DeserializeSfdrArticleStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let classification = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::FundSfdrClassification(classification)])
    }
}

/// Il corpo condiviso dai tre deserializzatori "manager" (`agent-memory/
/// M8-implementation-plan.md` §2): se `txt_blk.type_block != expected_type`, `Ok(None)` — il pipe
/// non ha nulla da dire; altrimenti il `content` viene collassato (`split_whitespace().join(" ")`,
/// non un `.trim()` semplice) e passato insieme a `metadata["managed_funds"]` al costruttore
/// dell'entità.
fn build_manager<T>(
    txt_blk: &TextBlock,
    expected_type: &crate::core::classes::BlockType,
    ctor: impl Fn(&BlockValue, &BlockValue) -> Result<T, OutputClassError>,
) -> Result<Option<T>, DeserializeStandardFuncsError> {
    if &txt_blk.type_block != expected_type {
        return Ok(None);
    }
    let name = txt_blk.content.str_or_fail("content")?;
    let normalized_name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let managed_funds = txt_blk.metadata_or_fail("managed_funds")?;
    Ok(Some(ctor(&BlockValue::from(normalized_name), managed_funds)?))
}

/// Legge un blocco `MANAGEMENT_COMPANY` e costruisce una [`ManagementCompany`].
pub struct DeserializerManagmentCompanyStandard;

impl DeserializerManagmentCompanyStandard {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<ManagementCompany>, DeserializeStandardFuncsError> {
        build_manager(txt_blk, &BlockType::MANAGEMENT_COMPANY, ManagementCompany::build)
    }
}

impl DeserializePipe for DeserializerManagmentCompanyStandard {
    fn name(&self) -> &str {
        "DeserializerManagmentCompanyStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let manager = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(manager.map(Extracted::ManagementCompany).into_iter().collect())
    }
}

/// Legge lo **stesso** blocco `MANAGEMENT_COMPANY`, ma costruisce un [`InvestmentsManager`]: due
/// formati diversi usano l'uno o l'altro deserializzatore sullo stesso tipo di blocco, mai
/// insieme sulla stessa pipeline.
pub struct DeserializerInvestmentsManagerFromManco;

impl DeserializerInvestmentsManagerFromManco {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<InvestmentsManager>, DeserializeStandardFuncsError> {
        build_manager(txt_blk, &BlockType::MANAGEMENT_COMPANY, InvestmentsManager::build)
    }
}

impl DeserializePipe for DeserializerInvestmentsManagerFromManco {
    fn name(&self) -> &str {
        "DeserializerInvestmentsManagerFromManco"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let manager = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(manager.map(Extracted::InvestmentsManager).into_iter().collect())
    }
}

/// Come [`DeserializerInvestmentsManagerFromManco`], ma legge da un blocco `INVESTMENTS_MANAGER`
/// (non `MANAGEMENT_COMPANY`).
pub struct DeserializerInvestmentsManagerStandard;

impl DeserializerInvestmentsManagerStandard {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<InvestmentsManager>, DeserializeStandardFuncsError> {
        build_manager(txt_blk, &BlockType::INVESTMENTS_MANAGER, InvestmentsManager::build)
    }
}

impl DeserializePipe for DeserializerInvestmentsManagerStandard {
    fn name(&self) -> &str {
        "DeserializerInvestmentsManagerStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let manager = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(manager.map(Extracted::InvestmentsManager).into_iter().collect())
    }
}

/// Il convertitore d'importo di [`DeserializerAssetsStandard`]: una funzione condivisibile, non un
/// flag, perché nel riferimento è un callable Python qualunque.
pub type NumConverter = std::sync::Arc<dyn Fn(&str) -> Result<f64, CastError> + Send + Sync>;

/// Il convertitore di date di [`DeserializerAssetsStandard`], nella stessa forma condivisibile di
/// [`NumConverter`].
pub type DateConverter =
    std::sync::Arc<dyn Fn(&str) -> Result<crate::commons::date::Date, CastError> + Send + Sync>;

/// Costruisce un [`FundAssets`] da un blocco `RELEVANT_BLOCK` prodotto da
/// `TextFilterAssetsStandard`.
///
/// `num_converter` è configurabile ([`DeserializerAssetsStandard::new`] sceglie con
/// `interpret_int` fra `cast::to_int`/`cast::to_float`, stesso pattern di
/// `DeserializerInvestmentStandard`); `date_converter` è di default [`cast::to_date`] ma
/// sostituibile. Entrambi sono convertitori arbitrari, non un flag: vedi
/// [`DeserializerAssetsStandard::with_converters`].
pub struct DeserializerAssetsStandard {
    num_converter: NumConverter,
    date_converter: DateConverter,
}

impl Default for DeserializerAssetsStandard {
    /// **Giudizio non pienamente verificato** (vedi il doc-comment di modulo): `interpret_int =
    /// true` è un'estrapolazione dal default di `DeserializerInvestmentStandard`, non un fatto
    /// verificato contro un formato reale.
    fn default() -> Self {
        Self::new(true, cast::to_date)
    }
}

impl DeserializerAssetsStandard {
    pub fn new(
        interpret_int: bool,
        date_converter: fn(&str) -> Result<crate::commons::date::Date, CastError>,
    ) -> Self {
        Self::with_converters(Self::builtin_num_converter(interpret_int), std::sync::Arc::new(date_converter))
    }

    /// Il convertitore d'importo che `interpret_int` seleziona: `cast::to_int` o `cast::to_float`.
    pub fn builtin_num_converter(interpret_int: bool) -> NumConverter {
        if interpret_int {
            std::sync::Arc::new(|text: &str| cast::to_int(text, false).map(|v| v as f64))
        } else {
            std::sync::Arc::new(|text: &str| cast::to_float(text, false))
        }
    }

    /// Costruisce il pipe con convertitori arbitrari, non solo con quelli predefiniti.
    ///
    /// Esiste perché nel riferimento `num_converter` e `date_converter` sono *callable* Python
    /// qualunque, e i moduli d'autore ne approfittano davvero (per esempio
    /// `num_converter=lambda txt: 0 if txt == "-" else to_int(txt)`): un `bool` non basta a
    /// rappresentarli. [`Self::new`] resta la firma comoda per i due casi predefiniti.
    pub fn with_converters(num_converter: NumConverter, date_converter: DateConverter) -> Self {
        Self { num_converter, date_converter }
    }

    /// Converte importi con `num_converter`: già tipizzato è accettato direttamente, una stringa
    /// passa per il convertitore configurato — stesso pattern duale di
    /// `DeserializerInvestmentStandard::required`.
    fn cast_amount(&self, field: &'static str, value: &BlockValue) -> Result<f64, DeserializeStandardFuncsError> {
        match value {
            BlockValue::Str(text) => (self.num_converter)(text)
                .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field, source }),
            other => other
                .as_float()
                .or_else(|| other.as_int().map(|v| v as f64))
                .ok_or(DeserializeStandardFuncsError::MissingField { field }),
        }
    }

    /// Legge un campo obbligatorio dei metadati, riportando `MissingField` se assente o `Null`.
    fn required_field<'a>(
        md: &'a std::collections::BTreeMap<String, BlockValue>,
        field: &'static str,
    ) -> Result<&'a BlockValue, DeserializeStandardFuncsError> {
        match md.get(field) {
            None | Some(BlockValue::Null) => Err(DeserializeStandardFuncsError::MissingField { field }),
            Some(value) => Ok(value),
        }
    }

    pub fn call(&self, txt_blk: &TextBlock) -> Result<FundAssets, DeserializeStandardFuncsError> {
        let md = &txt_blk.metadata;

        let fund = Self::required_field(md, "fund")?.str_or_fail("fund")?.to_string();
        let currency = match Self::required_field(md, "currency")? {
            BlockValue::Currency(c) => BlockValue::Currency(*c),
            BlockValue::Str(text) => BlockValue::from(
                cast::to_currency(text)
                    .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field: "currency", source })?,
            ),
            _ => return Err(DeserializeStandardFuncsError::MissingField { field: "currency" }),
        };

        let tot_assets = self.cast_amount("tot_assets", Self::required_field(md, "tot_assets")?)?;
        let net_assets = self.cast_amount("net_assets", Self::required_field(md, "net_assets")?)?;

        // Stranezza da preservare verbatim: le parentesi vengono tolte dal testo *prima* della
        // conversione, quindi "(200)" (convenzione contabile per negativo) diventa 200.0, non
        // -200.0 — non è un bug da correggere, è il comportamento del riferimento.
        let liabilities_raw = Self::required_field(md, "liabilities")?;
        let liabilities = match liabilities_raw {
            BlockValue::Str(text) => {
                let cleaned = text.replace(['(', ')'], "");
                self.cast_amount("liabilities", &BlockValue::Str(cleaned))?
            }
            other => self.cast_amount("liabilities", other)?,
        };

        let date = match md.get("date") {
            None | Some(BlockValue::Null) => None,
            Some(BlockValue::Date(d)) => Some(BlockValue::Date(*d)),
            Some(value) => {
                let text = value.str_or_fail("date")?;
                Some(BlockValue::from(
                    (self.date_converter)(text)
                        .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field: "date", source })?,
                ))
            }
        };

        Ok(FundAssets::build(fund, tot_assets, liabilities, net_assets, &currency, date.as_ref())?)
    }
}

impl DeserializePipe for DeserializerAssetsStandard {
    fn name(&self) -> &str {
        "DeserializerAssetsStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let assets = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::FundAssets(assets)])
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

    // -----------------------------------------------------------------------------------------
    // M8: le cinque funzioni deferite (`agent-memory/M8-implementation-plan.md` §2, passo 8),
    // costruibili ora che `output::classes` esiste. `DeserializeSfdrArticleStandard` non filtra
    // per `type_block` (come nel riferimento); le tre funzioni "manager" lo fanno, e condividono
    // un helper privato comune (`build_manager`, vedi §2).
    // -----------------------------------------------------------------------------------------

    mod deserialize_sfdr_article {
        use super::*;
        use crate::commons::consts::SfdrArticle;

        fn sfdr_block(content: BlockValue, article: Option<BlockValue>) -> TextBlock {
            let metadata = article.map(|a| BTreeMap::from([("article".to_string(), a)])).unwrap_or_default();
            TextBlock::from_content(BlockType::SFDR_ARTICLE, metadata, content)
        }

        #[test]
        fn builds_a_classification_from_content_and_the_article_metadata() {
            let blk = sfdr_block(BlockValue::from("Alpha Fund"), Some(BlockValue::from(SfdrArticle::Art8)));
            let classification = DeserializeSfdrArticleStandard.call(&blk).unwrap();
            assert_eq!(classification.fund, "Alpha Fund");
            assert_eq!(classification.article.resolved(), Some(&SfdrArticle::Art8));
        }

        #[test]
        fn a_missing_article_key_is_an_error() {
            let blk = sfdr_block(BlockValue::from("Alpha Fund"), None);
            assert!(DeserializeSfdrArticleStandard.call(&blk).is_err());
        }

        #[test]
        fn a_non_string_content_is_an_error() {
            let blk = sfdr_block(BlockValue::from(1i64), Some(BlockValue::from(SfdrArticle::Art6)));
            assert!(DeserializeSfdrArticleStandard.call(&blk).is_err());
        }

        #[test]
        fn a_wrongly_typed_article_is_an_error() {
            let blk = sfdr_block(BlockValue::from("Alpha Fund"), Some(BlockValue::from("Art. 8")));
            assert!(DeserializeSfdrArticleStandard.call(&blk).is_err());
        }

        mod as_a_deserialize_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializeSfdrArticleStandard.name(), "DeserializeSfdrArticleStandard");
            }

            #[test]
            fn a_matching_block_produces_exactly_one_classification_result() {
                let blk = sfdr_block(BlockValue::from("Alpha Fund"), Some(BlockValue::from(SfdrArticle::Art9)));
                let out = DeserializeSfdrArticleStandard.deserialize(&blk).unwrap();
                assert_eq!(out.len(), 1);
                assert!(out[0].as_fund_sfdr_classification().is_some());
            }

            #[test]
            fn a_missing_article_is_a_value_error_not_a_page_failure() {
                let blk = sfdr_block(BlockValue::from("Alpha Fund"), None);
                let err = DeserializeSfdrArticleStandard.deserialize(&blk).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    /// I tre deserializzatori "manager" condividono lo stesso corpo (helper `build_manager`), a
    /// meno del tipo di blocco atteso e dell'entità costruita — vedi il doc-comment di modulo.
    mod manager_deserializers {
        use super::*;
        use std::collections::BTreeSet;

        fn managed_funds_value(names: &[&str]) -> BlockValue {
            BlockValue::Set(names.iter().map(|n| BlockValue::from(*n)).collect())
        }

        fn manager_block(
            type_block: BlockType,
            content: &str,
            managed_funds: Option<BlockValue>,
        ) -> TextBlock {
            let metadata = managed_funds
                .map(|f| BTreeMap::from([("managed_funds".to_string(), f)]))
                .unwrap_or_default();
            TextBlock::from_content(type_block, metadata, content)
        }

        mod management_company {
            use super::*;

            #[test]
            fn builds_a_management_company_from_a_matching_block() {
                let blk = manager_block(
                    BlockType::MANAGEMENT_COMPANY,
                    "  Acme   Manager  \n",
                    Some(managed_funds_value(&["Fund A", "Fund B"])),
                );
                let manager = DeserializerManagmentCompanyStandard.call(&blk).unwrap().unwrap();
                assert_eq!(manager.data.name, "Acme Manager");
                assert_eq!(
                    manager.data.managed_funds,
                    BTreeSet::from(["Fund A".to_string(), "Fund B".to_string()])
                );
            }

            #[test]
            fn a_block_of_another_type_is_skipped_rather_than_rejected() {
                let blk = manager_block(BlockType::INVESTMENTS_MANAGER, "Acme", Some(managed_funds_value(&[])));
                assert!(DeserializerManagmentCompanyStandard.call(&blk).unwrap().is_none());
            }

            #[test]
            fn a_skipped_block_produces_no_extracted_result() {
                let blk = manager_block(BlockType::INVESTMENTS_MANAGER, "Acme", Some(managed_funds_value(&[])));
                assert!(DeserializerManagmentCompanyStandard.deserialize(&blk).unwrap().is_empty());
            }

            #[test]
            fn a_missing_managed_funds_key_is_an_error() {
                let blk = manager_block(BlockType::MANAGEMENT_COMPANY, "Acme", None);
                assert!(DeserializerManagmentCompanyStandard.call(&blk).is_err());
            }

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerManagmentCompanyStandard.name(), "DeserializerManagmentCompanyStandard");
            }
        }

        /// Stesso tipo di blocco di `DeserializerManagmentCompanyStandard` (`MANAGEMENT_COMPANY`),
        /// ma costruisce un `InvestmentsManager`.
        mod investments_manager_from_manco {
            use super::*;

            #[test]
            fn builds_an_investments_manager_from_a_management_company_block() {
                let blk = manager_block(
                    BlockType::MANAGEMENT_COMPANY,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                let manager = DeserializerInvestmentsManagerFromManco.call(&blk).unwrap().unwrap();
                assert_eq!(manager.data.name, "Acme Manager");
            }

            #[test]
            fn an_investments_manager_typed_block_is_skipped() {
                let blk = manager_block(
                    BlockType::INVESTMENTS_MANAGER,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                assert!(DeserializerInvestmentsManagerFromManco.call(&blk).unwrap().is_none());
            }

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(
                    DeserializerInvestmentsManagerFromManco.name(),
                    "DeserializerInvestmentsManagerFromManco"
                );
            }
        }

        /// Come sopra, ma legge da un blocco `INVESTMENTS_MANAGER` (non `MANAGEMENT_COMPANY`).
        mod investments_manager_standard {
            use super::*;

            #[test]
            fn builds_an_investments_manager_from_an_investments_manager_block() {
                let blk = manager_block(
                    BlockType::INVESTMENTS_MANAGER,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                let manager = DeserializerInvestmentsManagerStandard.call(&blk).unwrap().unwrap();
                assert_eq!(manager.data.name, "Acme Manager");
            }

            #[test]
            fn a_management_company_typed_block_is_skipped() {
                let blk = manager_block(
                    BlockType::MANAGEMENT_COMPANY,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                assert!(DeserializerInvestmentsManagerStandard.call(&blk).unwrap().is_none());
            }

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerInvestmentsManagerStandard.name(), "DeserializerInvestmentsManagerStandard");
            }

            #[test]
            fn a_matching_block_produces_exactly_one_extracted_result() {
                let blk = manager_block(
                    BlockType::INVESTMENTS_MANAGER,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                let out = DeserializerInvestmentsManagerStandard.deserialize(&blk).unwrap();
                assert_eq!(out.len(), 1);
                assert!(out[0].as_investments_manager().is_some());
            }
        }
    }

    mod deserializer_assets {
        use super::*;
        use crate::commons::consts::Currency;
        use crate::commons::date::Date;

        fn assets_metadata(overrides: &[(&str, BlockValue)]) -> BTreeMap<String, BlockValue> {
            let mut md = BTreeMap::from([
                ("fund".to_string(), BlockValue::from("Alpha Fund")),
                ("currency".to_string(), BlockValue::from("EUR")),
                ("tot_assets".to_string(), BlockValue::from("1000")),
                ("net_assets".to_string(), BlockValue::from("800")),
                ("liabilities".to_string(), BlockValue::from("200")),
            ]);
            for (k, v) in overrides {
                md.insert((*k).to_string(), v.clone());
            }
            md
        }

        fn assets_block(metadata: BTreeMap<String, BlockValue>) -> TextBlock {
            TextBlock::from_content(BlockType::RELEVANT_BLOCK, metadata, "")
        }

        mod required_fields {
            use super::*;

            #[test]
            fn builds_fund_assets_from_a_fully_populated_block() {
                let assets =
                    DeserializerAssetsStandard::default().call(&assets_block(assets_metadata(&[]))).unwrap();
                assert_eq!(assets.fund, "Alpha Fund");
                assert_eq!(assets.currency.resolved(), Some(&Currency::EUR));
                assert_eq!(assets.tot_assets.into_inner(), 1000.0);
                assert_eq!(assets.net_assets.into_inner(), 800.0);
                assert_eq!(assets.liabilities.into_inner(), 200.0);
            }

            #[test]
            fn a_missing_fund_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("fund");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_currency_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("currency");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_tot_assets_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("tot_assets");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_net_assets_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("net_assets");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_liabilities_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("liabilities");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn an_already_typed_currency_is_accepted_directly() {
                let md = assets_metadata(&[("currency", BlockValue::from(Currency::USD))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.currency.resolved(), Some(&Currency::USD));
            }

            #[test]
            fn an_unknown_currency_text_is_an_error() {
                let md = assets_metadata(&[("currency", BlockValue::from("XYZ"))]);
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }
        }

        mod num_converter {
            use super::*;

            #[test]
            fn interprets_amounts_as_integers_by_default() {
                let md = assets_metadata(&[("tot_assets", BlockValue::from("1.000"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.tot_assets.into_inner(), 1000.0);
            }

            #[test]
            fn interprets_amounts_as_floats_when_configured_so() {
                let md = assets_metadata(&[
                    ("tot_assets", BlockValue::from("1.000,5")),
                    ("net_assets", BlockValue::from("800,5")),
                ]);
                let deserializer = DeserializerAssetsStandard::new(false, cast::to_date);
                let assets = deserializer.call(&assets_block(md)).unwrap();
                assert_eq!(assets.tot_assets.into_inner(), 1000.5);
            }

            #[test]
            fn an_unreadable_amount_loses_the_whole_line() {
                let md = assets_metadata(&[("tot_assets", BlockValue::from("not a number"))]);
                let err = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::LineParseFail { field: "tot_assets", .. }));
            }
        }

        /// La stranezza da preservare verbatim (`agent-memory/M8-implementation-plan.md` §2): le
        /// parentesi vengono tolte da `liabilities` **prima** della conversione, quindi
        /// `"(200)"` diventa `200.0`, non `-200.0`.
        mod liabilities_parentheses_quirk {
            use super::*;

            #[test]
            fn parenthesized_liabilities_become_positive_not_negative() {
                let md = assets_metadata(&[("liabilities", BlockValue::from("(200)"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.liabilities.into_inner(), 200.0);
            }

            #[test]
            fn liabilities_without_parentheses_are_unaffected() {
                let md = assets_metadata(&[("liabilities", BlockValue::from("200"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.liabilities.into_inner(), 200.0);
            }
        }

        mod date_field {
            use super::*;

            #[test]
            fn an_absent_date_key_becomes_none_not_an_error() {
                let md = assets_metadata(&[]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert!(assets.date.is_none());
            }

            #[test]
            fn a_null_date_becomes_none_too() {
                let md = assets_metadata(&[("date", BlockValue::Null)]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert!(assets.date.is_none());
            }

            #[test]
            fn a_textual_date_is_converted_with_the_configured_converter() {
                let md = assets_metadata(&[("date", BlockValue::from("2024-12-31"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(
                    assets.date.and_then(|d| d.resolved().copied()),
                    Some(Date::new(2024, 12, 31).unwrap())
                );
            }

            #[test]
            fn an_unreadable_date_loses_the_whole_line() {
                let md = assets_metadata(&[("date", BlockValue::from("not a date"))]);
                let err = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::LineParseFail { field: "date", .. }));
            }
        }

        mod as_a_deserialize_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerAssetsStandard::default().name(), "DeserializerAssetsStandard");
            }

            #[test]
            fn a_matching_block_produces_exactly_one_extracted_result() {
                let out = DeserializerAssetsStandard::default()
                    .deserialize(&assets_block(assets_metadata(&[])))
                    .unwrap();
                assert_eq!(out.len(), 1);
                assert!(out[0].as_fund_assets().is_some());
            }

            #[test]
            fn a_missing_required_field_is_a_value_error_not_a_page_failure() {
                let mut md = assets_metadata(&[]);
                md.remove("fund");
                let err = DeserializerAssetsStandard::default().deserialize(&assets_block(md)).unwrap_err();
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
