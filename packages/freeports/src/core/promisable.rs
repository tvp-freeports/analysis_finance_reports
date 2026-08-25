//! Come un'entita' con campi promessi viene risolta contro una mappa appiattita.
//!
//! [`Promised<T>`] e' il singolo campo — o gia' un `T`, o ancora una
//! [`Promise`]. [`PromisableFields`] e' cio' che un'entita' deve saper fare perche'
//! [`fulfill_promises`] possa risolverla: elencare i campi ancora pendenti e assegnarne uno per
//! nome. Le entita' vere sono quelle di `output::classes` (M8): qui c'e' solo il meccanismo.
//!
//! **Il contratto di ritorno e' un enum, non un `Option<Vec<_>>`** (`PLAN.md` §4.3). Nel
//! riferimento `fulfill_promises` restituiva `None` per "risolto sul posto", `Some([])` per
//! "l'entita' sparisce" e `Some([...])` per "l'entita' si moltiplica": tre significati diversi
//! nello stesso tipo, distinguibili solo leggendo il chiamante. [`Fulfilled`] li rende tre
//! varianti con un nome.
//!
//! **Due fasi, in quest'ordine** (semantica del riferimento, conservata):
//!
//! 1. i campi con promessa normale si risolvono **sul posto**, mutando l'entita';
//! 2. solo dopo, i campi con promessa *multiple* espandono l'entita' in una copia per valore —
//!    prodotto cartesiano se i campi multiple sono piu' d'uno, nell'ordine in cui compaiono in
//!    [`PromisableFields::pending`].
//!
//! L'ordine conta: le copie prodotte dalla fase 2 portano gia' i valori risolti dalla fase 1,
//! invece di doverli risolvere una volta per copia.

use serde::{Serialize, Serializer};

use super::classes::value::{BlockValue, BlockValueError};
use super::promise::{Promise, PromiseError};
use super::promise_resolution::FlatPromiseMap;

/// Un campo che o e' gia' risolto, o e' ancora una promessa.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Promised<T> {
    Resolved(T),
    Pending(Promise),
}

impl<T> Promised<T> {
    /// Il valore, se il campo e' gia' risolto.
    pub fn resolved(&self) -> Option<&T> {
        match self {
            Promised::Resolved(v) => Some(v),
            Promised::Pending(_) => None,
        }
    }

    /// La promessa, se il campo e' ancora pendente.
    pub fn pending(&self) -> Option<&Promise> {
        match self {
            Promised::Pending(p) => Some(p),
            Promised::Resolved(_) => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Promised::Pending(_))
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, Promised::Resolved(_))
    }

    /// Consuma il campo e restituisce il valore risolto, se c'e'.
    pub fn into_resolved(self) -> Option<T> {
        match self {
            Promised::Resolved(v) => Some(v),
            Promised::Pending(_) => None,
        }
    }

    /// Trasforma il valore risolto lasciando intatta la promessa pendente.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Promised<U> {
        match self {
            Promised::Resolved(v) => Promised::Resolved(f(v)),
            Promised::Pending(p) => Promised::Pending(p),
        }
    }
}

impl<T> From<Promise> for Promised<T> {
    fn from(p: Promise) -> Self {
        Promised::Pending(p)
    }
}

/// Serializza come il valore risolto, o come la forma canonica della promessa — la stessa forma
/// che un repo formati scrive nei CSV.
///
/// La direzione opposta non e' implementata di proposito: da una stringa non si puo' decidere in
/// generale se sia un `T` o una promessa (per `T = String` sono la stessa cosa), quindi la scelta
/// spetta all'entita' che dichiara il campo — sara' `output::classes` (M8) a fissarla, tipo per
/// tipo.
impl<T: Serialize> Serialize for Promised<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Promised::Resolved(v) => v.serialize(serializer),
            Promised::Pending(p) => p.serialize(serializer),
        }
    }
}

/// Cos'e' successo a un'entita' passata a [`fulfill_promises`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fulfilled<T> {
    /// Ogni promessa si e' risolta sul posto: l'entita' passata e' quella buona.
    InPlace,
    /// Una promessa non-strict non si e' potuta risolvere: l'entita' va scartata.
    Dropped,
    /// Almeno un campo era *multiple*: l'entita' e' sostituita da queste copie.
    ///
    /// La lista puo' contenere un solo elemento (promessa *multiple* con un valore solo): resta
    /// comunque `Expanded`, perche' il chiamante deve sostituire l'entita' con il contenuto della
    /// lista, non tenersi quella che aveva.
    Expanded(Vec<T>),
}

/// Fallimenti della risoluzione di un'entita'.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromisableError {
    /// Il valore risolto non era del tipo che il campo si aspetta.
    #[error("field '{field}': {source}")]
    Field {
        field: &'static str,
        #[source]
        source: BlockValueError,
    },
    /// Una promessa *strict* non si e' potuta risolvere.
    #[error(transparent)]
    Promise(#[from] PromiseError),
}

/// Cio' che un'entita' deve saper fare per essere risolvibile.
///
/// I nomi dei campi sono `&'static str` e non `String`: sono i nomi dei campi della struct, noti
/// a tempo di compilazione, e questo evita un'allocazione per campo pendente a ogni entita'.
pub trait PromisableFields: Clone {
    /// I campi ancora pendenti, con la loro promessa, in ordine stabile — l'ordine di
    /// dichiarazione dei campi. Da esso dipende l'ordine del prodotto cartesiano della fase 2.
    fn pending(&self) -> Vec<(&'static str, Promise)>;

    /// Assegna a `field` il valore risolto. `field` e' sempre uno dei nomi restituiti da
    /// [`PromisableFields::pending`]; l'implementazione converte il [`BlockValue`] nel tipo del
    /// campo e riporta un [`BlockValueError`] se non e' convertibile.
    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError>;
}

/// Risolve tutte le promesse di `entity` contro `map`. Vedi [`Fulfilled`] per l'esito e la nota
/// in testa al modulo per l'ordine delle due fasi.
pub fn fulfill_promises<T: PromisableFields>(
    entity: &mut T,
    map: &FlatPromiseMap,
) -> Result<Fulfilled<T>, PromisableError> {
    let mut multiples = Vec::new();

    // Fase 1: promesse normali, risolte sul posto.
    for (field, promise) in entity.pending() {
        if promise.multiple() {
            multiples.push((field, promise));
            continue;
        }
        match map.fulfill(&promise) {
            Ok(value) => assign(entity, field, value)?,
            Err(err) if promise.strict() => return Err(err.into()),
            Err(_) => return Ok(Fulfilled::Dropped),
        }
    }

    if multiples.is_empty() {
        return Ok(Fulfilled::InPlace);
    }

    // Fase 2: promesse multiple, una copia per valore.
    let mut expansions = vec![entity.clone()];
    for (field, promise) in multiples {
        let values = match map.fulfill(&promise) {
            Ok(v) => v,
            Err(err) if promise.strict() => return Err(err.into()),
            Err(_) => return Ok(Fulfilled::Dropped),
        };
        // `FlatPromiseMap::fulfill` su una promessa `multiple` restituisce sempre una `List` non
        // vuota; il ramo `other` copre solo il caso in cui quel contratto cambiasse.
        let values = match values {
            BlockValue::List(items) => items,
            other => vec![other],
        };
        let mut next = Vec::with_capacity(expansions.len() * values.len());
        for base in &expansions {
            for value in &values {
                let mut copy = base.clone();
                assign(&mut copy, field, value.clone())?;
                next.push(copy);
            }
        }
        expansions = next;
    }

    Ok(Fulfilled::Expanded(expansions))
}

/// Assegna un campo riportando il nome del campo nell'errore: senza questo, un
/// [`BlockValueError`] risalirebbe senza dire *quale* entita' e quale campo lo ha prodotto.
fn assign<T: PromisableFields>(
    entity: &mut T,
    field: &'static str,
    value: BlockValue,
) -> Result<(), PromisableError> {
    entity
        .resolve_field(field, value)
        .map_err(|source| PromisableError::Field { field, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entita' di prova con due campi promettibili e uno mai promesso, il minimo per esercitare
    /// entrambe le fasi e il prodotto cartesiano.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Investment {
        fund: Promised<String>,
        quantity: Promised<i64>,
        note: String,
    }

    impl Investment {
        fn new(fund: Promised<String>, quantity: Promised<i64>) -> Self {
            Investment { fund, quantity, note: "fissa".into() }
        }

        fn resolved(fund: &str, quantity: i64) -> Self {
            Investment::new(Promised::Resolved(fund.into()), Promised::Resolved(quantity))
        }
    }

    impl PromisableFields for Investment {
        fn pending(&self) -> Vec<(&'static str, Promise)> {
            let mut out = Vec::new();
            if let Some(p) = self.fund.pending() {
                out.push(("fund", p.clone()));
            }
            if let Some(p) = self.quantity.pending() {
                out.push(("quantity", p.clone()));
            }
            out
        }

        fn resolve_field(
            &mut self,
            field: &'static str,
            value: BlockValue,
        ) -> Result<(), BlockValueError> {
            match field {
                "fund" => self.fund = Promised::Resolved(value.str_or_fail(field)?.to_string()),
                "quantity" => self.quantity = Promised::Resolved(value.int_or_fail(field)?),
                other => return Err(BlockValueError::MissingField { field: other.to_string() }),
            }
            Ok(())
        }
    }

    fn flat_map(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
        pairs.into_iter().collect()
    }

    fn pending(raw: &str) -> Promised<String> {
        Promised::Pending(Promise::new(raw))
    }

    fn pending_i64(raw: &str) -> Promised<i64> {
        Promised::Pending(Promise::new(raw))
    }

    mod promised_field {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn distinguishes_resolved_from_pending() {
            let resolved: Promised<i64> = Promised::Resolved(3);
            assert!(resolved.is_resolved());
            assert!(!resolved.is_pending());
            assert_eq!(resolved.resolved(), Some(&3));
            assert_eq!(resolved.pending(), None);

            let pending_field = pending_i64("x!");
            assert!(pending_field.is_pending());
            assert!(!pending_field.is_resolved());
            assert_eq!(pending_field.resolved(), None);
            assert_eq!(pending_field.pending(), Some(&Promise::new("x!")));
        }

        #[test]
        fn into_resolved_consumes_the_field() {
            assert_eq!(Promised::Resolved("x".to_string()).into_resolved(), Some("x".to_string()));
            assert_eq!(pending("x").into_resolved(), None);
        }

        #[test]
        fn map_transforms_only_the_resolved_value() {
            assert_eq!(Promised::Resolved(2_i64).map(|v| v * 2), Promised::Resolved(4));
            assert_eq!(pending_i64("x").map(|v| v * 2), Promised::Pending(Promise::new("x")));
        }

        #[test]
        fn is_built_from_a_promise() {
            let field: Promised<i64> = Promise::new("x[]").into();
            assert_eq!(field, Promised::Pending(Promise::new("x[]")));
        }

        #[test]
        fn serializes_as_value_or_as_promise() {
            assert_eq!(serde_json::to_string(&Promised::Resolved(3_i64)).unwrap(), "3");
            assert_eq!(serde_json::to_string(&pending_i64("fund[]!")).unwrap(), "\"fund[]!\"");
        }
    }

    mod no_promise {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_already_resolved_entity_stays_in_place_and_intact() {
            let mut entity = Investment::resolved("Acme", 10);
            let before = entity.clone();
            assert_eq!(fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap(), Fulfilled::InPlace);
            assert_eq!(entity, before);
        }
    }

    mod in_place_phase {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn resolves_a_promised_field() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(10));
            let map = flat_map(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(entity.fund, Promised::Resolved("Acme".into()));
        }

        #[test]
        fn resolves_multiple_fields_in_the_same_pass() {
            let mut entity = Investment::new(pending("fund"), pending_i64("qty"));
            let map = flat_map(vec![("fund", BlockValue::from("Acme")), ("qty", BlockValue::Int(7))]);
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(entity, Investment::resolved("Acme", 7));
        }

        #[test]
        fn does_not_touch_unpromised_fields() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(10));
            let map = flat_map(vec![("fund", BlockValue::from("Acme"))]);
            fulfill_promises(&mut entity, &map).unwrap();
            assert_eq!(entity.note, "fissa");
            assert_eq!(entity.quantity, Promised::Resolved(10));
        }

        #[test]
        fn on_a_list_takes_the_last_value() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(1));
            let map = flat_map(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::from("Vecchio"), BlockValue::from("Nuovo")]),
            )]);
            fulfill_promises(&mut entity, &map).unwrap();
            assert_eq!(entity.fund, Promised::Resolved("Nuovo".into()));
        }
    }

    mod unresolvable_promise {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn non_strict_makes_the_entity_disappear() {
            let mut entity = Investment::new(pending("assente"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap(),
                Fulfilled::Dropped
            );
        }

        #[test]
        fn strict_is_an_error() {
            let mut entity = Investment::new(pending("assente!"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap_err(),
                PromisableError::Promise(PromiseError::Unresolved { id: "assente".into() })
            );
        }

        /// La politica sui riferimenti pendenti chiude il cerchio qui: una promessa sopravvissuta
        /// all'appiattimento (`promise_resolution`) diventa un drop o un errore a seconda di
        /// `strict`, non un errore di appiattimento.
        #[test]
        fn a_promise_surviving_flattening_behaves_like_a_missing_id() {
            let map = flat_map(vec![("fund", BlockValue::Promise(Promise::new("nowhere")))]);

            let mut non_strict = Investment::new(pending("fund"), Promised::Resolved(1));
            assert_eq!(fulfill_promises(&mut non_strict, &map).unwrap(), Fulfilled::Dropped);

            let mut strict = Investment::new(pending("fund!"), Promised::Resolved(1));
            assert!(fulfill_promises(&mut strict, &map).is_err());
        }

        #[test]
        fn a_null_value_counts_as_missing() {
            let map = flat_map(vec![("fund", BlockValue::Null)]);
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(1));
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::Dropped);
        }

        #[test]
        fn an_unresolvable_non_strict_multiple_makes_the_entity_disappear() {
            let mut entity = Investment::new(pending("assente[]"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap(),
                Fulfilled::Dropped
            );
        }

        #[test]
        fn an_unresolvable_strict_multiple_is_an_error() {
            let mut entity = Investment::new(pending("assente[]!"), Promised::Resolved(1));
            assert!(fulfill_promises(&mut entity, &FlatPromiseMap::new()).is_err());
        }

        /// Il drop vince sull'espansione: se un campo normale non si risolve, la fase 2 non parte
        /// nemmeno.
        #[test]
        fn an_unresolvable_normal_field_prevents_the_expansion() {
            let mut entity = Investment::new(pending("assente"), pending_i64("qty[]"));
            let map = flat_map(vec![(
                "qty",
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]),
            )]);
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::Dropped);
        }
    }

    mod expansion_phase {
        use super::*;
        use pretty_assertions::assert_eq;

        fn expansions(outcome: Fulfilled<Investment>) -> Vec<Investment> {
            match outcome {
                Fulfilled::Expanded(v) => v,
                other => panic!("attesa un'espansione, trovato {other:?}"),
            }
        }

        #[test]
        fn one_copy_per_value() {
            let mut entity = Investment::new(pending("fund[]"), Promised::Resolved(1));
            let map = flat_map(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::from("A"), BlockValue::from("B"), BlockValue::from("C")]),
            )]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            let names: Vec<&str> = copies.iter().filter_map(|c| c.fund.resolved()).map(String::as_str).collect();
            assert_eq!(names, vec!["A", "B", "C"]);
        }

        /// Un solo valore resta comunque un'espansione, non un `InPlace`: il chiamante deve
        /// sostituire l'entita' con il contenuto della lista in entrambi i casi.
        #[test]
        fn a_single_value_still_produces_an_expansion() {
            let mut entity = Investment::new(pending("fund[]"), Promised::Resolved(1));
            let map = flat_map(vec![("fund", BlockValue::from("A"))]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            assert_eq!(copies, vec![Investment::resolved("A", 1)]);
        }

        #[test]
        fn two_multiple_fields_give_the_cartesian_product() {
            let mut entity = Investment::new(pending("fund[]"), pending_i64("qty[]"));
            let map = flat_map(vec![
                ("fund", BlockValue::List(vec![BlockValue::from("A"), BlockValue::from("B")])),
                ("qty", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)])),
            ]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            assert_eq!(copies.len(), 6);
            let pairs: Vec<(&str, i64)> = copies
                .iter()
                .filter_map(|c| Some((c.fund.resolved()?.as_str(), *c.quantity.resolved()?)))
                .collect();
            // Il campo che compare per primo in `pending` varia piu' lentamente.
            assert_eq!(
                pairs,
                vec![("A", 1), ("A", 2), ("A", 3), ("B", 1), ("B", 2), ("B", 3)]
            );
        }

        /// L'ordine delle due fasi, reso osservabile: il campo normale e' gia' risolto in *ogni*
        /// copia, quindi e' stato risolto una volta sola, prima dell'espansione.
        #[test]
        fn the_copies_already_carry_the_fields_resolved_in_the_first_phase() {
            let mut entity = Investment::new(pending("fund"), pending_i64("qty[]"));
            let map = flat_map(vec![
                ("fund", BlockValue::from("Acme")),
                ("qty", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])),
            ]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            assert_eq!(copies, vec![Investment::resolved("Acme", 1), Investment::resolved("Acme", 2)]);
        }

        #[test]
        fn a_multiple_on_a_scalar_value_produces_a_single_copy() {
            let mut entity = Investment::new(Promised::Resolved("Acme".into()), pending_i64("qty[]"));
            let map = flat_map(vec![("qty", BlockValue::Int(9))]);
            assert_eq!(
                expansions(fulfill_promises(&mut entity, &map).unwrap()),
                vec![Investment::resolved("Acme", 9)]
            );
        }

        #[test]
        fn the_copies_are_independent_from_each_other() {
            let mut entity = Investment::new(pending("fund[]"), Promised::Resolved(1));
            let map = flat_map(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::from("A"), BlockValue::from("B")]),
            )]);
            let mut copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            copies[0].note = "cambiata".into();
            assert_eq!(copies[1].note, "fissa");
        }
    }

    mod type_errors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_value_of_the_wrong_type_names_the_field() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(1));
            let map = flat_map(vec![("fund", BlockValue::Int(3))]);
            let err = fulfill_promises(&mut entity, &map).unwrap_err();
            assert_eq!(
                err,
                PromisableError::Field {
                    field: "fund",
                    source: BlockValueError::TypeMismatch {
                        field: "fund".into(),
                        expected: "str",
                        found: "int",
                    },
                }
            );
            assert_eq!(err.to_string(), "field 'fund': field 'fund' expected str, found int");
        }

        #[test]
        fn a_type_error_during_expansion_stops_everything() {
            let mut entity = Investment::new(Promised::Resolved("Acme".into()), pending_i64("qty[]"));
            let map = flat_map(vec![(
                "qty",
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::from("non un numero")]),
            )]);
            let err = fulfill_promises(&mut entity, &map).unwrap_err();
            assert!(matches!(err, PromisableError::Field { field: "quantity", .. }), "{err:?}");
        }
    }
}
