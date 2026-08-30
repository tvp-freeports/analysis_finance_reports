//! Accumulo: da `&[DocumentOutcome]` alle tabelle tipizzate di `output::files_schema`, con le
//! promesse già risolte.
//!
//! M8, passo 12 (`agent-memory/M8-implementation-plan.md` §1/§3/§4) — il cuore del focus di test
//! di questa milestone insieme a [`super::write`]. Porta la logica di
//! `packages/freeports_core/src/output/routines.rs::transform_to_files_schema`/`Accumulator`, ma
//! parte direttamente da [`crate::core::algorithm::DocumentOutcome`] (M5) invece che dai due
//! `#[pyclass]` `PageResults`/`DocumentResults` del riferimento — quel ruolo lo svolgono già
//! `DocumentOutcome`/`PageOutcome`, portarli di nuovo sarebbe rifare due volte lo stesso lavoro
//! (`PLAN.md` §1).
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct TransformedTables {
//!     pub investments: Vec<InvestmentRow>,
//!     pub funds: Vec<FundRow>,
//!     pub funds_change_name: Vec<FundChangeNameRow>,
//!     pub funds_assets: Vec<FundAssetsRow>,
//!     pub funds_sfdr_classification: Vec<FundSfdrClassificationRow>,
//!     pub funds_esg_indicators: Vec<FundEsgIndicatorRow>,
//!     pub assets_managers: Vec<AssetsManagerRow>,
//!     pub investments_managers: Vec<InvestmentsManagerRow>,
//!     pub additional_infos: BTreeMap<u32, BondAdditionalInfoRow>,
//! }
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum AccumulateError {
//!     Promise(#[from] PromiseError),        // catena di promesse circolare
//!     Promisable(#[from] PromisableError),   // promessa strict irrisolvibile, o campo malformato
//!     Schema(#[from] SchemaError),           // limite numerico violato, o chiave duplicata
//! }
//!
//! pub fn accumulate(outcomes: &[DocumentOutcome]) -> Result<TransformedTables, AccumulateError>;
//! ```
//!
//! **Regole di comportamento pinnate dai test** (`agent-memory/M8-implementation-plan.md` §4):
//!
//! 1. **Le promesse si risolvono contro un'unica mappa globale**: ogni `Extracted::Promises` di
//!    *ogni* pagina di *ogni* documento viene versato in una sola `PromiseMap` prima di
//!    `flatten()` — le catene di promesse possono attraversare pagine e documenti.
//! 2. **`Report`/`Format`/`Report page`** vengono sempre da `DocumentOutcome::id`/`::format`/
//!    `PageOutcome::page` — non esiste (e non serve) un parametro `batch_mode`.
//! 3. **Deduplicazione fondi**: la chiave è il nome normalizzato e maiuscolizzato
//!    (`Fund::new(name).name()`, idempotente); un fondo visto **solo indirettamente** (mai come
//!    `Extracted::Fund` autonomo, solo come `.fund` di un `Equity`/`Bond`/`FundAssets`/...) resta
//!    nella tabella `funds` ma **senza** `Report`/`Format`/`Report page` — quirk verbatim del
//!    riferimento (`FundRow` è l'unica riga con quei tre campi opzionali, vedi `files_schema.rs`).
//! 4. **Deduplicazione asset manager**: **un'unica tabella condivisa**, chiave il solo nome —
//!    verificato leggendo `Accumulator::assets_managers`/`manager_index` del riferimento (una
//!    singola `HashMap<String, usize>` senza distinzione di tipo): un `ManagementCompany` e un
//!    `InvestmentsManager` con lo **stesso nome** producono la **stessa** riga/id in
//!    `assets_managers`, e solo il tipo dell'istanza incontrata decide se contribuisce a
//!    `funds[].management_company_id` oppure a `investments_managers`.
//! 5. **`Extracted::Promises`** non produce righe (è la fonte della mappa di risoluzione);
//!    **`Extracted::PageClass`** è ignorato (non produce righe, non è un errore vederlo).
//! 6. **Esito della risoluzione per entità**: `Fulfilled::InPlace`/`Expanded` producono una riga
//!    (o una per copia); `Fulfilled::Dropped` fa sparire l'entità da ogni tabella, senza errore;
//!    una promessa *strict* irrisolvibile è un [`AccumulateError`] (mai un panic).

use std::collections::{BTreeMap, HashMap};

use crate::commons::consts::FinancialInstrument;
use crate::commons::date::Date;
use crate::core::algorithm::DocumentOutcome;
#[cfg(test)]
use crate::core::algorithm::PageOutcome;
use crate::core::pipeline::Extracted;
use crate::core::promisable::{Fulfilled, PromisableError, PromisableFields, fulfill_promises};
use crate::core::promise::PromiseError;
use crate::core::promise_resolution::{FlatPromiseMap, PromiseMap};
use crate::output::classes::fund::Fund;
use crate::output::files_schema::{
    AssetsManagerRow, BondAdditionalInfoRow, ChangeNameEventType, FundAssetsRow, FundChangeNameRow,
    FundEsgIndicatorRow, FundRow, FundSfdrClassificationRow, InvestmentRow, InvestmentsManagerRow, SchemaError,
    UniqueTable,
};

// Usati solo dai fixture dei test qui sotto (non da questa produzione): `#[cfg(test)]` evita gli
// `unused import` quando questo crate è compilato senza i test (es. come dipendenza di
// `tests/output_routines.rs`).
#[cfg(test)]
use crate::commons::consts::{Currency, SfdrArticle};
#[cfg(test)]
use crate::core::classes::value::BlockValue;
#[cfg(test)]
use crate::core::page::{DocumentId, FormatName};
#[cfg(test)]
use crate::core::pipeline::PromiseEntries;
#[cfg(test)]
use crate::core::promise::Promise;
#[cfg(test)]
use crate::core::schedule::PageClass;
#[cfg(test)]
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
#[cfg(test)]
use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
#[cfg(test)]
use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
#[cfg(test)]
use crate::output::classes::investment::{Equity, InvestmentFields};
#[cfg(test)]
use std::collections::BTreeSet;

/// Le tabelle tipizzate risultanti dall'accumulo, pronte per [`super::write::write_files`].
#[derive(Debug)]
pub struct TransformedTables {
    pub investments: Vec<InvestmentRow>,
    pub funds: Vec<FundRow>,
    pub funds_change_name: Vec<FundChangeNameRow>,
    pub funds_assets: Vec<FundAssetsRow>,
    pub funds_sfdr_classification: Vec<FundSfdrClassificationRow>,
    pub funds_esg_indicators: Vec<FundEsgIndicatorRow>,
    pub assets_managers: Vec<AssetsManagerRow>,
    pub investments_managers: Vec<InvestmentsManagerRow>,
    pub additional_infos: BTreeMap<u32, BondAdditionalInfoRow>,
}

/// Fallimenti dell'accumulo.
#[derive(Debug, thiserror::Error)]
pub enum AccumulateError {
    /// Una catena di promesse circolare, scoperta durante l'appiattimento della mappa globale.
    #[error(transparent)]
    Promise(#[from] PromiseError),
    /// Una promessa *strict* irrisolvibile, o un valore risolto del tipo sbagliato.
    #[error(transparent)]
    Promisable(#[from] PromisableError),
    /// Un limite numerico violato, o una chiave duplicata.
    #[error(transparent)]
    Schema(#[from] SchemaError),
}

/// Una voce di `funds` in costruzione: `report_page`/`report`/`format` restano `None` finché il
/// fondo non è visto **direttamente** (come `Extracted::Fund`, o come fonte di `management_company_id`
/// negli asset manager) — quirk verbatim del riferimento, vedi il doc-comment del modulo.
struct FundEntryBuilder {
    id: u32,
    name: String,
    management_company_id: Option<u32>,
    report_page: Option<i32>,
    report: Option<String>,
    format: Option<String>,
}

#[derive(Default)]
struct Accumulator {
    investments: Vec<InvestmentRow>,
    fund_index: HashMap<String, usize>,
    funds: Vec<FundEntryBuilder>,
    manager_index: HashMap<String, usize>,
    assets_managers: Vec<AssetsManagerRow>,
    funds_change_name: Vec<FundChangeNameRow>,
    funds_assets: Vec<FundAssetsRow>,
    funds_sfdr_classification: Vec<FundSfdrClassificationRow>,
    funds_esg_indicators: Vec<FundEsgIndicatorRow>,
    investments_managers_to_funds: Vec<InvestmentsManagerRow>,
    add_infos: BTreeMap<u32, BondAdditionalInfoRow>,
}

impl Accumulator {
    /// `name` deve già essere la chiave canonica (il nome normalizzato+maiuscolizzato che
    /// `Fund::name()` produce): ogni chiamante la calcola prima di arrivare qui.
    fn get_or_create_fund(&mut self, name: &str) -> usize {
        if let Some(&idx) = self.fund_index.get(name) {
            idx
        } else {
            let id = self.funds.len() as u32 + 1;
            self.funds.push(FundEntryBuilder {
                id,
                name: name.to_string(),
                management_company_id: None,
                report_page: None,
                report: None,
                format: None,
            });
            let idx = self.funds.len() - 1;
            self.fund_index.insert(name.to_string(), idx);
            idx
        }
    }

    fn finalize(self) -> Result<TransformedTables, SchemaError> {
        let funds: Vec<FundRow> = self
            .funds
            .into_iter()
            .map(|b| {
                FundRow::new(
                    b.id as i64,
                    b.name,
                    b.management_company_id.map(|v| v as i64),
                    b.report_page,
                    b.report,
                    b.format,
                )
            })
            .collect::<Result<_, _>>()?;

        let mut fcn_table: UniqueTable<FundChangeNameRow> =
            UniqueTable::new("funds_change_name", "Fund ID|From|Type of event|Old name");
        for row in self.funds_change_name {
            let key = format!("{}|{:?}|{:?}|{}", row.fund_id, row.from_date, row.event_type, row.old_name);
            fcn_table.push(key, row)?;
        }

        let mut fa_table: UniqueTable<FundAssetsRow> = UniqueTable::new("funds_assets", "Fund ID|Date");
        for row in self.funds_assets {
            let key = format!("{}|{:?}", row.fund_id, row.date);
            fa_table.push(key, row)?;
        }

        let mut sfdr_table: UniqueTable<FundSfdrClassificationRow> =
            UniqueTable::new("funds_sfdr_classification", "Fund ID");
        for row in self.funds_sfdr_classification {
            let key = row.fund_id.to_string();
            sfdr_table.push(key, row)?;
        }

        let mut im_table: UniqueTable<InvestmentsManagerRow> =
            UniqueTable::new("investments_managers", "Investment manager ID|Fund ID");
        for row in self.investments_managers_to_funds {
            let key = format!("{}|{}", row.investment_manager_id, row.fund_id);
            im_table.push(key, row)?;
        }

        Ok(TransformedTables {
            investments: self.investments,
            assets_managers: self.assets_managers,
            funds,
            investments_managers: im_table.into_rows(),
            funds_sfdr_classification: sfdr_table.into_rows(),
            funds_esg_indicators: self.funds_esg_indicators,
            funds_change_name: fcn_table.into_rows(),
            funds_assets: fa_table.into_rows(),
            additional_infos: self.add_infos,
        })
    }
}

/// La chiave canonica di un fondo dal nome grezzo (scritto in un blocco): `Fund::new(name).name()`
/// — normalizzato e maiuscolizzato, idempotente su un nome già normalizzato. È lo stesso valore
/// che `Extracted::Fund(fund)` produce direttamente via `fund.name()`, quindi le due fonti
/// convergono sempre sulla stessa riga di `funds`.
fn canonical_fund_key(raw_name: &str) -> String {
    Fund::new(raw_name).name().expect("a freshly-built Fund is always resolved")
}

/// Risolve le promesse di `entity` contro `flat`, restituendo zero (`Dropped`), una (`InPlace`) o
/// più (`Expanded`) copie già risolte.
fn resolve<T: PromisableFields + Clone>(mut entity: T, flat: &FlatPromiseMap) -> Result<Vec<T>, AccumulateError> {
    match fulfill_promises(&mut entity, flat)? {
        Fulfilled::InPlace => Ok(vec![entity]),
        Fulfilled::Expanded(copies) => Ok(copies),
        Fulfilled::Dropped => Ok(Vec::new()),
    }
}

/// Assembla `outcomes` nelle tabelle tipizzate di `output::files_schema`, risolvendo ogni
/// promessa prima di costruire le righe. Vedi il doc-comment del modulo per le regole di
/// comportamento pinnate dai test.
pub fn accumulate(outcomes: &[DocumentOutcome]) -> Result<TransformedTables, AccumulateError> {
    // Fase 1: ogni `Extracted::Promises` di ogni pagina di ogni documento confluisce in un'unica
    // mappa globale — le catene di promesse possono attraversare pagine e documenti.
    let mut promise_map = PromiseMap::new();
    for doc in outcomes {
        for page in &doc.pages {
            for result in &page.results {
                if let Extracted::Promises(entries) = result {
                    entries.merge_into(&mut promise_map);
                }
            }
        }
    }
    let flat = promise_map.flatten()?;
    tracing::debug!(documents = outcomes.len(), promise_ids = flat.len(), "promise map flattened");

    // Fase 2: ogni entità viene risolta e versata nella tabella giusta.
    let mut acc = Accumulator::default();
    for doc in outcomes {
        let report = doc.id.as_str().to_string();
        let format = doc.format.as_str().to_string();
        for page in &doc.pages {
            let page_n = page.page as i32;
            for result in &page.results {
                match result {
                    Extracted::Promises(_) | Extracted::PageClass(_) => {}

                    Extracted::Fund(fund) => {
                        for fund in resolve(fund.clone(), &flat)? {
                            let key = fund.name().expect("resolved after fulfill_promises");
                            let idx = acc.get_or_create_fund(&key);
                            if acc.funds[idx].report_page.is_none() {
                                acc.funds[idx].report_page = Some(page_n);
                                acc.funds[idx].report = Some(report.clone());
                                acc.funds[idx].format = Some(format.clone());
                            }
                        }
                    }

                    Extracted::Equity(equity) => {
                        for equity in resolve(equity.clone(), &flat)? {
                            push_investment(&mut acc, &equity.data, FinancialInstrument::EQUITY, None, None, page_n, &report, &format)?;
                        }
                    }

                    Extracted::Bond(bond) => {
                        for bond in resolve(bond.clone(), &flat)? {
                            push_investment(
                                &mut acc,
                                &bond.data,
                                FinancialInstrument::BOND,
                                bond.maturity,
                                bond.interest_rate.map(|v| v.into_inner()),
                                page_n,
                                &report,
                                &format,
                            )?;
                        }
                    }

                    Extracted::FundAssets(assets) => {
                        for assets in resolve(assets.clone(), &flat)? {
                            let idx = acc.get_or_create_fund(&canonical_fund_key(&assets.fund));
                            let fund_id = acc.funds[idx].id;
                            let date = assets.date.and_then(|d| d.resolved().copied());
                            let currency = *assets.currency.resolved().expect("resolved after fulfill_promises");
                            let id = acc.funds_assets.len() as i64 + 1;
                            acc.funds_assets.push(FundAssetsRow::new(
                                id,
                                page_n,
                                report.clone(),
                                format.clone(),
                                fund_id as i64,
                                date,
                                assets.tot_assets.into_inner() as f32,
                                assets.liabilities.into_inner() as f32,
                                assets.net_assets.into_inner() as f32,
                                currency,
                            )?);
                        }
                    }

                    Extracted::FundSfdrClassification(fsc) => {
                        for fsc in resolve(fsc.clone(), &flat)? {
                            let idx = acc.get_or_create_fund(&canonical_fund_key(&fsc.fund));
                            let fund_id = acc.funds[idx].id;
                            let article = *fsc.article.resolved().expect("resolved after fulfill_promises");
                            acc.funds_sfdr_classification.push(FundSfdrClassificationRow::new(
                                fund_id as i64,
                                article,
                                page_n,
                                report.clone(),
                                format.clone(),
                            )?);
                        }
                    }

                    Extracted::FundEsgIndicator(fei) => {
                        for fei in resolve(fei.clone(), &flat)? {
                            let fund_raw = fei.fund.resolved().expect("resolved after fulfill_promises");
                            let idx = acc.get_or_create_fund(&canonical_fund_key(fund_raw));
                            let fund_id = acc.funds[idx].id;
                            acc.funds_esg_indicators.push(FundEsgIndicatorRow::new(
                                fund_id as i64,
                                fei.name.clone(),
                                fei.value.clone(),
                                page_n,
                                report.clone(),
                                format.clone(),
                            )?);
                        }
                    }

                    Extracted::FundRename(rename) => {
                        for rename in resolve(rename.clone(), &flat)? {
                            push_fund_change_name(&mut acc, &rename.data, ChangeNameEventType::Renaming, page_n, &report, &format)?;
                        }
                    }

                    Extracted::FundMerge(merge) => {
                        for merge in resolve(merge.clone(), &flat)? {
                            push_fund_change_name(&mut acc, &merge.data, ChangeNameEventType::Merging, page_n, &report, &format)?;
                        }
                    }

                    Extracted::ManagementCompany(mc) => {
                        for mc in resolve(mc.clone(), &flat)? {
                            let am_id = get_or_create_manager(&mut acc, &mc.data.name, page_n, &report, &format)?;
                            for fund_name in &mc.data.managed_funds {
                                let fund_idx = acc.get_or_create_fund(&canonical_fund_key(fund_name));
                                acc.funds[fund_idx].management_company_id = Some(am_id);
                            }
                        }
                    }

                    Extracted::InvestmentsManager(im) => {
                        for im in resolve(im.clone(), &flat)? {
                            let am_id = get_or_create_manager(&mut acc, &im.data.name, page_n, &report, &format)?;
                            for fund_name in &im.data.managed_funds {
                                let fund_idx = acc.get_or_create_fund(&canonical_fund_key(fund_name));
                                let fund_id = acc.funds[fund_idx].id;
                                acc.investments_managers_to_funds
                                    .push(InvestmentsManagerRow::new(am_id as i64, fund_id as i64)?);
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::debug!(
        investments = acc.investments.len(),
        funds = acc.funds.len(),
        funds_change_name = acc.funds_change_name.len(),
        funds_assets = acc.funds_assets.len(),
        funds_sfdr_classification = acc.funds_sfdr_classification.len(),
        funds_esg_indicators = acc.funds_esg_indicators.len(),
        assets_managers = acc.assets_managers.len(),
        investments_managers = acc.investments_managers_to_funds.len(),
        "accumulated tables"
    );
    Ok(acc.finalize()?)
}

/// I campi comuni di `Equity`/`Bond`, con le specificità di ciascuno passate a parte (`financial_instrument`,
/// `maturity`/`interest_rate`, entrambi `None` per un'`Equity`).
#[allow(clippy::too_many_arguments)]
fn push_investment(
    acc: &mut Accumulator,
    data: &crate::output::classes::investment::InvestmentData,
    financial_instrument: FinancialInstrument,
    maturity: Option<Date>,
    interest_rate: Option<f64>,
    page_n: i32,
    report: &str,
    format: &str,
) -> Result<(), AccumulateError> {
    let fund_raw = data.fund.resolved().expect("resolved after fulfill_promises");
    let idx = acc.get_or_create_fund(&canonical_fund_key(fund_raw));
    let fund_id = acc.funds[idx].id;
    let currency = *data.currency.resolved().expect("resolved after fulfill_promises");
    let market_value = data.market_value.resolved().expect("resolved after fulfill_promises").into_inner();

    let id = acc.investments.len() as i64 + 1;
    let row = InvestmentRow::new(
        id,
        page_n,
        report.to_string(),
        format.to_string(),
        data.company_match.clone(),
        data.company.clone(),
        financial_instrument,
        data.nominal_quantity.map(|v| v.into_inner() as f32),
        market_value as f32,
        currency,
        data.perc_net_assets.as_ref().map(|p| p.resolved().expect("resolved after fulfill_promises").into_inner() as f32),
        fund_id as i64,
        data.acquisition_cost.as_ref().map(|p| p.resolved().expect("resolved after fulfill_promises").into_inner() as f32),
        data.acquisition_currency.as_ref().map(|p| *p.resolved().expect("resolved after fulfill_promises")),
    )?;

    if financial_instrument == FinancialInstrument::BOND {
        let bond_info = BondAdditionalInfoRow::new(maturity, interest_rate)?;
        acc.add_infos.insert(row.id, bond_info);
    }
    acc.investments.push(row);
    Ok(())
}

fn push_fund_change_name(
    acc: &mut Accumulator,
    data: &crate::output::classes::fund_change_name::FundChangeNameData,
    event_type: ChangeNameEventType,
    page_n: i32,
    report: &str,
    format: &str,
) -> Result<(), AccumulateError> {
    let idx = acc.get_or_create_fund(&canonical_fund_key(&data.current_name));
    let fund_id = acc.funds[idx].id;
    let from_date = *data.date.resolved().expect("resolved after fulfill_promises");
    let id = acc.funds_change_name.len() as i64 + 1;
    acc.funds_change_name.push(FundChangeNameRow::new(
        id,
        page_n,
        report.to_string(),
        format.to_string(),
        fund_id as i64,
        from_date,
        event_type,
        data.old_name.clone(),
    )?);
    Ok(())
}

/// Trova o crea la riga `assets_managers` per `name`, restituendone l'id — condivisa fra
/// `ManagementCompany` e `InvestmentsManager` (§1: un'unica tabella, un unico indice per nome).
fn get_or_create_manager(
    acc: &mut Accumulator,
    name: &str,
    page_n: i32,
    report: &str,
    format: &str,
) -> Result<u32, AccumulateError> {
    if let Some(&idx) = acc.manager_index.get(name) {
        return Ok(acc.assets_managers[idx].id);
    }
    let id = acc.assets_managers.len() as i64 + 1;
    let row = AssetsManagerRow::new(id, page_n, report.to_string(), format.to_string(), name.to_string())?;
    acc.assets_managers.push(row);
    let idx = acc.assets_managers.len() - 1;
    acc.manager_index.insert(name.to_string(), idx);
    Ok(acc.assets_managers[idx].id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, format: &str, pages: Vec<PageOutcome>) -> DocumentOutcome {
        DocumentOutcome { id: DocumentId::new(id), format: FormatName::new(format), pages }
    }

    fn page(number: u32, class: &str, results: Vec<Extracted>) -> PageOutcome {
        PageOutcome { page: number, class: PageClass::new(class), results }
    }

    fn equity(fund: &str, market_value: f64) -> Extracted {
        Extracted::Equity(
            Equity::build(InvestmentFields::new(
                "Acme Corp",
                "Acme",
                BlockValue::from(fund),
                BlockValue::from(market_value),
                BlockValue::from(Currency::EUR),
            ))
            .expect("fixed, valid fixture"),
        )
    }

    fn equity_with_promised_fund(promise_id: &str) -> Extracted {
        Extracted::Equity(
            Equity::build(InvestmentFields::new(
                "Acme Corp",
                "Acme",
                BlockValue::from(Promise::new(promise_id)),
                BlockValue::from(1000.0),
                BlockValue::from(Currency::EUR),
            ))
            .expect("fixed, valid fixture"),
        )
    }

    fn equity_with_promised_market_value(fund: &str, promise_id: &str) -> Extracted {
        Extracted::Equity(
            Equity::build(InvestmentFields::new(
                "Acme Corp",
                "Acme",
                BlockValue::from(fund),
                BlockValue::from(Promise::new(promise_id)),
                BlockValue::from(Currency::EUR),
            ))
            .expect("fixed, valid fixture"),
        )
    }

    fn fund_entry(name: &str) -> Extracted {
        Extracted::Fund(Fund::new(name))
    }

    fn promises(entries: Vec<(&str, BlockValue)>) -> Extracted {
        Extracted::Promises(entries.into_iter().collect::<PromiseEntries>())
    }

    fn management_company(name: &str) -> Extracted {
        Extracted::ManagementCompany(
            ManagementCompany::build(&BlockValue::from(name), &BlockValue::Set(BTreeSet::new()))
                .expect("fixed, valid fixture"),
        )
    }

    fn investments_manager(name: &str) -> Extracted {
        Extracted::InvestmentsManager(
            InvestmentsManager::build(&BlockValue::from(name), &BlockValue::Set(BTreeSet::new()))
                .expect("fixed, valid fixture"),
        )
    }

    fn fund_sfdr(fund: &str, article: SfdrArticle) -> Extracted {
        Extracted::FundSfdrClassification(
            FundSfdrClassification::build(fund, &BlockValue::from(article)).expect("fixed, valid fixture"),
        )
    }

    mod report_debug_info {
        use super::*;

        #[test]
        fn investment_rows_carry_report_format_and_page_from_the_outcome() {
            let outcomes = vec![doc("Report A", "FMT", vec![page(3, "investments", vec![equity("Alpha Fund", 1000.0)])])];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.investments.len(), 1);
            let row = &tables.investments[0];
            assert_eq!(row.report, "Report A");
            assert_eq!(row.format, "FMT");
            assert_eq!(row.report_page, 3);
        }

        #[test]
        fn multiple_pages_of_the_same_document_all_contribute_rows() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![
                    page(1, "investments", vec![equity("Alpha Fund", 1000.0)]),
                    page(2, "investments", vec![equity("Beta Fund", 500.0)]),
                ],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.investments.len(), 2);
        }

        #[test]
        fn multiple_documents_all_contribute_rows_with_their_own_report_name() {
            let outcomes = vec![
                doc("Doc A", "FMT", vec![page(1, "investments", vec![equity("Alpha Fund", 1000.0)])]),
                doc("Doc B", "FMT", vec![page(1, "investments", vec![equity("Beta Fund", 500.0)])]),
            ];
            let tables = accumulate(&outcomes).unwrap();
            let reports: BTreeSet<&str> = tables.investments.iter().map(|r| r.report.as_str()).collect();
            assert_eq!(reports, BTreeSet::from(["Doc A", "Doc B"]));
        }
    }

    mod fund_deduplication {
        use super::*;

        #[test]
        fn two_equities_referencing_the_same_fund_up_to_normalization_share_one_fund_id() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![
                    page(1, "investments", vec![equity("Alpha  Fund", 1000.0)]),
                    page(2, "investments", vec![equity("alpha fund", 500.0)]),
                ],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.funds.len(), 1);
            let fund_id = tables.funds[0].id;
            assert!(tables.investments.iter().all(|r| r.fund_id == fund_id));
        }

        #[test]
        fn a_fund_seen_only_indirectly_has_no_report_debug_info() {
            let outcomes = vec![doc("R", "FMT", vec![page(1, "investments", vec![equity("Alpha Fund", 1000.0)])])];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.funds.len(), 1);
            assert_eq!(tables.funds[0].report_page, None);
            assert_eq!(tables.funds[0].report, None);
            assert_eq!(tables.funds[0].format, None);
        }

        #[test]
        fn a_fund_also_seen_directly_gets_the_debug_info_of_its_direct_sighting() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![
                    page(1, "investments", vec![equity("Alpha Fund", 1000.0)]),
                    page(2, "funds", vec![fund_entry("Alpha Fund")]),
                ],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.funds.len(), 1);
            assert_eq!(tables.funds[0].report_page, Some(2));
        }
    }

    mod asset_manager_deduplication {
        use super::*;

        #[test]
        fn a_management_company_and_an_investments_manager_with_the_same_name_share_one_row() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(1, "c", vec![management_company("Acme AM")]), page(2, "c", vec![investments_manager("Acme AM")])],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(
                tables.assets_managers.len(),
                1,
                "a ManagementCompany and an InvestmentsManager with the same name must share one assets_managers row"
            );
        }

        #[test]
        fn two_differently_named_managers_get_two_rows() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(1, "c", vec![management_company("Acme AM"), management_company("Other AM")])],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.assets_managers.len(), 2);
        }
    }

    mod ignored_and_source_only_variants {
        use super::*;

        #[test]
        fn page_class_results_produce_no_rows_and_no_error() {
            let outcomes =
                vec![doc("R", "FMT", vec![page(1, "c", vec![Extracted::PageClass(Some(PageClass::new("investments")))])])];
            let tables = accumulate(&outcomes).unwrap();
            assert!(tables.investments.is_empty());
            assert!(tables.funds.is_empty());
        }

        #[test]
        fn an_unclassified_page_class_is_also_ignored_without_error() {
            let outcomes = vec![doc("R", "FMT", vec![page(1, "c", vec![Extracted::PageClass(None)])])];
            assert!(accumulate(&outcomes).is_ok());
        }

        #[test]
        fn promises_by_themselves_produce_no_rows() {
            let outcomes = vec![doc("R", "FMT", vec![page(1, "c", vec![promises(vec![("id", BlockValue::from(1i64))])])])];
            let tables = accumulate(&outcomes).unwrap();
            assert!(tables.investments.is_empty());
            assert!(tables.funds.is_empty());
        }
    }

    mod promise_resolution {
        use super::*;

        #[test]
        fn a_pending_market_value_is_resolved_before_the_row_is_built() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(
                    1,
                    "investments",
                    vec![
                        equity_with_promised_market_value("Alpha Fund", "mv-id"),
                        promises(vec![("mv-id", BlockValue::from(1500.0))]),
                    ],
                )],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.investments.len(), 1);
            assert_eq!(tables.investments[0].market_value, 1500.0);
        }

        #[test]
        fn a_non_strict_unresolvable_promise_drops_the_entity_without_error() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(1, "investments", vec![equity_with_promised_market_value("Alpha Fund", "mv-id")])],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert!(tables.investments.is_empty());
        }

        #[test]
        fn a_strict_unresolvable_promise_is_a_fatal_accumulate_error() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(1, "investments", vec![equity_with_promised_market_value("Alpha Fund", "mv-id!")])],
            )];
            assert!(accumulate(&outcomes).is_err());
        }

        #[test]
        fn a_multiple_promise_expands_the_entity_into_one_row_per_value() {
            let promise_val: BlockValue = Promise::new("fund-id[]").into();
            let pending = Extracted::FundEsgIndicator(
                FundEsgIndicator::build(&promise_val, "GHG intensity", "12.3").expect("fixed, valid fixture"),
            );
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(
                    1,
                    "c",
                    vec![
                        pending,
                        promises(vec![
                            ("fund-id", BlockValue::from("Alpha Fund")),
                            ("fund-id", BlockValue::from("Beta Fund")),
                        ]),
                    ],
                )],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.funds_esg_indicators.len(), 2);
        }

        #[test]
        fn a_promise_chain_spanning_two_pages_of_the_same_document_resolves() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![
                    page(1, "investments", vec![equity_with_promised_fund("fund-id")]),
                    page(2, "investments", vec![promises(vec![("fund-id", BlockValue::from("Alpha Fund"))])]),
                ],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.investments.len(), 1);
            assert_eq!(tables.funds[0].name, "ALPHA FUND");
        }

        #[test]
        fn a_promise_can_be_resolved_by_a_contribution_from_a_different_document() {
            let outcomes = vec![
                doc("Doc A", "FMT", vec![page(1, "investments", vec![equity_with_promised_fund("fund-id")])]),
                doc(
                    "Doc B",
                    "FMT",
                    vec![page(1, "investments", vec![promises(vec![("fund-id", BlockValue::from("Alpha Fund"))])])],
                ),
            ];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.investments.len(), 1);
        }

        #[test]
        fn a_circular_promise_chain_is_a_fatal_accumulate_error() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![page(
                    1,
                    "investments",
                    vec![
                        equity_with_promised_market_value("Alpha Fund", "a"),
                        promises(vec![("a", BlockValue::from(Promise::new("b"))), ("b", BlockValue::from(Promise::new("a")))]),
                    ],
                )],
            )];
            assert!(accumulate(&outcomes).is_err());
        }
    }

    mod uniqueness {
        use super::*;

        #[test]
        fn two_sfdr_classifications_of_the_same_fund_are_rejected_as_duplicates() {
            // La chiave di unicità di `funds_sfdr_classification` è il solo `Fund ID`: una
            // seconda classificazione per lo stesso fondo, in una pagina diversa, è un duplicato
            // anche se l'articolo dichiarato è diverso — stesso comportamento del riferimento.
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![
                    page(1, "c", vec![fund_sfdr("Alpha Fund", SfdrArticle::Art8)]),
                    page(2, "c", vec![fund_sfdr("Alpha Fund", SfdrArticle::Art9)]),
                ],
            )];
            assert!(accumulate(&outcomes).is_err());
        }

        #[test]
        fn classifications_of_different_funds_are_both_accepted() {
            let outcomes = vec![doc(
                "R",
                "FMT",
                vec![
                    page(1, "c", vec![fund_sfdr("Alpha Fund", SfdrArticle::Art8)]),
                    page(2, "c", vec![fund_sfdr("Beta Fund", SfdrArticle::Art9)]),
                ],
            )];
            let tables = accumulate(&outcomes).unwrap();
            assert_eq!(tables.funds_sfdr_classification.len(), 2);
        }
    }

    mod empty_input {
        use super::*;

        #[test]
        fn no_documents_produce_every_table_empty() {
            let tables = accumulate(&[]).unwrap();
            assert!(tables.investments.is_empty());
            assert!(tables.funds.is_empty());
            assert!(tables.funds_change_name.is_empty());
            assert!(tables.funds_assets.is_empty());
            assert!(tables.funds_sfdr_classification.is_empty());
            assert!(tables.funds_esg_indicators.is_empty());
            assert!(tables.assets_managers.is_empty());
            assert!(tables.investments_managers.is_empty());
            assert!(tables.additional_infos.is_empty());
        }

        #[test]
        fn a_document_with_no_pages_contributes_nothing() {
            let tables = accumulate(&[doc("R", "FMT", vec![])]).unwrap();
            assert!(tables.investments.is_empty());
        }
    }
}
