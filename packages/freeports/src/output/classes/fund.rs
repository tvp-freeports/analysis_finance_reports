//! L'entità `Fund`: il nome di un fondo, normalizzato.
//!
//! Anticipata da M8 a M7 per decisione D-M7-2 dell'utente — vedi il doc-comment di
//! [`crate::output::classes`].
//!
//! **La sorpresa del riferimento, conservata.** In Python `Fund.__init__` calcola la forma
//! profondamente normalizzata del nome e poi *sovrascrive* `self.name` con quella forma in
//! maiuscolo: il campo che finisce nei CSV **non** è l'argomento del costruttore ma il nome
//! normalizzato e maiuscolizzato, mentre hash e uguaglianza usano la forma normalizzata non
//! maiuscolizzata. Qui il campo interno è quella forma normalizzata (minuscola) e
//! [`Fund::name`] la maiuscolizza in lettura, che è lo stesso comportamento osservabile senza
//! doverne tenere due copie sincronizzate.
//!
//! **Bug del riferimento non replicato** (già corretto nel porting Rust di `freeports_core`, e
//! qui strutturalmente impossibile): risolvere una promessa su `name` via `setattr` saltava la
//! normalizzazione di `__init__`, lasciando un `Fund` con nome non normalizzato il cui `hash()`
//! sollevava `AttributeError`. [`Fund::resolve_field`] normalizza sempre, quindi un `Fund`
//! risolto da promessa si comporta esattamente come uno costruito direttamente.

use serde::{Deserialize, Serialize};

use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::normalization::deep_normalize_string;
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{OutputClassError, pending_of, promised_from_value, serde_promised};

/// Un fondo, identificato dal solo nome.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fund {
    /// La forma profondamente normalizzata (minuscola) del nome, oppure la promessa che la
    /// produrrà. È privata perché il nome "vero" — quello che si legge e si scrive — è
    /// [`Fund::name`], che la maiuscolizza.
    #[serde(with = "serde_promised")]
    n_name: Promised<String>,
}

impl Fund {
    /// Costruisce un fondo da un nome già noto.
    pub fn new(name: &str) -> Self {
        Self { n_name: Promised::Resolved(deep_normalize_string(name)) }
    }

    /// Costruisce un fondo da un [`BlockValue`], che può essere una stringa o una promessa.
    pub fn from_value(value: &BlockValue) -> Result<Self, OutputClassError> {
        let n_name = promised_from_value("name", value, |v| v.str_or_fail("name").map(deep_normalize_string))?;
        Ok(Self { n_name })
    }

    /// Il nome del fondo: normalizzato e in maiuscolo, come nel riferimento. `None` finché il
    /// nome è una promessa non risolta.
    pub fn name(&self) -> Option<String> {
        self.n_name.resolved().map(|n| n.to_uppercase())
    }

    /// La forma normalizzata minuscola, quella su cui si fanno i confronti fra fondi.
    pub fn normalized_name(&self) -> Option<&str> {
        self.n_name.resolved().map(String::as_str)
    }

    /// La promessa ancora da risolvere, se il nome è pendente.
    pub fn pending_name(&self) -> Option<&Promise> {
        self.n_name.pending()
    }
}

impl PromisableFields for Fund {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        pending_of("name", &self.n_name).into_iter().collect()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            // Sempre normalizzato, anche qui: è la correzione descritta nel doc-comment del modulo.
            "name" => {
                self.n_name = Promised::Resolved(deep_normalize_string(value.str_or_fail("name")?));
                Ok(())
            }
            other => unreachable!("Fund has no promisable field {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise::Promise;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    mod construction {
        use super::*;

        #[test]
        fn normalizes_and_uppercases_the_name_it_exposes() {
            assert_eq!(Fund::new("  Alpha   Fund  ").name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn keeps_the_lowercase_normalized_form_for_comparisons() {
            assert_eq!(Fund::new("Alpha Fund").normalized_name(), Some("alpha fund"));
        }

        #[test]
        fn two_names_differing_only_in_case_and_spacing_are_the_same_fund() {
            assert_eq!(Fund::new("Alpha  Fund"), Fund::new("alpha fund"));
        }

        #[test]
        fn builds_from_a_string_block_value() {
            let fund = Fund::from_value(&BlockValue::from("Alpha Fund")).unwrap();
            assert_eq!(fund.name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn a_non_string_block_value_is_a_typed_field_error() {
            let err = Fund::from_value(&BlockValue::from(42i64)).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "name", .. }));
        }

        #[test]
        fn a_null_block_value_is_rejected_rather_than_silently_accepted() {
            assert!(Fund::from_value(&BlockValue::Null).is_err());
        }
    }

    mod promises {
        use super::*;

        fn promised_fund() -> Fund {
            Fund::from_value(&BlockValue::Promise(Promise::new("fund-id"))).unwrap()
        }

        #[test]
        fn a_promise_stays_pending_instead_of_becoming_a_name() {
            let fund = promised_fund();
            assert_eq!(fund.name(), None);
            assert!(fund.pending_name().is_some());
        }

        #[test]
        fn a_pending_fund_reports_its_name_field_as_pending() {
            let pending = promised_fund().pending();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].0, "name");
        }

        #[test]
        fn a_resolved_fund_reports_nothing_pending() {
            assert!(Fund::new("Alpha").pending().is_empty());
        }

        #[test]
        fn resolving_a_promise_normalizes_the_name_exactly_like_construction() {
            // È la correzione descritta nel doc-comment del modulo: nel riferimento Python la
            // risoluzione via `setattr` saltava la normalizzazione.
            let mut fund = promised_fund();
            fund.resolve_field("name", BlockValue::from("  Alpha   Fund ")).unwrap();
            assert_eq!(fund, Fund::new("Alpha Fund"));
            assert_eq!(fund.name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn resolving_with_a_non_string_value_is_an_error() {
            let mut fund = promised_fund();
            assert!(fund.resolve_field("name", BlockValue::from(1i64)).is_err());
        }

        #[test]
        fn fulfilling_against_a_map_produces_the_same_fund_as_direct_construction() {
            let mut fund = promised_fund();
            let map = FlatPromiseMap::from_iter([("fund-id".to_string(), vec![BlockValue::from("Alpha Fund")])]);
            assert_eq!(fulfill_promises(&mut fund, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(fund, Fund::new("Alpha Fund"));
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_fund_survives_a_json_roundtrip() {
            let fund = Fund::new("Alpha Fund");
            let json = serde_json::to_string(&fund).unwrap();
            assert_eq!(serde_json::from_str::<Fund>(&json).unwrap(), fund);
        }

        #[test]
        fn the_serialized_form_is_the_normalized_name_not_the_promise_wrapper() {
            let json = serde_json::to_string(&Fund::new("Alpha Fund")).unwrap();
            assert!(json.contains("alpha fund"), "unexpected serialization: {json}");
        }
    }
}
