//! Le entità che i pipe `deserialize` producono: ciò che finisce nei CSV di output.
//!
//! **Stato: completo (M8).** `PLAN.md` §11 assegna l'intero modulo a M8; M7 ne aveva anticipato
//! due file — [`fund`] e [`investment`] — per **decisione dell'utente D-M7-2** (2026-08-23,
//! `agent-memory/M7-implementation-plan.md` §0): senza `Fund`/`Equity`/`Bond` i due deserializer
//! `DeserializerFundStandard`/`DeserializerInvestmentStandard` non esistono, e senza quelli il
//! segmento `deserialize` della pipeline structured `investments` non è costruibile — cioè la
//! fusione dei tre livelli, che è *il* focus di test di M7, non sarebbe verificabile end-to-end
//! su una pipeline reale. M8 aggiunge le cinque entità restanti —
//! [`assets_manager`]/[`fund_assets`]/[`fund_change_name`]/[`fund_esg_indicator`]/
//! [`fund_sfdr_classification`] — chiudendo il modulo.
//!
//! **Un solo enum d'errore per tutto `output::classes`** ([`OutputClassError`]) invece di uno per
//! sottomodulo: le validazioni di campo sono le stesse per tutte le entità (Pydantic le
//! esprimeva con gli stessi `PositiveFloat`/`confloat` ovunque), e duplicare l'enum
//! costringerebbe a convertire avanti e indietro fra file gemelli. Stesso precedente di
//! `core::promise_resolution`, che riusa `PromiseError` di `core::promise` (M2).
//!
//! **`OrderedFloat<f64>` e non `f64` nudo** nei campi numerici: `core::pipeline::Extracted`, che
//! trasporta queste entità attraverso il motore, deriva `Eq`/`PartialEq`, e un `f64` lo
//! renderebbe impossibile. È la stessa scelta già fatta da `BlockValue::Float` (M2, `PLAN.md`
//! §4.1); i costruttori accettano e gli accessori restituiscono `f64`, quindi il tipo interno non
//! si vede da fuori.

pub mod assets_manager;
pub mod fund;
pub mod fund_assets;
pub mod fund_change_name;
pub mod fund_esg_indicator;
pub mod fund_sfdr_classification;
pub mod investment;

use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::Promised;
use crate::core::promise::Promise;

/// Fallimenti nella costruzione di un'entità di output.
///
/// Sostituisce le eccezioni di validazione di Pydantic (`PositiveFloat`, `confloat(0, 1)`), che
/// `PLAN.md` §7 vuole diventate costruttori fallibili.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputClassError {
    /// Un campo aveva un tipo diverso da quello atteso.
    #[error("field '{field}': {source}")]
    Field {
        field: &'static str,
        #[source]
        source: BlockValueError,
    },
    /// Un campo numerico è fuori dal dominio ammesso.
    #[error("field '{field}': {constraint}, got {value}")]
    OutOfRange { field: &'static str, constraint: FloatConstraint, value: String },
    /// L'equazione contabile di [`fund_assets::FundAssets`] non torna, oltre la tolleranza
    /// `1e-4`. Non è un [`FloatConstraint`] perché non è un vincolo su un singolo campo, ma
    /// incrociato fra tre.
    #[error(
        "unbalanced fund assets: liabilities ({liabilities}) + net_assets ({net_assets}) != tot_assets ({tot_assets})"
    )]
    UnbalancedFundAssets {
        tot_assets: ordered_float::OrderedFloat<f64>,
        liabilities: ordered_float::OrderedFloat<f64>,
        net_assets: ordered_float::OrderedFloat<f64>,
    },
}

/// I domini numerici che Pydantic esprimeva come annotazioni di tipo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatConstraint {
    /// `PositiveFloat`: strettamente maggiore di zero.
    Positive,
    /// `NonNegativeFloat`: maggiore o uguale a zero.
    NonNegative,
    /// `confloat(ge=0.0, lt=1.0)`: una **frazione**, non una percentuale — `0.05` significa 5%.
    UnitIntervalHalfOpen,
}

impl std::fmt::Display for FloatConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            FloatConstraint::Positive => "input should be greater than 0",
            FloatConstraint::NonNegative => "input should be greater than or equal to 0",
            FloatConstraint::UnitIntervalHalfOpen => "input should be in the range [0.0, 1.0)",
        };
        f.write_str(text)
    }
}

impl FloatConstraint {
    /// Verifica `value`, riportando `field` nell'errore.
    pub fn validate(self, field: &'static str, value: f64) -> Result<f64, OutputClassError> {
        let ok = match self {
            FloatConstraint::Positive => value > 0.0,
            FloatConstraint::NonNegative => value >= 0.0,
            FloatConstraint::UnitIntervalHalfOpen => (0.0..1.0).contains(&value),
        };
        if ok {
            Ok(value)
        } else {
            Err(OutputClassError::OutOfRange { field, constraint: self, value: value.to_string() })
        }
    }
}

/// Traduce un [`BlockValue`] in un campo `Promised<T>`: una promessa resta pendente, qualunque
/// altro valore viene convertito subito da `extract`.
///
/// È il punto in cui si concentra la regola generale delle entità di output: **ogni** campo può
/// arrivare come promessa, e la decisione di risolverla o scartarla spetta a
/// `core::promisable::fulfill_promises`, non al costruttore.
pub(crate) fn promised_from_value<T>(
    field: &'static str,
    value: &BlockValue,
    extract: impl FnOnce(&BlockValue) -> Result<T, BlockValueError>,
) -> Result<Promised<T>, OutputClassError> {
    match value {
        BlockValue::Promise(promise) => Ok(Promised::Pending(promise.clone())),
        other => extract(other).map(Promised::Resolved).map_err(|source| OutputClassError::Field { field, source }),
    }
}

/// Come [`promised_from_value`], ma per un campo opzionale: `Null` (o assente) diventa `None`.
pub(crate) fn optional_promised_from_value<T>(
    field: &'static str,
    value: Option<&BlockValue>,
    extract: impl FnOnce(&BlockValue) -> Result<T, BlockValueError>,
) -> Result<Option<Promised<T>>, OutputClassError> {
    match value {
        None | Some(BlockValue::Null) => Ok(None),
        Some(value) => promised_from_value(field, value, extract).map(Some),
    }
}

/// La promessa pendente di un campo, se c'è: helper per le implementazioni di
/// `PromisableFields::pending`.
pub(crate) fn pending_of<T>(field: &'static str, value: &Promised<T>) -> Option<(&'static str, Promise)> {
    value.pending().map(|promise| (field, promise.clone()))
}

/// Serde per un campo `Promised<T>`.
///
/// `Promised<T>` serializza già da solo (M2), ma **non** deserializza: da una stringa non si può
/// decidere in generale se sia un `T` o una promessa, e `PLAN.md` §4.3 lascia la scelta a ogni
/// entità di output. Qui la scelta è: **in lettura una promessa non esiste**, il valore è sempre
/// `Resolved`. È corretto per l'uso reale — `PLAN.md` §7 impone che le promesse siano risolte
/// *prima* della scrittura, quindi nessun file prodotto dal sistema contiene un'entità pendente —
/// e rende il round-trip totale su tutto ciò che il sistema scrive davvero.
pub(crate) mod serde_promised {
    use super::Promised;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(value: &Promised<T>, serializer: S) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Promised<T>, D::Error> {
        T::deserialize(deserializer).map(Promised::Resolved)
    }
}

/// Come [`serde_promised`], per un campo opzionale.
pub(crate) mod serde_optional_promised {
    use super::Promised;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Promised<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Promised<T>>, D::Error> {
        Ok(Option::<T>::deserialize(deserializer)?.map(Promised::Resolved))
    }
}
