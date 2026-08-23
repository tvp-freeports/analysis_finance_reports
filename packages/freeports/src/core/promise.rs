//! `Promise`: riferimento a un valore che non e' ancora noto quando il blocco viene costruito.
//!
//! Una pagina puo' produrre un valore che dipende da qualcosa scritto su un'altra pagina dello
//! stesso documento (il nome del fondo su una pagina di intestazione, la valuta dichiarata una
//! volta sola, ...). Invece di ordinare le pagine o fare due passate, il deserializzatore
//! deposita una `Promise` al posto del valore; a documento finito la mappa delle promesse viene
//! appiattita ([`crate::core::promise_resolution`]) e le entita' vengono risolte
//! ([`crate::core::promisable`]).
//!
//! Questo modulo contiene solo il *vocabolario*: l'identificativo, i due flag e la sintassi dei
//! suffissi. La risoluzione vera vive in `promise_resolution`, cosi' che la dipendenza fra i due
//! moduli resti a senso unico.
//!
//! **Sintassi dei suffissi, portata invariata dal riferimento** (`PLAN.md` §4.3, "semantica
//! invariata ... con l'ordine di strip che va verificato con test dedicati"): un `!` finale
//! significa *strict*, un `[]` finale significa *multiple*. L'ordine in cui vengono tolti non e'
//! simmetrico — prima `!`, poi `[]` — e questo rende `"x![]"` e `"x[]!"` due promesse diverse.
//! Vedi `tests::suffissi::ordine_di_strip_non_e_simmetrico`, che fissa il comportamento.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Riferimento differito a un valore, risolto piu' tardi contro una
/// [`crate::core::promise_resolution::FlatPromiseMap`].
///
/// Immutabile per costruzione: i campi sono privati e non esistono setter, cosi' l'invariante
/// "l'id non contiene piu' i suffissi che hanno acceso i flag" non puo' essere violata dopo la
/// costruzione.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Promise {
    id: String,
    strict: bool,
    multiple: bool,
}

/// Fallimenti della risoluzione di una promessa. Vive qui, e non in `promise_resolution`, perche'
/// e' vocabolario delle promesse: lo condividono `promise_resolution` (che produce
/// [`PromiseError::Circular`]) e `promisable` (che incontra [`PromiseError::Unresolved`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromiseError {
    /// L'id non ha un valore utilizzabile nella mappa appiattita: assente, `Null`, oppure ancora
    /// una `Promise` a sua volta irrisolvibile.
    #[error("promise '{id}' has no value in the resolution map")]
    Unresolved { id: String },
    /// Catena di riferimenti che torna su se stessa. `chain` e' il percorso completo, dal primo
    /// id visitato fino alla ripetizione inclusa, cosi' che il messaggio mostri il ciclo intero.
    #[error("circular promise chain: {}", .chain.join(" -> "))]
    Circular { chain: Vec<String> },
}

impl Promise {
    /// Costruisce una promessa interpretando i suffissi di `raw`: `"fund"`, `"fund!"`,
    /// `"fund[]"`, `"fund[]!"`.
    pub fn new(raw: &str) -> Self {
        Self::with_flags(raw, false, false)
    }

    /// Come [`Promise::new`], ma con i due flag gia' decisi dal chiamante.
    ///
    /// Un flag gia' `true` **disattiva** lo strip del suffisso corrispondente: `with_flags("x!",
    /// true, false)` tiene l'id `"x!"` letterale. E' il comportamento del riferimento
    /// (`if not strict: ...`), non un incidente: e' l'unico modo di costruire un id che contenga
    /// davvero un `!` o un `[]` finale.
    pub fn with_flags(raw: &str, mut strict: bool, mut multiple: bool) -> Self {
        let mut id = raw.to_string();
        if !strict && id.ends_with('!') {
            id.pop();
            strict = true;
        }
        if !multiple && id.ends_with("[]") {
            id.truncate(id.len() - 2);
            multiple = true;
        }
        Promise { id, strict, multiple }
    }

    /// L'identificativo, senza i suffissi che hanno acceso i flag.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Se la promessa e' *strict*, un riferimento irrisolvibile e' un errore invece di far
    /// sparire l'entita' che la contiene (vedi [`crate::core::promisable::Fulfilled`]).
    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Se la promessa e' *multiple*, si risolve sempre in una lista, e l'entita' che la contiene
    /// viene duplicata una volta per valore.
    pub fn multiple(&self) -> bool {
        self.multiple
    }

    /// Errore [`PromiseError::Unresolved`] per questa promessa, costruito qui per non ripetere
    /// il clone dell'id in ogni punto di risoluzione.
    pub(crate) fn unresolved(&self) -> PromiseError {
        PromiseError::Unresolved { id: self.id.clone() }
    }
}

/// Forma canonica: `id` seguito da `[]` se *multiple* e da `!` se *strict*, in quest'ordine.
///
/// L'ordine `[]` prima di `!` non e' arbitrario: e' l'unico che rende la forma canonica
/// ri-parsabile da [`Promise::new`] per **ogni** promessa costruibile, proprio perche' lo strip
/// toglie `!` prima di `[]` (vedi `tests::suffissi`).
impl fmt::Display for Promise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)?;
        if self.multiple {
            f.write_str("[]")?;
        }
        if self.strict {
            f.write_str("!")?;
        }
        Ok(())
    }
}

/// Serializzata come la sua forma canonica (`"fund[]!"`), non come struct a tre campi: e' la
/// stessa stringa che gli autori dei repo formati scrivono nei CSV, e ci si ri-deserializza
/// senza perdita.
impl Serialize for Promise {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Promise {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Promise::new(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod suffissi {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("ref", "ref", false, false; "id nudo")]
        #[test_case("ref!", "ref", true, false; "bang finale accende strict")]
        #[test_case("ref[]", "ref", false, true; "parentesi finali accendono multiple")]
        #[test_case("ref[]!", "ref", true, true; "parentesi poi bang accendono entrambi")]
        #[test_case("ref![]", "ref!", false, true; "bang poi parentesi: solo multiple")]
        #[test_case("!", "", true, false; "solo un bang")]
        #[test_case("[]", "", false, true; "solo parentesi")]
        #[test_case("", "", false, false; "stringa vuota")]
        #[test_case("a!b", "a!b", false, false; "bang non finale non conta")]
        #[test_case("a[]b", "a[]b", false, false; "parentesi non finali non contano")]
        #[test_case("ref[", "ref[", false, false; "parentesi spaiata")]
        #[test_case("ref]", "ref]", false, false; "parentesi chiusa sola")]
        #[test_case("ref!!", "ref!", true, false; "solo un bang viene tolto")]
        #[test_case("ref[][]", "ref[]", false, true; "solo una coppia viene tolta")]
        fn new_interpreta_i_suffissi(raw: &str, id: &str, strict: bool, multiple: bool) {
            let p = Promise::new(raw);
            assert_eq!(p.id(), id);
            assert_eq!(p.strict(), strict, "strict per {raw:?}");
            assert_eq!(p.multiple(), multiple, "multiple per {raw:?}");
        }

        /// Il punto delicato dell'intera sintassi, fissato esplicitamente: lo strip guarda `!`
        /// **prima** di `[]`, quindi `"ref![]"` non finisce con `!` e resta non-strict, mentre
        /// `"ref[]!"` accende entrambi i flag.
        #[test]
        fn ordine_di_strip_non_e_simmetrico() {
            let bang_poi_parentesi = Promise::new("ref![]");
            assert_eq!(bang_poi_parentesi.id(), "ref!");
            assert!(!bang_poi_parentesi.strict());
            assert!(bang_poi_parentesi.multiple());

            let parentesi_poi_bang = Promise::new("ref[]!");
            assert_eq!(parentesi_poi_bang.id(), "ref");
            assert!(parentesi_poi_bang.strict());
            assert!(parentesi_poi_bang.multiple());

            assert_ne!(bang_poi_parentesi, parentesi_poi_bang);
        }

        #[test]
        fn flag_gia_acceso_lascia_il_suffisso_nell_id() {
            let strict = Promise::with_flags("weird!", true, false);
            assert_eq!(strict.id(), "weird!");
            assert!(strict.strict());
            assert!(!strict.multiple());

            let multiple = Promise::with_flags("weird[]", false, true);
            assert_eq!(multiple.id(), "weird[]");
            assert!(!multiple.strict());
            assert!(multiple.multiple());

            let both = Promise::with_flags("weird[]!", true, true);
            assert_eq!(both.id(), "weird[]!");
        }

        #[test]
        fn with_flags_a_flag_spenti_coincide_con_new() {
            for raw in ["ref", "ref!", "ref[]", "ref[]!", "ref![]", "", "!", "[]"] {
                assert_eq!(Promise::with_flags(raw, false, false), Promise::new(raw), "raw {raw:?}");
            }
        }
    }

    mod forma_canonica {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("ref", "ref"; "nessun flag")]
        #[test_case("ref!", "ref!"; "solo strict")]
        #[test_case("ref[]", "ref[]"; "solo multiple")]
        #[test_case("ref[]!", "ref[]!"; "entrambi in forma canonica")]
        #[test_case("ref![]", "ref![]"; "entrambi in forma non canonica restano distinguibili")]
        fn display_riformatta_in_forma_canonica(raw: &str, expected: &str) {
            assert_eq!(Promise::new(raw).to_string(), expected);
        }

        /// Invariante che giustifica l'ordine `[]` prima di `!` in [`fmt::Display`]: per ogni
        /// promessa costruibile, riparsare la forma canonica restituisce la stessa promessa.
        /// Verificata in modo esaustivo su tutte le combinazioni di id "difficili" (che
        /// contengono essi stessi i suffissi) e di flag iniziali.
        #[test]
        fn la_forma_canonica_si_riparsa_identica() {
            let ids = ["", "a", "a!", "a[]", "a![]", "a[]!", "!", "[]", "][", "a!!", "a[][]"];
            for id in ids {
                for strict in [false, true] {
                    for multiple in [false, true] {
                        let p = Promise::with_flags(id, strict, multiple);
                        let reparsed = Promise::new(&p.to_string());
                        assert_eq!(reparsed, p, "id {id:?} strict {strict} multiple {multiple}");
                    }
                }
            }
        }
    }

    mod identita {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of(p: &Promise) -> u64 {
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        }

        #[test]
        fn uguali_solo_se_id_e_flag_coincidono() {
            let base = Promise::new("ref");
            assert_eq!(base, Promise::new("ref"));
            assert_ne!(base, Promise::new("ref!"));
            assert_ne!(base, Promise::new("ref[]"));
            assert_ne!(base, Promise::new("other"));
        }

        #[test]
        fn promesse_uguali_hanno_lo_stesso_hash() {
            assert_eq!(hash_of(&Promise::new("ref[]!")), hash_of(&Promise::with_flags("ref", true, true)));
        }

        #[test]
        fn l_ordinamento_segue_id_poi_strict_poi_multiple() {
            let mut v = [
                Promise::with_flags("b", false, false),
                Promise::with_flags("a", true, false),
                Promise::with_flags("a", false, true),
                Promise::with_flags("a", false, false),
            ];
            v.sort();
            let canonical: Vec<String> = v.iter().map(Promise::to_string).collect();
            assert_eq!(canonical, vec!["a", "a[]", "a!", "b"]);
        }
    }

    mod serde_roundtrip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn serializza_come_stringa_canonica() {
            let p = Promise::new("fund[]!");
            assert_eq!(serde_json::to_string(&p).unwrap(), "\"fund[]!\"");
        }

        #[test]
        fn ogni_promessa_costruibile_sopravvive_al_json() {
            for raw in ["ref", "ref!", "ref[]", "ref[]!", "ref![]", "", "a b/c"] {
                let p = Promise::new(raw);
                let json = serde_json::to_string(&p).unwrap();
                let back: Promise = serde_json::from_str(&json).unwrap();
                assert_eq!(back, p, "raw {raw:?}");
            }
        }

        #[test]
        fn deserializza_solo_da_stringa() {
            assert!(serde_json::from_str::<Promise>("42").is_err());
            assert!(serde_json::from_str::<Promise>("{\"id\":\"x\"}").is_err());
        }
    }

    mod errori {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn unresolved_riporta_l_id_senza_suffissi() {
            let err = Promise::new("fund[]!").unresolved();
            assert_eq!(err, PromiseError::Unresolved { id: "fund".into() });
            assert_eq!(err.to_string(), "promise 'fund' has no value in the resolution map");
        }

        #[test]
        fn circular_mostra_la_catena_intera() {
            let err = PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] };
            assert_eq!(err.to_string(), "circular promise chain: a -> b -> a");
        }
    }
}
