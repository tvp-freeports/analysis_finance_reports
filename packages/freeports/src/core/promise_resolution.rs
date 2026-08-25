//! Le due mappe con cui le promesse vengono raccolte e poi risolte.
//!
//! Il flusso e' quello del riferimento, ma con tipi Rust al posto dei `dict`:
//!
//! 1. mentre le pagine di un documento vengono deserializzate, ogni pipe deposita le coppie
//!    `(id, valore)` che ha prodotto in una [`PromiseMap`] — una *multimappa*, perche' pagine
//!    diverse possono contribuire allo stesso id (il nome del fondo che compare piu' volte, il
//!    totale ripetuto in fondo a ogni tabella);
//! 2. a documento finito la multimappa viene **appiattita** ([`PromiseMap::flatten`]): i
//!    riferimenti fra promesse vengono seguiti, e ogni id resta con un solo valore — scalare se
//!    il contributo era uno solo, [`BlockValue::List`] se erano piu' d'uno;
//! 3. le entita' prodotte dai deserializzatori vengono risolte contro la [`FlatPromiseMap`]
//!    risultante (`crate::core::promisable`).
//!
//! **Riferimenti pendenti (decisione dell'utente, 2026-08-22).** Una promessa che punta a un id
//! di cui la mappa non sa nulla **non e' un errore qui**: resta nella mappa appiattita come
//! [`BlockValue::Promise`], e la politica la decide a valle
//! [`crate::core::promisable::fulfill_promises`] (non-strict ⇒ l'entita' sparisce, strict ⇒
//! errore). Il riferimento `freeports_core` faceva invece uscire un `CircularPromisesChain` anche
//! in quel caso, per un effetto collaterale del suo fallback (`mapping.get(id, [value])`
//! restituiva la promessa stessa, e al giro dopo scattava il controllo di ciclo): un messaggio
//! fuorviante che qui sparisce. [`PromiseError::Circular`] e' riservato ai cicli veri.
//!
//! **Determinismo.** Entrambe le mappe sono [`BTreeMap`] e non `HashMap` (`PLAN.md` §4.3 diceva
//! `HashMap`): l'appiattimento visita gli id in ordine, quindi a parita' di contenuto la catena
//! riportata da un ciclo e' sempre la stessa e i messaggi d'errore sono riproducibili nei test.

use std::collections::BTreeMap;

use super::classes::value::BlockValue;
use super::promise::{Promise, PromiseError};

/// Multimappa `id -> contributi`, riempita una pagina alla volta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromiseMap {
    entries: BTreeMap<String, Vec<BlockValue>>,
}

/// Mappa `id -> valore unico`, prodotta da [`PromiseMap::flatten`] e usata per risolvere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlatPromiseMap {
    entries: BTreeMap<String, BlockValue>,
}

impl PromiseMap {
    pub fn new() -> Self {
        PromiseMap::default()
    }

    /// Aggiunge un contributo per `id`, in coda a quelli gia' presenti. L'ordine di inserimento
    /// e' significativo: e' l'ordine delle pagine, e chi arriva dopo vince quando la promessa non
    /// e' *multiple* (vedi [`FlatPromiseMap::fulfill`]).
    pub fn push(&mut self, id: impl Into<String>, value: impl Into<BlockValue>) {
        self.entries.entry(id.into()).or_default().push(value.into());
    }

    /// Versa nella multimappa tutte le coppie prodotte da un pipe. Equivalente al
    /// `merge_into_multimap` del riferimento.
    pub fn merge<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<BlockValue>,
    {
        for (k, v) in entries {
            self.push(k, v);
        }
    }

    /// I contributi registrati per `id`, in ordine di inserimento.
    pub fn get(&self, id: &str) -> Option<&[BlockValue]> {
        self.entries.get(id).map(Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &[BlockValue])> {
        self.entries.iter().map(|(k, v)| (k, v.as_slice()))
    }

    /// Segue i riferimenti fra promesse e riduce ogni id a un valore solo.
    ///
    /// Un id con un solo contributo diventa quel contributo; con piu' d'uno diventa una
    /// [`BlockValue::List`] nell'ordine di inserimento; senza contributi sparisce dalla mappa
    /// appiattita. I riferimenti pendenti restano `Promise` (vedi la nota in testa al modulo);
    /// un ciclo e' [`PromiseError::Circular`], con la catena completa dal primo id visitato fino
    /// alla ripetizione.
    ///
    /// La ricorsione **non** scende dentro liste, insiemi e mappe: una `Promise` annidata dentro
    /// un `BlockValue::List` non viene risolta, esattamente come nel riferimento. I pipe
    /// depositano promesse al livello superiore, mai sepolte in un contenitore.
    pub fn flatten(&self) -> Result<FlatPromiseMap, PromiseError> {
        let mut resolved = BTreeMap::new();
        let mut in_progress = Vec::new();
        for id in self.entries.keys() {
            self.resolve_id(id, &mut in_progress, &mut resolved)?;
        }
        Ok(FlatPromiseMap { entries: resolved })
    }

    /// Visita in profondita' un singolo id. `in_corso` e' il cammino corrente (rileva i cicli),
    /// `resolved` e' la memoizzazione (ogni id si appiattisce una volta sola, anche se molti altri
    /// lo riferiscono).
    fn resolve_id(
        &self,
        id: &str,
        in_progress: &mut Vec<String>,
        resolved: &mut BTreeMap<String, BlockValue>,
    ) -> Result<(), PromiseError> {
        if resolved.contains_key(id) {
            return Ok(());
        }
        if let Some(start) = in_progress.iter().position(|visited| visited == id) {
            let mut chain: Vec<String> = in_progress[start..].to_vec();
            chain.push(id.to_string());
            return Err(PromiseError::Circular { chain });
        }
        // Un id di cui non sappiamo nulla non e' un errore: chi lo riferisce si tiene la promessa.
        let Some(contributions) = self.entries.get(id) else {
            return Ok(());
        };

        in_progress.push(id.to_string());
        let mut flattened = Vec::with_capacity(contributions.len());
        for contribution in contributions {
            match contribution {
                BlockValue::Promise(promise) => {
                    self.resolve_id(promise.id(), in_progress, resolved)?;
                    match resolved.get(promise.id()) {
                        Some(value) => flattened.push(value.clone()),
                        // L'id riferito non esiste, o esiste senza contributi: la promessa resta.
                        None => flattened.push(contribution.clone()),
                    }
                }
                other => flattened.push(other.clone()),
            }
        }
        in_progress.pop();

        let single = if flattened.len() == 1 {
            flattened.pop()
        } else if flattened.is_empty() {
            None
        } else {
            Some(BlockValue::List(flattened))
        };
        if let Some(value) = single {
            resolved.insert(id.to_string(), value);
        }
        Ok(())
    }
}

impl FlatPromiseMap {
    pub fn new() -> Self {
        FlatPromiseMap::default()
    }

    pub fn get(&self, id: &str) -> Option<&BlockValue> {
        self.entries.get(id)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &BlockValue)> {
        self.entries.iter()
    }

    /// Risolve una promessa contro questa mappa.
    ///
    /// - id assente, valore [`BlockValue::Null`], o valore ancora `Promise`: la promessa non e'
    ///   risolvibile, [`PromiseError::Unresolved`];
    /// - promessa *multiple*: si ottiene sempre una [`BlockValue::List`], anche quando il valore
    ///   registrato era scalare;
    /// - promessa normale su una lista: vince **l'ultimo** valore, cioe' il contributo della
    ///   pagina piu' recente.
    ///
    /// In entrambi i casi con lista, i contributi rimasti `Promise` (riferimenti pendenti, vedi
    /// la nota in testa al modulo) vengono scartati: se non ne resta nessun altro, la promessa e'
    /// [`PromiseError::Unresolved`].
    pub fn fulfill(&self, promise: &Promise) -> Result<BlockValue, PromiseError> {
        let registered = self.entries.get(promise.id()).ok_or_else(|| promise.unresolved())?;
        let candidates: Vec<&BlockValue> = match registered {
            BlockValue::Null | BlockValue::Promise(_) => Vec::new(),
            BlockValue::List(values) => values.iter().filter(|v| !v.is_promise()).collect(),
            scalar => vec![scalar],
        };
        if promise.multiple() {
            if candidates.is_empty() {
                return Err(promise.unresolved());
            }
            return Ok(BlockValue::List(candidates.into_iter().cloned().collect()));
        }
        candidates.last().map(|v| (*v).clone()).ok_or_else(|| promise.unresolved())
    }
}

impl<K: Into<String>, V: Into<BlockValue>> FromIterator<(K, V)> for PromiseMap {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = PromiseMap::new();
        map.merge(iter);
        map
    }
}

impl<K: Into<String>, V: Into<BlockValue>> FromIterator<(K, V)> for FlatPromiseMap {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        FlatPromiseMap { entries: iter.into_iter().map(|(k, v)| (k.into(), v.into())).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promise(raw: &str) -> BlockValue {
        BlockValue::Promise(Promise::new(raw))
    }

    mod multimap {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn accumulates_contributions_in_order() {
            let mut map = PromiseMap::new();
            map.push("fund", 1_i64);
            map.push("fund", 2_i64);
            assert_eq!(map.get("fund"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
        }

        #[test]
        fn merge_pours_multiple_pairs_at_once() {
            let mut map = PromiseMap::new();
            map.merge([("a", 1_i64), ("b", 2_i64)]);
            map.merge([("a", 3_i64)]);
            assert_eq!(map.get("a"), Some(&[BlockValue::Int(1), BlockValue::Int(3)][..]));
            assert_eq!(map.get("b"), Some(&[BlockValue::Int(2)][..]));
            assert_eq!(map.len(), 2);
        }

        #[test]
        fn a_new_map_is_empty() {
            let map = PromiseMap::new();
            assert!(map.is_empty());
            assert_eq!(map.len(), 0);
            assert_eq!(map.get("assente"), None);
        }

        #[test]
        fn is_built_from_an_iterator() {
            let map: PromiseMap = [("a", 1_i64), ("a", 2_i64), ("b", 3_i64)].into_iter().collect();
            assert_eq!(map.get("a"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
            assert_eq!(map.len(), 2);
        }

        #[test]
        fn iterates_in_key_order() {
            let map: PromiseMap = [("z", 1_i64), ("a", 2_i64), ("m", 3_i64)].into_iter().collect();
            let keys: Vec<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(keys, vec!["a", "m", "z"]);
        }
    }

    mod flattening {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_single_contribution_stays_scalar() {
            let map: PromiseMap = [("x", 42_i64)].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("x"), Some(&BlockValue::Int(42)));
        }

        #[test]
        fn multiple_contributions_become_a_list() {
            let map: PromiseMap = [("x", 1_i64), ("x", 2_i64), ("x", 3_i64)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)]))
            );
        }

        #[test]
        fn an_empty_map_flattens_to_an_empty_map() {
            assert!(PromiseMap::new().flatten().unwrap().is_empty());
        }

        #[test]
        fn an_id_without_contributions_disappears() {
            let mut map = PromiseMap::new();
            map.entries.insert("vuoto".into(), Vec::new());
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("vuoto"), None);
            assert!(flat.is_empty());
        }

        #[test]
        fn resolves_a_simple_reference() {
            let map: PromiseMap =
                [("source", promise("target")), ("target", BlockValue::Int(99))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&BlockValue::Int(99)));
            assert_eq!(flat.get("target"), Some(&BlockValue::Int(99)));
        }

        #[test]
        fn resolves_a_chain_of_references() {
            let map: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", BlockValue::Int(7))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&BlockValue::Int(7)));
            assert_eq!(flat.get("b"), Some(&BlockValue::Int(7)));
        }

        #[test]
        fn a_reference_to_a_list_receives_the_whole_list() {
            let map: PromiseMap =
                [("src", promise("t")), ("t", BlockValue::Int(1)), ("t", BlockValue::Int(2))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            let expected = BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]);
            assert_eq!(flat.get("src"), Some(&expected));
            assert_eq!(flat.get("t"), Some(&expected));
        }

        /// Caso che nel riferimento costava una passata a vuoto in piu': un id con due contributi
        /// che sono entrambi promesse. Qui e' una visita sola, e il risultato e' la lista dei due
        /// valori riferiti, nell'ordine di inserimento.
        #[test]
        fn an_id_with_two_promised_contributions_becomes_the_list_of_values() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", promise("b")),
                ("a", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])));
        }

        #[test]
        fn unpromised_contributions_stay_alongside_resolved_ones() {
            let map: PromiseMap =
                [("x", BlockValue::from("fisso")), ("x", promise("a")), ("a", BlockValue::Int(5))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&BlockValue::List(vec![BlockValue::from("fisso"), BlockValue::Int(5)]))
            );
        }

        #[test]
        fn the_promise_flags_do_not_affect_flattening() {
            // Strict e multiple contano al momento di risolvere un'entita', non qui: `flatten`
            // sostituisce comunque il valore registrato per l'id riferito.
            let map: PromiseMap =
                [("src", promise("t[]!")), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&BlockValue::Int(1)));
        }

        #[test]
        fn does_not_descend_into_containers() {
            let nested = BlockValue::List(vec![promise("t")]);
            let map: PromiseMap =
                [("src", nested.clone()), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&nested));
        }
    }

    /// La politica scelta dall'utente (2026-08-22): un riferimento che non porta da nessuna parte
    /// non e' un errore di appiattimento.
    mod pending_references {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_unknown_id_leaves_the_promise_in_place() {
            let map: PromiseMap = [("source", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&promise("nowhere")));
        }

        #[test]
        fn a_reference_to_an_id_without_contributions_leaves_the_promise() {
            let mut map = PromiseMap::new();
            map.push("source", Promise::new("vuoto"));
            map.entries.insert("vuoto".into(), Vec::new());
            assert_eq!(map.flatten().unwrap().get("source"), Some(&promise("vuoto")));
        }

        #[test]
        fn a_chain_that_ends_in_nothing_stops_on_the_pending_promise() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("b"), Some(&promise("nowhere")));
            assert_eq!(flat.get("a"), Some(&promise("nowhere")));
        }

        #[test]
        fn a_pending_reference_does_not_prevent_others_from_resolving() {
            let map: PromiseMap =
                [("a", promise("nowhere")), ("b", promise("c")), ("c", BlockValue::Int(3))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&promise("nowhere")));
            assert_eq!(flat.get("b"), Some(&BlockValue::Int(3)));
        }
    }

    mod cycles {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_self_reference_is_a_cycle() {
            let map: PromiseMap = [("a", promise("a"))].into_iter().collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "a".into()] }
            );
        }

        #[test]
        fn two_ids_that_reference_each_other_are_a_cycle() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("a"))].into_iter().collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] }
            );
        }

        #[test]
        fn the_reported_chain_covers_the_whole_cycle() {
            let map: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", promise("a"))].into_iter().collect();
            let err = map.flatten().unwrap_err();
            assert_eq!(
                err,
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "c".into(), "a".into()] }
            );
            assert_eq!(err.to_string(), "circular promise chain: a -> b -> c -> a");
        }

        /// Un cammino che *entra* in un ciclo senza farne parte: la catena riportata parte dal
        /// primo id visitato, non dal primo id del ciclo — cosi' il messaggio mostra anche come ci
        /// si e' arrivati.
        #[test]
        fn a_path_that_enters_a_cycle_also_reports_the_entry_point() {
            let map: PromiseMap =
                [("ingresso", promise("a")), ("a", promise("b")), ("b", promise("a"))]
                    .into_iter()
                    .collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] }
            );
        }

        /// L'appiattimento visita gli id in ordine, quindi la catena riportata non dipende
        /// dall'ordine in cui i contributi sono stati inseriti: i messaggi d'errore sono
        /// riproducibili.
        #[test]
        fn the_reported_chain_is_deterministic() {
            let forward: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", promise("a"))].into_iter().collect();
            let reversed: PromiseMap =
                [("c", promise("a")), ("b", promise("c")), ("a", promise("b"))].into_iter().collect();
            assert_eq!(forward.flatten().unwrap_err(), reversed.flatten().unwrap_err());
        }

        #[test]
        fn a_cycle_fails_the_entire_flattening() {
            let map: PromiseMap =
                [("sano", BlockValue::Int(1)), ("a", promise("b")), ("b", promise("a"))]
                    .into_iter()
                    .collect();
            assert!(map.flatten().is_err());
        }
    }

    mod resolution {
        use super::*;
        use pretty_assertions::assert_eq;

        fn flat(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
            pairs.into_iter().collect()
        }

        #[test]
        fn a_scalar_value_resolves_to_itself() {
            let map = flat(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::from("Acme"));
        }

        #[test]
        fn on_a_list_the_last_value_wins() {
            let map = flat(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)]),
            )]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(3));
        }

        #[test]
        fn a_multiple_promise_always_gets_a_list() {
            let scalar = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(
                scalar.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1)])
            );

            let list = flat(vec![("fund", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]))]);
            assert_eq!(
                list.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn a_missing_id_is_unresolvable() {
            let map = flat(vec![("altro", BlockValue::Int(1))]);
            assert_eq!(
                map.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }

        /// Un `Null` registrato conta come valore *assente*, non come valore nullo: e' la
        /// semantica del riferimento (`if value.is_none(): raise KeyError`).
        #[test]
        fn a_null_value_is_unresolvable() {
            let map = flat(vec![("fund", BlockValue::Null)]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// Il seguito della politica sui riferimenti pendenti: una promessa sopravvissuta
        /// all'appiattimento non risolve nulla, ed e' qui che diventa un errore.
        #[test]
        fn a_value_still_a_promise_is_unresolvable() {
            let map = flat(vec![("fund", promise("nowhere"))]);
            assert_eq!(
                map.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }

        #[test]
        fn pending_promises_inside_a_list_are_discarded() {
            let map = flat(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::Int(1), promise("nowhere"), BlockValue::Int(2)]),
            )]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(2));
            assert_eq!(
                map.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn a_list_of_only_pending_promises_is_unresolvable() {
            let map = flat(vec![("fund", BlockValue::List(vec![promise("a"), promise("b")]))]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        #[test]
        fn an_empty_list_is_unresolvable() {
            let map = flat(vec![("fund", BlockValue::List(Vec::new()))]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// `strict` non cambia *se* una promessa si risolve, solo cosa succede a chi la contiene
        /// quando non si risolve — decisione che spetta a `promisable`.
        #[test]
        fn strict_does_not_change_the_resolution_outcome() {
            let map = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(map.fulfill(&Promise::new("fund!")).unwrap(), BlockValue::Int(1));
            let empty = FlatPromiseMap::new();
            assert_eq!(
                empty.fulfill(&Promise::new("fund!")).unwrap_err(),
                empty.fulfill(&Promise::new("fund")).unwrap_err()
            );
        }

        #[test]
        fn the_error_names_the_id_without_suffixes() {
            let empty = FlatPromiseMap::new();
            assert_eq!(
                empty.fulfill(&Promise::new("fund[]!")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }
    }

    /// Le proprieta' che devono valere su input generati, non solo sui casi scritti a mano.
    mod invariants {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Appiattire due volte non cambia nulla: la mappa appiattita, reinserita in una
        /// multimappa, si appiattisce in se stessa.
        #[test]
        fn flattening_is_idempotent() {
            let map: PromiseMap = [
                ("a", promise("b")),
                ("b", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
                ("c", BlockValue::from("x")),
                ("d", promise("nowhere")),
            ]
            .into_iter()
            .collect();
            let once = map.flatten().unwrap();
            let reinserted: PromiseMap =
                once.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            assert_eq!(reinserted.flatten().unwrap(), once);
        }

        /// Una catena lineare lunga si risolve tutta sullo stesso valore finale, senza esplosioni
        /// combinatorie: ogni id viene appiattito una volta sola grazie alla memoizzazione.
        #[test]
        fn a_long_chain_resolves_entirely_to_the_final_value() {
            const LENGTH: usize = 500;
            let mut map = PromiseMap::new();
            for i in 0..LENGTH {
                map.push(format!("id{i}"), Promise::new(&format!("id{}", i + 1)));
            }
            map.push(format!("id{LENGTH}"), 42_i64);
            let flat = map.flatten().unwrap();
            for i in 0..=LENGTH {
                assert_eq!(flat.get(&format!("id{i}")), Some(&BlockValue::Int(42)), "id{i}");
            }
        }

        /// Molti id che puntano tutti allo stesso bersaglio: nessun ciclo, tutti risolti.
        #[test]
        fn many_references_to_the_same_id_all_resolve() {
            let mut map = PromiseMap::new();
            map.push("target", 7_i64);
            for i in 0..200 {
                map.push(format!("src{i}"), Promise::new("target"));
            }
            let flat = map.flatten().unwrap();
            for i in 0..200 {
                assert_eq!(flat.get(&format!("src{i}")), Some(&BlockValue::Int(7)));
            }
        }

        /// Un ciclo lungo viene comunque rilevato, e la catena riportata ha esattamente la
        /// lunghezza del ciclo piu' uno (la ripetizione finale).
        #[test]
        fn a_long_cycle_is_detected() {
            const LENGTH: usize = 300;
            let mut map = PromiseMap::new();
            for i in 0..LENGTH {
                map.push(format!("id{i:03}"), Promise::new(&format!("id{:03}", (i + 1) % LENGTH)));
            }
            match map.flatten().unwrap_err() {
                PromiseError::Circular { chain } => assert_eq!(chain.len(), LENGTH + 1),
                other => panic!("atteso un ciclo, trovato {other:?}"),
            }
        }

        /// Se nessun contributo e' una promessa, l'appiattimento e' pura riduzione: ogni id
        /// conserva i suoi valori, nell'ordine, e nessuno puo' fallire.
        #[test]
        fn without_promises_flattening_preserves_the_contributions() {
            for n_contributions in 1..8_usize {
                let mut map = PromiseMap::new();
                for i in 0..n_contributions {
                    map.push("x", i as i64);
                }
                let flat = map.flatten().unwrap();
                let expected = if n_contributions == 1 {
                    BlockValue::Int(0)
                } else {
                    BlockValue::List((0..n_contributions).map(|i| BlockValue::Int(i as i64)).collect())
                };
                assert_eq!(flat.get("x"), Some(&expected), "con {n_contributions} contributi");
            }
        }
    }
}
