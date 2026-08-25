//! `BlockValue`: l'unico tipo di valore che puo' stare in `metadata` o in `content` di un blocco.
//!
//! In `freeports_core` questi due campi erano `dict`/`Any` Python: qualunque oggetto poteva
//! finirci, il che rendeva impossibile serializzare senza un modulo `serialization.py` ad hoc e
//! costringeva ogni consumatore a un `isinstance` difensivo. Qui sono un enum chiuso
//! (`PLAN.md` §4.1, decisione D1): serde funziona per derivazione, il compilatore verifica
//! l'esaustivita' dei `match`, e gli accessori tipizzati restituiscono `Result` invece di
//! lasciare `unwrap` sparsi nei deserializzatori (`PLAN.md` §4.1, ultimo punto).
//!
//! **Due conseguenze volute del passaggio a un enum ordinato**, entrambe fissate dai test qui
//! sotto:
//!
//! - `BlockValue` e' `Ord`, quindi usabile come elemento di [`BTreeSet`]: i contenitori
//!   `Set`/`Map` sono ordinati, e hash e serializzazione diventano deterministici a parita' di
//!   contenuto, indipendentemente dall'ordine di inserimento.
//! - Sparisce il `__hash__` del riferimento, che normalizzava `metadata` *mutandolo* (liste e
//!   insiemi convertiti in `frozenset`) per riuscire a calcolare l'hash — e che, essendo `__eq__`
//!   definito come uguaglianza di hash, faceva mutare i blocchi anche solo confrontandoli
//!   (`PLAN.md` §4.1 e decisione D3). Qui `Hash` e' derivato e non tocca nulla.
//!
//! **Limite noto**: `Float(NaN)` e' un valore legittimo in memoria (`OrderedFloat` lo rende
//! `Eq`/`Ord`/`Hash`) ma non sopravvive a un giro in JSON, perche' JSON non ha un `NaN`. Vedi
//! `tests::serde_roundtrip::nan_does_not_survive_json`.

use std::collections::{BTreeMap, BTreeSet};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
use crate::commons::date::Date;
use crate::core::promise::Promise;

/// Valore eterogeneo ma tipizzato, ammesso in `metadata` e `content` di un blocco.
///
/// La rappresentazione serde e' *adiacente* (`{"kind": "...", "v": ...}`): l'alternativa
/// non-taggata costringerebbe il deserializzatore a indovinare fra `Int` e `Float`, o fra `Str` e
/// `Promise`, e le due ambiguita' sono reali nei dati dei repo formati.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum BlockValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(OrderedFloat<f64>),
    Str(String),
    Date(Date),
    Currency(Currency),
    SfdrArticle(SfdrArticle),
    FinancialInstrument(FinancialInstrument),
    Promise(Promise),
    List(Vec<BlockValue>),
    Set(BTreeSet<BlockValue>),
    Map(BTreeMap<String, BlockValue>),
}

/// Fallimenti nella lettura tipizzata di un [`BlockValue`].
///
/// `field` e' sempre il nome che il chiamante stava cercando di leggere, non un indice interno:
/// serve a rendere il messaggio utile all'autore di un repo formati, che vede il nome del campo
/// che ha scritto lui nel CSV.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BlockValueError {
    #[error("field '{field}' expected {expected}, found {found}")]
    TypeMismatch { field: String, expected: &'static str, found: &'static str },
    #[error("missing field '{field}'")]
    MissingField { field: String },
    #[error("cannot read field '{field}': the value is a {found}, not a map")]
    NotAMap { field: String, found: &'static str },
}

/// Genera la coppia di accessori per una variante: `as_*` che restituisce `Option`, e
/// `*_or_fail` che restituisce `Result` con il nome del campo nel messaggio d'errore.
///
/// Sono dodici varianti con la stessa identica forma: scriverle a mano significherebbe
/// settantadue righe che differiscono per un pattern e una stringa.
macro_rules! typed_accessor {
    ($as_fn:ident, $or_fail:ident, $ret:ty, $expected:literal, $pat:pat => $val:expr) => {
        #[doc = concat!("Il valore se questo `BlockValue` e' un `", $expected, "`, altrimenti `None`.")]
        pub fn $as_fn(&self) -> Option<$ret> {
            match self {
                $pat => Some($val),
                _ => None,
            }
        }

        #[doc = concat!("Come sopra, ma un tipo diverso da `", $expected, "` e' un errore che")]
        #[doc = "riporta `field` — il nome sotto cui il chiamante si aspettava questo valore."]
        pub fn $or_fail(&self, field: &str) -> Result<$ret, BlockValueError> {
            self.$as_fn().ok_or_else(|| BlockValueError::TypeMismatch {
                field: field.to_string(),
                expected: $expected,
                found: self.kind(),
            })
        }
    };
}

impl BlockValue {
    /// Il nome della variante, identico al valore del tag `kind` usato da serde. E' quello che
    /// finisce nei messaggi d'errore, quindi la coincidenza fra i due non e' casuale: e'
    /// verificata da `tests::serde_roundtrip::kind_matches_the_serde_tag`.
    pub fn kind(&self) -> &'static str {
        match self {
            BlockValue::Null => "null",
            BlockValue::Bool(_) => "bool",
            BlockValue::Int(_) => "int",
            BlockValue::Float(_) => "float",
            BlockValue::Str(_) => "str",
            BlockValue::Date(_) => "date",
            BlockValue::Currency(_) => "currency",
            BlockValue::SfdrArticle(_) => "sfdr_article",
            BlockValue::FinancialInstrument(_) => "financial_instrument",
            BlockValue::Promise(_) => "promise",
            BlockValue::List(_) => "list",
            BlockValue::Set(_) => "set",
            BlockValue::Map(_) => "map",
        }
    }

    /// `true` solo per [`BlockValue::Null`]. Un `Null` in una mappa di risoluzione conta come
    /// valore *assente*, non come valore nullo — vedi `crate::core::promise_resolution`.
    pub fn is_null(&self) -> bool {
        matches!(self, BlockValue::Null)
    }

    /// `true` se il valore e' ancora una promessa da risolvere.
    pub fn is_promise(&self) -> bool {
        matches!(self, BlockValue::Promise(_))
    }

    typed_accessor!(as_bool, bool_or_fail, bool, "bool", BlockValue::Bool(v) => *v);
    typed_accessor!(as_int, int_or_fail, i64, "int", BlockValue::Int(v) => *v);
    typed_accessor!(as_float, float_or_fail, f64, "float", BlockValue::Float(v) => v.into_inner());
    typed_accessor!(as_str, str_or_fail, &str, "str", BlockValue::Str(v) => v.as_str());
    typed_accessor!(as_date, date_or_fail, Date, "date", BlockValue::Date(v) => *v);
    typed_accessor!(as_currency, currency_or_fail, Currency, "currency", BlockValue::Currency(v) => *v);
    typed_accessor!(as_sfdr_article, sfdr_article_or_fail, SfdrArticle, "sfdr_article", BlockValue::SfdrArticle(v) => *v);
    typed_accessor!(
        as_financial_instrument,
        financial_instrument_or_fail,
        FinancialInstrument,
        "financial_instrument",
        BlockValue::FinancialInstrument(v) => *v
    );
    typed_accessor!(as_promise, promise_or_fail, &Promise, "promise", BlockValue::Promise(v) => v);
    typed_accessor!(as_list, list_or_fail, &[BlockValue], "list", BlockValue::List(v) => v.as_slice());
    typed_accessor!(as_set, set_or_fail, &BTreeSet<BlockValue>, "set", BlockValue::Set(v) => v);
    typed_accessor!(as_map, map_or_fail, &BTreeMap<String, BlockValue>, "map", BlockValue::Map(v) => v);

    /// Legge `field` da un [`BlockValue::Map`]. `None` sia se il valore non e' una mappa sia se
    /// la chiave manca: e' l'accessore "morbido", per quando le due cose sono equivalenti per il
    /// chiamante.
    pub fn get(&self, field: &str) -> Option<&BlockValue> {
        self.as_map()?.get(field)
    }

    /// Come [`BlockValue::get`], ma distingue i due modi di fallire: [`BlockValueError::NotAMap`]
    /// se il valore non e' una mappa, [`BlockValueError::MissingField`] se la chiave manca.
    pub fn get_or_fail(&self, field: &str) -> Result<&BlockValue, BlockValueError> {
        let map = self.as_map().ok_or_else(|| BlockValueError::NotAMap {
            field: field.to_string(),
            found: self.kind(),
        })?;
        map.get(field).ok_or_else(|| BlockValueError::MissingField { field: field.to_string() })
    }
}

impl From<bool> for BlockValue {
    fn from(v: bool) -> Self {
        BlockValue::Bool(v)
    }
}

impl From<i64> for BlockValue {
    fn from(v: i64) -> Self {
        BlockValue::Int(v)
    }
}

impl From<f64> for BlockValue {
    fn from(v: f64) -> Self {
        BlockValue::Float(OrderedFloat(v))
    }
}

impl From<String> for BlockValue {
    fn from(v: String) -> Self {
        BlockValue::Str(v)
    }
}

impl From<&str> for BlockValue {
    fn from(v: &str) -> Self {
        BlockValue::Str(v.to_string())
    }
}

impl From<Date> for BlockValue {
    fn from(v: Date) -> Self {
        BlockValue::Date(v)
    }
}

impl From<Currency> for BlockValue {
    fn from(v: Currency) -> Self {
        BlockValue::Currency(v)
    }
}

impl From<SfdrArticle> for BlockValue {
    fn from(v: SfdrArticle) -> Self {
        BlockValue::SfdrArticle(v)
    }
}

impl From<FinancialInstrument> for BlockValue {
    fn from(v: FinancialInstrument) -> Self {
        BlockValue::FinancialInstrument(v)
    }
}

impl From<Promise> for BlockValue {
    fn from(v: Promise) -> Self {
        BlockValue::Promise(v)
    }
}

impl From<Vec<BlockValue>> for BlockValue {
    fn from(v: Vec<BlockValue>) -> Self {
        BlockValue::List(v)
    }
}

impl From<BTreeSet<BlockValue>> for BlockValue {
    fn from(v: BTreeSet<BlockValue>) -> Self {
        BlockValue::Set(v)
    }
}

impl From<BTreeMap<String, BlockValue>> for BlockValue {
    fn from(v: BTreeMap<String, BlockValue>) -> Self {
        BlockValue::Map(v)
    }
}

/// `None` diventa [`BlockValue::Null`]: e' il modo naturale di portare un campo opzionale di un
/// deserializzatore dentro un blocco senza un `match` a ogni chiamata.
impl<T: Into<BlockValue>> From<Option<T>> for BlockValue {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => BlockValue::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un esemplare per ogni variante, nell'ordine di dichiarazione dell'enum. Le prove di
    /// esaustivita' (`kind`, accessori, serde) iterano su questa lista, cosi' che aggiungere una
    /// variante senza aggiornarla faccia fallire `covers_all_variants`.
    fn one_of_each() -> Vec<BlockValue> {
        vec![
            BlockValue::Null,
            BlockValue::Bool(true),
            BlockValue::Int(-7),
            BlockValue::Float(OrderedFloat(1.5)),
            BlockValue::Str("testo".into()),
            BlockValue::Date(Date::new(2024, 2, 29).unwrap()),
            BlockValue::Currency(Currency::EUR),
            BlockValue::SfdrArticle(SfdrArticle::Art8),
            BlockValue::FinancialInstrument(FinancialInstrument::BOND),
            BlockValue::Promise(Promise::new("fund[]!")),
            BlockValue::List(vec![BlockValue::Int(1), BlockValue::Str("due".into())]),
            BlockValue::Set(BTreeSet::from([BlockValue::Int(1), BlockValue::Int(2)])),
            BlockValue::Map(BTreeMap::from([("a".to_string(), BlockValue::Int(1))])),
        ]
    }

    mod kind {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn covers_all_variants() {
            // Il `match` esaustivo in `kind` e' la garanzia lato compilatore; questo test e' la
            // garanzia che `one_of_each` — usato da tutti gli altri test — resti completo.
            let kinds: Vec<&str> = one_of_each().iter().map(BlockValue::kind).collect();
            assert_eq!(
                kinds,
                vec![
                    "null", "bool", "int", "float", "str", "date", "currency", "sfdr_article",
                    "financial_instrument", "promise", "list", "set", "map"
                ]
            );
        }

        #[test]
        fn the_names_are_all_distinct() {
            let kinds: BTreeSet<&str> = one_of_each().iter().map(BlockValue::kind).collect();
            assert_eq!(kinds.len(), one_of_each().len());
        }

        #[test]
        fn is_null_and_is_promise_recognize_only_their_own_variant() {
            for v in one_of_each() {
                assert_eq!(v.is_null(), v.kind() == "null", "{v:?}");
                assert_eq!(v.is_promise(), v.kind() == "promise", "{v:?}");
            }
        }
    }

    mod accessors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_accessor_reads_its_own_variant() {
            assert_eq!(BlockValue::Bool(true).as_bool(), Some(true));
            assert_eq!(BlockValue::Int(-7).as_int(), Some(-7));
            assert_eq!(BlockValue::from(1.5).as_float(), Some(1.5));
            assert_eq!(BlockValue::from("x").as_str(), Some("x"));
            let date = Date::new(2024, 1, 2).unwrap();
            assert_eq!(BlockValue::from(date).as_date(), Some(date));
            assert_eq!(BlockValue::from(Currency::EUR).as_currency(), Some(Currency::EUR));
            assert_eq!(BlockValue::from(SfdrArticle::Art9).as_sfdr_article(), Some(SfdrArticle::Art9));
            assert_eq!(
                BlockValue::from(FinancialInstrument::EQUITY).as_financial_instrument(),
                Some(FinancialInstrument::EQUITY)
            );
            let promise = Promise::new("p");
            assert_eq!(BlockValue::from(promise.clone()).as_promise(), Some(&promise));
            assert_eq!(BlockValue::List(vec![BlockValue::Int(1)]).as_list(), Some(&[BlockValue::Int(1)][..]));
            let set = BTreeSet::from([BlockValue::Int(1)]);
            assert_eq!(BlockValue::from(set.clone()).as_set(), Some(&set));
            let map = BTreeMap::from([("k".to_string(), BlockValue::Int(1))]);
            assert_eq!(BlockValue::from(map.clone()).as_map(), Some(&map));
        }

        /// Ogni accessore deve dire `None` su *tutte* le altre dodici varianti, non solo su
        /// quella che verrebbe in mente scrivendo il test a mano.
        #[test]
        fn each_accessor_rejects_all_other_variants() {
            /// Nome della variante che l'accessore accetta, e l'accessore stesso ridotto a
            /// un predicato — l'unico modo di metterli tutti nella stessa lista, visto che i
            /// dodici hanno tipi di ritorno diversi.
            type Accessor = (&'static str, fn(&BlockValue) -> bool);
            let checks: Vec<Accessor> = vec![
                ("bool", |v| v.as_bool().is_some()),
                ("int", |v| v.as_int().is_some()),
                ("float", |v| v.as_float().is_some()),
                ("str", |v| v.as_str().is_some()),
                ("date", |v| v.as_date().is_some()),
                ("currency", |v| v.as_currency().is_some()),
                ("sfdr_article", |v| v.as_sfdr_article().is_some()),
                ("financial_instrument", |v| v.as_financial_instrument().is_some()),
                ("promise", |v| v.as_promise().is_some()),
                ("list", |v| v.as_list().is_some()),
                ("set", |v| v.as_set().is_some()),
                ("map", |v| v.as_map().is_some()),
            ];
            for (expected_kind, accessor) in checks {
                for value in one_of_each() {
                    assert_eq!(
                        accessor(&value),
                        value.kind() == expected_kind,
                        "accessor {expected_kind} on value {value:?}"
                    );
                }
            }
        }

        #[test]
        fn or_fail_reports_expected_and_found_field() {
            let err = BlockValue::Int(1).str_or_fail("fund_name").unwrap_err();
            assert_eq!(
                err,
                BlockValueError::TypeMismatch {
                    field: "fund_name".into(),
                    expected: "str",
                    found: "int",
                }
            );
            assert_eq!(err.to_string(), "field 'fund_name' expected str, found int");
        }

        #[test]
        fn or_fail_succeeds_exactly_when_as_succeeds() {
            for value in one_of_each() {
                assert_eq!(value.as_int().is_some(), value.int_or_fail("f").is_ok(), "{value:?}");
                assert_eq!(value.as_str().is_some(), value.str_or_fail("f").is_ok(), "{value:?}");
                assert_eq!(value.as_promise().is_some(), value.promise_or_fail("f").is_ok(), "{value:?}");
            }
        }

        /// `Null` non e' un jolly: non soddisfa nessun accessore tipizzato. E' la ragione per cui
        /// esiste `is_null` separato.
        #[test]
        fn null_satisfies_no_accessor() {
            let null = BlockValue::Null;
            assert!(null.as_bool().is_none());
            assert!(null.as_int().is_none());
            assert!(null.as_str().is_none());
            assert!(null.as_map().is_none());
            assert!(null.as_list().is_none());
        }

        /// `Int` e `Float` sono varianti distinte: nessuna conversione implicita fra le due, cosi'
        /// che un CSV che dichiara un intero non passi silenziosamente per un float e viceversa.
        #[test]
        fn int_and_float_do_not_convert_into_each_other() {
            assert!(BlockValue::Int(1).as_float().is_none());
            assert!(BlockValue::from(1.0).as_int().is_none());
        }
    }

    mod field_reading {
        use super::*;
        use pretty_assertions::assert_eq;

        fn map_value() -> BlockValue {
            BlockValue::Map(BTreeMap::from([
                ("nome".to_string(), BlockValue::from("Acme")),
                ("valore".to_string(), BlockValue::Int(3)),
            ]))
        }

        #[test]
        fn get_reads_a_present_key() {
            assert_eq!(map_value().get("nome"), Some(&BlockValue::from("Acme")));
        }

        #[test]
        fn get_returns_none_for_missing_key_or_non_map() {
            assert_eq!(map_value().get("assente"), None);
            assert_eq!(BlockValue::Int(1).get("nome"), None);
        }

        #[test]
        fn get_or_fail_distinguishes_missing_key_from_non_map_value() {
            assert_eq!(
                map_value().get_or_fail("assente").unwrap_err(),
                BlockValueError::MissingField { field: "assente".into() }
            );
            assert_eq!(
                BlockValue::Int(1).get_or_fail("nome").unwrap_err(),
                BlockValueError::NotAMap { field: "nome".into(), found: "int" }
            );
        }

        #[test]
        fn get_or_fail_succeeds_exactly_when_get_succeeds() {
            let values = [map_value(), BlockValue::Int(1), BlockValue::Map(BTreeMap::new()), BlockValue::Null];
            for v in values {
                for key in ["nome", "assente", ""] {
                    assert_eq!(v.get(key).is_some(), v.get_or_fail(key).is_ok(), "{v:?} / {key}");
                }
            }
        }

        #[test]
        fn error_messages_name_the_field() {
            assert_eq!(
                map_value().get_or_fail("assente").unwrap_err().to_string(),
                "missing field 'assente'"
            );
            assert_eq!(
                BlockValue::Null.get_or_fail("nome").unwrap_err().to_string(),
                "cannot read field 'nome': the value is a null, not a map"
            );
        }
    }

    mod conversions {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn from_covers_scalar_types() {
            assert_eq!(BlockValue::from(true), BlockValue::Bool(true));
            assert_eq!(BlockValue::from(3_i64), BlockValue::Int(3));
            assert_eq!(BlockValue::from(3.5_f64), BlockValue::Float(OrderedFloat(3.5)));
            assert_eq!(BlockValue::from("x"), BlockValue::Str("x".into()));
            assert_eq!(BlockValue::from("x".to_string()), BlockValue::Str("x".into()));
        }

        #[test]
        fn option_none_becomes_null() {
            let absent: Option<i64> = None;
            assert_eq!(BlockValue::from(absent), BlockValue::Null);
            assert_eq!(BlockValue::from(Some(4_i64)), BlockValue::Int(4));
            let absent_str: Option<&str> = None;
            assert_eq!(BlockValue::from(absent_str), BlockValue::Null);
        }

        #[test]
        fn from_covers_containers() {
            assert_eq!(
                BlockValue::from(vec![BlockValue::Int(1)]),
                BlockValue::List(vec![BlockValue::Int(1)])
            );
            assert_eq!(
                BlockValue::from(BTreeSet::from([BlockValue::Int(1)])),
                BlockValue::Set(BTreeSet::from([BlockValue::Int(1)]))
            );
            assert_eq!(
                BlockValue::from(BTreeMap::from([("a".to_string(), BlockValue::Int(1))])),
                BlockValue::Map(BTreeMap::from([("a".to_string(), BlockValue::Int(1))]))
            );
        }
    }

    mod order_and_hash {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of(v: &BlockValue) -> u64 {
            let mut h = DefaultHasher::new();
            v.hash(&mut h);
            h.finish()
        }

        #[test]
        fn order_among_variants_follows_declaration() {
            let mut values = one_of_each();
            let expected = values.clone();
            values.reverse();
            values.sort();
            assert_eq!(values, expected);
        }

        #[test]
        fn within_a_variant_orders_by_content() {
            assert!(BlockValue::Int(1) < BlockValue::Int(2));
            assert!(BlockValue::from("a") < BlockValue::from("b"));
            assert!(BlockValue::from(1.0) < BlockValue::from(2.0));
        }

        /// L'invariante che rende `Set`/`Map` utilizzabili: l'ordine di inserimento non cambia
        /// ne' il valore ne' l'hash. E' cio' che nel riferimento richiedeva la mutazione di
        /// `metadata` in `__hash__` (`PLAN.md` D3) e che qui viene gratis dai contenitori ordinati.
        #[test]
        fn insertion_order_does_not_change_hash_or_equality() {
            let forward = BlockValue::Set(BTreeSet::from([
                BlockValue::Int(1),
                BlockValue::from("b"),
                BlockValue::Int(2),
            ]));
            let mut reversed = BTreeSet::new();
            reversed.insert(BlockValue::from("b"));
            reversed.insert(BlockValue::Int(2));
            reversed.insert(BlockValue::Int(1));
            let reversed = BlockValue::Set(reversed);
            assert_eq!(forward, reversed);
            assert_eq!(hash_of(&forward), hash_of(&reversed));

            let mut map_a = BTreeMap::new();
            map_a.insert("x".to_string(), BlockValue::Int(1));
            map_a.insert("y".to_string(), BlockValue::Int(2));
            let mut map_b = BTreeMap::new();
            map_b.insert("y".to_string(), BlockValue::Int(2));
            map_b.insert("x".to_string(), BlockValue::Int(1));
            assert_eq!(hash_of(&BlockValue::from(map_a)), hash_of(&BlockValue::from(map_b)));
        }

        /// L'ordine di una `List` invece conta: e' una sequenza, non un insieme.
        #[test]
        fn list_order_matters() {
            let a = BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]);
            let b = BlockValue::List(vec![BlockValue::Int(2), BlockValue::Int(1)]);
            assert_ne!(a, b);
        }

        #[test]
        fn nested_values_remain_comparable_and_hashable() {
            let nested = BlockValue::Map(BTreeMap::from([(
                "dentro".to_string(),
                BlockValue::List(vec![BlockValue::Set(BTreeSet::from([BlockValue::Int(1)]))]),
            )]));
            assert_eq!(hash_of(&nested), hash_of(&nested.clone()));
            assert_eq!(nested.cmp(&nested.clone()), std::cmp::Ordering::Equal);
        }

        /// Un `BlockValue` puo' essere elemento di un insieme e chiave di una mappa ordinata: e'
        /// la ragione per cui l'enum e' `Ord` e non solo `Eq`.
        #[test]
        fn usable_as_a_set_element() {
            let set: BTreeSet<BlockValue> = one_of_each().into_iter().collect();
            assert_eq!(set.len(), one_of_each().len());
            assert!(set.contains(&BlockValue::Null));
        }
    }

    mod serde_roundtrip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_variant_survives_json() {
            for value in one_of_each() {
                let json = serde_json::to_string(&value).unwrap();
                let back: BlockValue = serde_json::from_str(&json).unwrap();
                assert_eq!(back, value, "json: {json}");
            }
        }

        #[test]
        fn kind_matches_the_serde_tag() {
            for value in one_of_each() {
                let json: serde_json::Value = serde_json::to_value(&value).unwrap();
                assert_eq!(json["kind"], serde_json::Value::from(value.kind()), "{value:?}");
            }
        }

        #[test]
        fn the_shape_is_adjacently_tagged() {
            assert_eq!(serde_json::to_string(&BlockValue::Int(3)).unwrap(), r#"{"kind":"int","v":3}"#);
            assert_eq!(serde_json::to_string(&BlockValue::Null).unwrap(), r#"{"kind":"null"}"#);
            assert_eq!(
                serde_json::to_string(&BlockValue::from(Promise::new("fund[]!"))).unwrap(),
                r#"{"kind":"promise","v":"fund[]!"}"#
            );
            assert_eq!(
                serde_json::to_string(&BlockValue::from(Currency::EUR)).unwrap(),
                r#"{"kind":"currency","v":"EUR"}"#
            );
            assert_eq!(
                serde_json::to_string(&BlockValue::from(Date::new(2024, 3, 1).unwrap())).unwrap(),
                r#"{"kind":"date","v":"2024-03-01"}"#
            );
        }

        #[test]
        fn deeply_nested_values_survive() {
            let nested = BlockValue::Map(BTreeMap::from([
                (
                    "lista".to_string(),
                    BlockValue::List(vec![
                        BlockValue::from(Promise::new("p!")),
                        BlockValue::Map(BTreeMap::from([("dentro".to_string(), BlockValue::Null)])),
                    ]),
                ),
                ("insieme".to_string(), BlockValue::Set(BTreeSet::from([BlockValue::from("a")]))),
            ]));
            let json = serde_json::to_string(&nested).unwrap();
            assert_eq!(serde_json::from_str::<BlockValue>(&json).unwrap(), nested);
        }

        #[test]
        fn an_unknown_kind_is_an_error() {
            assert!(serde_json::from_str::<BlockValue>(r#"{"kind":"decimal","v":1}"#).is_err());
        }

        #[test]
        fn wrong_type_content_is_an_error() {
            assert!(serde_json::from_str::<BlockValue>(r#"{"kind":"int","v":"tre"}"#).is_err());
            assert!(serde_json::from_str::<BlockValue>(r#"{"kind":"currency","v":"EURO"}"#).is_err());
        }

        /// Limite noto e accettato: JSON non ha un `NaN`, quindi un `Float(NaN)` — legittimo in
        /// memoria grazie a `OrderedFloat` — non torna indietro. Il test non impone *dove* il giro
        /// si rompe (serializzazione o rilettura), solo che non produca silenziosamente un valore
        /// diverso.
        #[test]
        fn nan_does_not_survive_json() {
            let nan = BlockValue::from(f64::NAN);
            match serde_json::to_string(&nan) {
                Err(_) => {}
                Ok(json) => assert!(
                    serde_json::from_str::<BlockValue>(&json).is_err(),
                    "NaN came back from {json}"
                ),
            }
        }
    }
}
