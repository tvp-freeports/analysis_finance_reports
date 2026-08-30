//! Le due mappe con cui le promesse vengono raccolte e poi risolte.
//!
//! Il flusso e' quello del riferimento, ma con tipi Rust al posto dei `dict`:
//!
//! 1. mentre le pagine di un documento vengono deserializzate, ogni pipe deposita le coppie
//!    `(id, valore)` che ha prodotto in una [`PromiseMap`] — una *multimappa*, perche' pagine
//!    diverse possono contribuire allo stesso id (il nome del fondo che compare piu' volte, il
//!    totale ripetuto in fondo a ogni tabella);
//! 2. a documento finito la multimappa viene **appiattita** ([`PromiseMap::flatten`]): i
//!    riferimenti fra promesse vengono seguiti e sostituiti dai contributi dell'id riferito, ma
//!    ogni id conserva la propria **sequenza** di contributi;
//! 3. le entita' prodotte dai deserializzatori vengono risolte contro la [`FlatPromiseMap`]
//!    risultante (`crate::core::promisable`).
//!
//! **Contenitore e contributo sono cose diverse.** L'appiattimento non sintetizza nessuna lista:
//! `[("x", 1), ("x", 2)]` lascia due contributi, `[("x", List([1, 2]))]` ne lascia **uno** che e'
//! una lista, e le due mappe appiattite sono distinguibili. Un pipe d'autore che voglia depositare
//! piu' contributi per lo stesso id non restituisce quindi un dict con valore-lista
//! (`{"id": [a, b]}`, che vale un contributo solo), ma una lista di dict separati
//! (`[{"id": a}, {"id": b}]`): `PyDeserializePipe::deserialize` appiattisce il valore restituito
//! dal pipe, ogni dict diventa un `Extracted::Promises` a se' stante e tutti confluiscono nella
//! stessa multimappa, che accumula per chiave. Il meccanismo e' preesistente e indipendente da
//! questa distinzione.
//!
//! **Un [`BlockValue::Null`] e' un non-contributo** (decisione dell'utente, 2026-08-29): viene
//! scartato gia' durante l'appiattimento, come se non fosse mai stato depositato. Un id i cui
//! contributi erano tutti `Null` sparisce percio' dalla mappa appiattita, esattamente come un id
//! che non ha mai avuto contributi.
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

/// Mappa `id -> contributi appiattiti`, prodotta da [`PromiseMap::flatten`] e usata per risolvere.
///
/// Invariante: nessun id vi compare con un vettore vuoto — un id che dopo l'appiattimento non ha
/// lasciato alcun contributo semplicemente non entra nella mappa. E' cio' che permette a un
/// riferimento di distinguere "id risolto" da "id senza nulla da dare" guardando la sola presenza
/// della chiave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlatPromiseMap {
    entries: BTreeMap<String, Vec<BlockValue>>,
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

    /// Segue i riferimenti fra promesse, lasciando a ogni id la sua sequenza di contributi.
    ///
    /// Un contributo che non e' una promessa si conserva tale e quale, contenitori compresi: il
    /// numero di contributi non si perde, e un unico contributo [`BlockValue::List`] resta
    /// distinguibile da piu' contributi scalari. Fanno eccezione i [`BlockValue::Null`], che sono
    /// non-contributi e vengono scartati qui; un id che resta senza contributi — perche' non ne
    /// aveva o perche' erano tutti `Null` — sparisce dalla mappa appiattita.
    ///
    /// Un contributo `Promise` che punta a un id risolto viene sostituito da **tutti** i
    /// contributi di quell'id, innestati al posto suo e nel loro ordine: un riferimento *eredita*
    /// i contributi del bersaglio. Impacchettarli in una lista sola reintrodurrebbe l'ambiguita'
    /// fra contenitore e contributo al passaggio del riferimento. Due riferimenti allo stesso
    /// bersaglio ne innestano quindi i contributi due volte.
    ///
    /// I riferimenti pendenti restano `Promise` (vedi la nota in testa al modulo); un ciclo e'
    /// [`PromiseError::Circular`], con la catena completa dal primo id visitato fino alla
    /// ripetizione.
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
        tracing::debug!(ids = resolved.len(), "promise map flattened");
        Ok(FlatPromiseMap { entries: resolved })
    }

    /// Visita in profondita' un singolo id. `in_corso` e' il cammino corrente (rileva i cicli),
    /// `resolved` e' la memoizzazione (ogni id si appiattisce una volta sola, anche se molti altri
    /// lo riferiscono).
    ///
    /// L'invariante "nessun vettore vuoto in `resolved`" regge per induzione sulla profondita'
    /// della visita: si inserisce solo quando il vettore accumulato e' non vuoto, e i contributi
    /// ereditati da un riferimento vengono da un id gia' inserito, quindi gia' non vuoto.
    fn resolve_id(
        &self,
        id: &str,
        in_progress: &mut Vec<String>,
        resolved: &mut BTreeMap<String, Vec<BlockValue>>,
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
                // Un `Null` non e' un valore nullo: e' un contributo che non c'e'.
                BlockValue::Null => {}
                BlockValue::Promise(promise) => {
                    self.resolve_id(promise.id(), in_progress, resolved)?;
                    match resolved.get(promise.id()) {
                        // Il riferimento eredita i contributi del bersaglio, innestati al posto
                        // suo: impacchettarli in una lista li renderebbe un contributo solo.
                        Some(values) => flattened.extend(values.iter().cloned()),
                        // L'id riferito non esiste, o non ha lasciato contributi: la promessa
                        // resta pendente. Non e' un errore (vedi il doc-comment del modulo), ma
                        // e' il dettaglio che serve solo in un debug attivo.
                        None => {
                            tracing::trace!(id, target = promise.id(), "reference kept pending: target has no contributions");
                            flattened.push(contribution.clone());
                        }
                    }
                }
                other => flattened.push(other.clone()),
            }
        }
        in_progress.pop();

        if !flattened.is_empty() {
            resolved.insert(id.to_string(), flattened);
        }
        Ok(())
    }
}

impl FlatPromiseMap {
    pub fn new() -> Self {
        FlatPromiseMap::default()
    }

    /// I contributi appiattiti registrati per `id`, in ordine. Mai vuoto: un id senza contributi
    /// non entra nella mappa.
    ///
    /// I contributi non sono filtrati: possono ancora contenere riferimenti pendenti, che solo
    /// [`FlatPromiseMap::fulfill`] scarta. I [`BlockValue::Null`] invece non ci sono mai, perche'
    /// l'appiattimento li ha gia' scartati.
    pub fn get(&self, id: &str) -> Option<&[BlockValue]> {
        self.entries.get(id).map(Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Gli id con i loro contributi, in ordine di chiave. Come [`FlatPromiseMap::get`], i
    /// contributi non sono filtrati: i riferimenti pendenti ci sono ancora.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &[BlockValue])> {
        self.entries.iter().map(|(k, v)| (k, v.as_slice()))
    }

    /// Risolve una promessa contro questa mappa.
    ///
    /// - id assente dalla mappa: [`PromiseError::Unresolved`];
    /// - i contributi rimasti `Promise` (riferimenti pendenti, vedi la nota in testa al modulo)
    ///   vengono scartati; se non resta nessun candidato la promessa e' `Unresolved`. I `Null` non
    ///   arrivano fin qui: l'appiattimento li ha gia' scartati;
    /// - promessa *multiple*: [`BlockValue::List`] dei candidati, sempre non vuota. Un candidato
    ///   che e' esso stesso una lista vi finisce dentro come elemento, senza essere sciolto;
    /// - promessa normale: vince **l'ultimo** candidato, cioe' il contributo della pagina piu'
    ///   recente, restituito tale e quale — se e' una lista, si ottiene quella lista.
    pub fn fulfill(&self, promise: &Promise) -> Result<BlockValue, PromiseError> {
        let contributions = self.entries.get(promise.id()).ok_or_else(|| promise.unresolved())?;
        let candidates: Vec<&BlockValue> =
            contributions.iter().filter(|v| !v.is_promise()).collect();
        if candidates.len() != contributions.len() {
            tracing::trace!(
                id = promise.id(),
                pending = contributions.len() - candidates.len(),
                "pending contributions ignored while fulfilling"
            );
        }
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

/// Fuori dai test l'unico produttore legittimo di una [`FlatPromiseMap`] e'
/// [`PromiseMap::flatten`]: non esiste una `FromIterator`, perche' un `Into<BlockValue>` lascerebbe
/// scivolare dentro in silenzio un `vec![a, b]` come **unico** contributo-lista, che e' proprio
/// l'ambiguita' che questo modulo elimina.
#[cfg(test)]
impl FlatPromiseMap {
    /// Costruisce una mappa gia' appiattita da coppie `(id, contributo)`, **accumulando** per
    /// chiave: chiavi ripetute sono contributi multipli dello stesso id, non sovrascritture.
    pub(crate) fn from_pairs<K, V, I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<BlockValue>,
    {
        let mut entries: BTreeMap<String, Vec<BlockValue>> = BTreeMap::new();
        for (id, contribution) in pairs {
            entries.entry(id.into()).or_default().push(contribution.into());
        }
        FlatPromiseMap { entries }
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

        /// Non c'e' nessuna "scalarizzazione": un contributo solo resta un contributo solo, in un
        /// vettore lungo uno. E' il punto di F2 — il numero di contributi non si perde.
        #[test]
        fn a_single_contribution_stays_alone() {
            let map: PromiseMap = [("x", 42_i64)].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("x"), Some(&[BlockValue::Int(42)][..]));
        }

        /// Piu' contributi restano piu' contributi: non vengono impacchettati in un
        /// [`BlockValue::List`], che sarebbe indistinguibile da un unico contributo-lista.
        #[test]
        fn multiple_contributions_stay_separate() {
            let map: PromiseMap = [("x", 1_i64), ("x", 2_i64), ("x", 3_i64)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&[BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)][..])
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

        /// Caso speciale del test qui sopra (decisione dell'utente su Q-F2b/C3, 2026-08-29): un
        /// contributo [`BlockValue::Null`] e' un *non*-contributo, scartato gia' durante
        /// l'appiattimento. Un id di soli `Null` produce percio' un vettore vuoto, e un vettore
        /// vuoto non entra nella mappa appiattita: l'id sparisce esattamente come se non avesse
        /// mai avuto contributi.
        #[test]
        fn a_null_only_id_disappears_from_the_flat_map() {
            let map: PromiseMap =
                [("solo-null", BlockValue::Null), ("solo-null", BlockValue::Null)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("solo-null"), None);
            assert!(flat.is_empty());
        }

        /// Un `Null` in mezzo ad altri contributi non sopravvive e non puo' quindi diventare il
        /// valore vincente: sparisce lui, non gli altri.
        #[test]
        fn a_null_contribution_disappears_from_among_the_others() {
            let map: PromiseMap =
                [("x", BlockValue::Int(1)), ("x", BlockValue::Null), ("x", BlockValue::Int(2))]
                    .into_iter()
                    .collect();
            assert_eq!(
                map.flatten().unwrap().get("x"),
                Some(&[BlockValue::Int(1), BlockValue::Int(2)][..])
            );
        }

        #[test]
        fn resolves_a_simple_reference() {
            let map: PromiseMap =
                [("source", promise("target")), ("target", BlockValue::Int(99))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&[BlockValue::Int(99)][..]));
            assert_eq!(flat.get("target"), Some(&[BlockValue::Int(99)][..]));
        }

        #[test]
        fn resolves_a_chain_of_references() {
            let map: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", BlockValue::Int(7))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&[BlockValue::Int(7)][..]));
            assert_eq!(flat.get("b"), Some(&[BlockValue::Int(7)][..]));
        }

        /// Un riferimento **eredita i contributi** del bersaglio, in ordine: non li impacchetta in
        /// una lista, altrimenti l'ambiguita' F2 rientrerebbe dalla porta del riferimento (un
        /// bersaglio con due contributi scalari tornerebbe indistinguibile, per chi lo riferisce,
        /// da un bersaglio con un solo contributo-lista).
        #[test]
        fn a_reference_receives_the_contributions_of_its_target() {
            let map: PromiseMap =
                [("src", promise("t")), ("t", BlockValue::Int(1)), ("t", BlockValue::Int(2))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            let expected = [BlockValue::Int(1), BlockValue::Int(2)];
            assert_eq!(flat.get("src"), Some(&expected[..]));
            assert_eq!(flat.get("t"), Some(&expected[..]));
        }

        /// Caso che nel riferimento costava una passata a vuoto in piu': un id con due contributi
        /// che sono entrambi promesse. Qui e' una visita sola, e il risultato sono i due valori
        /// riferiti, nell'ordine di inserimento.
        #[test]
        fn an_id_with_two_promised_contributions_becomes_the_values_of_both() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", promise("b")),
                ("a", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
        }

        #[test]
        fn unpromised_contributions_stay_alongside_resolved_ones() {
            let map: PromiseMap =
                [("x", BlockValue::from("fisso")), ("x", promise("a")), ("a", BlockValue::Int(5))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[BlockValue::from("fisso"), BlockValue::Int(5)][..]));
        }

        /// I contributi ereditati da un riferimento si innestano **al posto della promessa**, non
        /// in coda: l'ordine di inserimento dei contributi propri e' conservato attorno a loro.
        #[test]
        fn a_reference_splices_the_contributions_of_its_target_among_its_own() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", BlockValue::Int(5)),
                ("a", BlockValue::Int(1)),
                ("a", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&[BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(5)][..])
            );
        }

        /// Due riferimenti allo stesso bersaglio multi-contributo innestano i suoi contributi due
        /// volte: la cardinalita' di `x` e' la somma, non il numero di riferimenti. E' il delta
        /// osservabile piu' ampio dello splicing (`critic`, C2), e si vede anche sulla promessa
        /// *multiple*, che qui espande in quattro copie.
        #[test]
        fn two_references_to_the_same_multi_contribution_target_splice_twice() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", promise("a")),
                ("a", BlockValue::Int(1)),
                ("a", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(
                    &[BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(1), BlockValue::Int(2)][..]
                )
            );
            assert_eq!(
                flat.fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![
                    BlockValue::Int(1),
                    BlockValue::Int(2),
                    BlockValue::Int(1),
                    BlockValue::Int(2),
                ])
            );
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), BlockValue::Int(2));
        }

        #[test]
        fn the_promise_flags_do_not_affect_flattening() {
            // Strict e multiple contano al momento di risolvere un'entita', non qui: `flatten`
            // sostituisce comunque la promessa con i contributi dell'id riferito.
            let map: PromiseMap =
                [("src", promise("t[]!")), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&[BlockValue::Int(1)][..]));
        }

        #[test]
        fn does_not_descend_into_containers() {
            let nested = BlockValue::List(vec![promise("t")]);
            let map: PromiseMap =
                [("src", nested.clone()), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&[nested][..]));
        }

        /// L'invariante da cui dipende tutto il resto: nella mappa appiattita nessun id ha un
        /// vettore di contributi vuoto. E' cio' che permette a un riferimento di distinguere "id
        /// risolto" da "id senza nulla da dare" guardando la sola presenza della chiave.
        #[test]
        fn the_flat_map_never_holds_an_empty_contribution_list() {
            let mut map = PromiseMap::new();
            map.push("scalare", 1_i64);
            map.push("multi", 1_i64);
            map.push("multi", 2_i64);
            map.push("riferimento", Promise::new("multi"));
            map.push("pendente", Promise::new("nowhere"));
            map.push("contenitore-vuoto", BlockValue::List(Vec::new()));
            map.push("solo-null", BlockValue::Null);
            map.entries.insert("senza-contributi".into(), Vec::new());

            let flat = map.flatten().unwrap();
            for (id, contributions) in flat.iter() {
                assert!(!contributions.is_empty(), "l'id `{id}` ha un vettore di contributi vuoto");
            }
            assert_eq!(flat.get("solo-null"), None);
            assert_eq!(flat.get("senza-contributi"), None);
            // Un contenitore vuoto e' invece un valore vero, e resta.
            assert_eq!(flat.get("contenitore-vuoto"), Some(&[BlockValue::List(Vec::new())][..]));
        }
    }

    /// Il cuore di F2: un contributo che *e'* un contenitore non e' la stessa cosa di N contributi.
    /// Attraversa appiattimento e risoluzione, perche' il vecchio disegno confondeva le due cose in
    /// entrambe le fasi (`flatten` sintetizzava una `List`, `fulfill` la ri-scioglieva).
    mod container_valued_contributions {
        use super::*;
        use pretty_assertions::{assert_eq, assert_ne};
        use std::collections::BTreeSet;

        fn one_list_contribution() -> FlatPromiseMap {
            let map: PromiseMap =
                [("x", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]))]
                    .into_iter()
                    .collect();
            map.flatten().unwrap()
        }

        fn two_scalar_contributions() -> FlatPromiseMap {
            let map: PromiseMap = [("x", BlockValue::Int(1)), ("x", BlockValue::Int(2))].into_iter().collect();
            map.flatten().unwrap()
        }

        /// Il test che pinna il bug: le due mappe erano indistinguibili dopo l'appiattimento, e da
        /// li' in poi nessuna informazione poteva piu' separarle.
        #[test]
        fn one_list_contribution_is_not_two_scalar_contributions() {
            let container = one_list_contribution();
            let scalars = two_scalar_contributions();
            assert_eq!(
                container.get("x"),
                Some(&[BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])][..])
            );
            assert_eq!(scalars.get("x"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
            assert_ne!(container, scalars);
        }

        #[test]
        fn a_normal_promise_on_a_single_list_contribution_returns_the_whole_list() {
            assert_eq!(
                one_list_contribution().fulfill(&Promise::new("x")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn a_normal_promise_on_two_scalar_contributions_still_returns_the_last() {
            assert_eq!(
                two_scalar_contributions().fulfill(&Promise::new("x")).unwrap(),
                BlockValue::Int(2)
            );
        }

        #[test]
        fn a_multiple_promise_on_a_single_list_contribution_wraps_it() {
            assert_eq!(
                one_list_contribution().fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])])
            );
        }

        #[test]
        fn a_multiple_promise_on_two_scalar_contributions_returns_both() {
            assert_eq!(
                two_scalar_contributions().fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        /// Da contrapporre a `resolution::an_id_with_zero_contributions_is_unresolvable`: "un
        /// contributo che e' una lista vuota" e "nessun contributo" sono due cose diverse, che nel
        /// vecchio disegno collassavano nella stessa.
        #[test]
        fn an_empty_list_is_a_legitimate_value() {
            let map: PromiseMap = [("x", BlockValue::List(Vec::new()))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[BlockValue::List(Vec::new())][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), BlockValue::List(Vec::new()));
            assert_eq!(
                flat.fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![BlockValue::List(Vec::new())])
            );
        }

        #[test]
        fn a_nested_list_contribution_survives_intact() {
            let nested = BlockValue::List(vec![
                BlockValue::List(vec![BlockValue::Int(1)]),
                BlockValue::List(vec![BlockValue::Int(2)]),
            ]);
            let map: PromiseMap = [("x", nested.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[nested.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), nested);
        }

        /// Il fix non deve essere scritto sulla sola variante `List`: un `Set` e' un contenitore
        /// come gli altri e non e' mai stato sciolto, ma va pinnato perche' non lo diventi.
        #[test]
        fn a_set_contribution_survives_intact() {
            let set = BlockValue::Set(BTreeSet::from([BlockValue::Int(1), BlockValue::Int(2)]));
            let map: PromiseMap = [("x", set.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[set.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), set.clone());
            assert_eq!(flat.fulfill(&Promise::new("x[]")).unwrap(), BlockValue::List(vec![set]));
        }

        #[test]
        fn a_map_contribution_survives_intact() {
            let inner = BlockValue::Map(BTreeMap::from([
                ("a".to_string(), BlockValue::Int(1)),
                ("b".to_string(), BlockValue::Int(2)),
            ]));
            let map: PromiseMap = [("x", inner.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[inner.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), inner.clone());
            assert_eq!(flat.fulfill(&Promise::new("x[]")).unwrap(), BlockValue::List(vec![inner]));
        }

        /// Lo stesso bug, ma attraversato da un riferimento: `src` eredita **un** contributo (che
        /// e' una lista), non due.
        #[test]
        fn a_reference_to_an_id_whose_only_contribution_is_a_list_yields_the_list() {
            let list = BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]);
            let map: PromiseMap = [("src", promise("t")), ("t", list.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("src"), Some(&[list.clone()][..]));
            assert_eq!(flat.get("t"), Some(&[list.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("src")).unwrap(), list);
        }

        /// Round-trip: appiattire, reinserire in una multimappa e riappiattire non scioglie il
        /// contenitore. Nel vecchio disegno l'informazione si perdeva al primo giro, in modo
        /// irreversibile.
        #[test]
        fn flattening_a_list_contribution_twice_does_not_unwrap_it() {
            let map: PromiseMap = [
                ("contenitore", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])),
                ("separati", BlockValue::Int(1)),
                ("separati", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let once = map.flatten().unwrap();
            let reinserted: PromiseMap = once
                .iter()
                .flat_map(|(id, contributions)| {
                    contributions.iter().map(move |v| (id.clone(), v.clone()))
                })
                .collect();
            assert_eq!(reinserted.flatten().unwrap(), once);
            assert_ne!(once.get("contenitore"), once.get("separati"));
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
            assert_eq!(flat.get("source"), Some(&[promise("nowhere")][..]));
        }

        #[test]
        fn a_reference_to_an_id_without_contributions_leaves_the_promise() {
            let mut map = PromiseMap::new();
            map.push("source", Promise::new("vuoto"));
            map.entries.insert("vuoto".into(), Vec::new());
            assert_eq!(map.flatten().unwrap().get("source"), Some(&[promise("vuoto")][..]));
        }

        /// Conseguenza della decisione su C3 (2026-08-29): i `Null` sono gia' spariti quando lo
        /// splicing guarda il bersaglio, quindi un id di soli `Null` e' indistinguibile da un id
        /// senza contributi — la promessa resta pendente, non eredita nessun `Null`.
        #[test]
        fn a_reference_to_a_null_only_target_stays_pending() {
            let map: PromiseMap =
                [("source", promise("t")), ("t", BlockValue::Null)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&[promise("t")][..]));
            assert_eq!(flat.get("t"), None);
        }

        #[test]
        fn a_chain_that_ends_in_nothing_stops_on_the_pending_promise() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("b"), Some(&[promise("nowhere")][..]));
            assert_eq!(flat.get("a"), Some(&[promise("nowhere")][..]));
        }

        #[test]
        fn a_pending_reference_does_not_prevent_others_from_resolving() {
            let map: PromiseMap =
                [("a", promise("nowhere")), ("b", promise("c")), ("c", BlockValue::Int(3))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&[promise("nowhere")][..]));
            assert_eq!(flat.get("b"), Some(&[BlockValue::Int(3)][..]));
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

        /// Chiavi ripetute = contributi multipli per lo stesso id: `from_pairs` accumula, non
        /// sovrascrive.
        fn flat(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
            FlatPromiseMap::from_pairs(pairs)
        }

        #[test]
        fn a_scalar_value_resolves_to_itself() {
            let map = flat(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::from("Acme"));
        }

        /// Chi arriva dopo vince: e' l'ordine delle pagine.
        #[test]
        fn on_several_contributions_the_last_one_wins() {
            let map = flat(vec![
                ("fund", BlockValue::Int(1)),
                ("fund", BlockValue::Int(2)),
                ("fund", BlockValue::Int(3)),
            ]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(3));
        }

        #[test]
        fn a_multiple_promise_always_gets_a_list() {
            let single = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(
                single.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1)])
            );

            let several = flat(vec![("fund", BlockValue::Int(1)), ("fund", BlockValue::Int(2))]);
            assert_eq!(
                several.fulfill(&Promise::new("fund[]")).unwrap(),
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
        ///
        /// Dalla decisione su C3 (2026-08-29) la via interna e' cambiata — il `Null` viene
        /// scartato gia' da `flatten`, quindi l'id non compare affatto nella mappa appiattita —
        /// ma l'esito osservabile e' lo stesso di prima. Il test passa percio' da una
        /// [`FlatPromiseMap`] costruita a mano a una prodotta da `flatten`, che e' l'unico modo in
        /// cui un `Null` puo' presentarsi nella realta'.
        #[test]
        fn a_null_value_is_unresolvable() {
            let map: PromiseMap = [("fund", BlockValue::Null)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
            assert!(flat.fulfill(&Promise::new("fund[]")).is_err());
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
        fn pending_promise_contributions_are_discarded() {
            let map = flat(vec![
                ("fund", BlockValue::Int(1)),
                ("fund", promise("nowhere")),
                ("fund", BlockValue::Int(2)),
            ]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(2));
            assert_eq!(
                map.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn an_id_with_only_pending_contributions_is_unresolvable() {
            let map = flat(vec![("fund", promise("a")), ("fund", promise("b"))]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// Un id presente ma con zero contributi non puo' esistere in una mappa prodotta da
        /// `flatten` (vedi `flattening::the_flat_map_never_holds_an_empty_contribution_list`): lo
        /// si costruisce a mano, dall'interno del modulo, solo per fissare cosa farebbe `fulfill`
        /// se ci arrivasse. Da non confondere con
        /// `container_valued_contributions::an_empty_list_is_a_legitimate_value`.
        #[test]
        fn an_id_with_zero_contributions_is_unresolvable() {
            let mut map = FlatPromiseMap::new();
            map.entries.insert("fund".into(), Vec::new());
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
        /// multimappa un contributo alla volta, si appiattisce in se stessa.
        ///
        /// L'idempotenza regge anche con lo splicing: un contributo rimasto `Promise` punta per
        /// costruzione a un id **assente** dalla mappa appiattita, quindi al secondo giro resta
        /// pendente invece di innestare qualcosa di nuovo. La fixture include un contributo che e'
        /// esso stesso una `List`, cosi' la proprieta' copre anche il caso F2.
        #[test]
        fn flattening_is_idempotent() {
            let map: PromiseMap = [
                ("a", promise("b")),
                ("b", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
                ("c", BlockValue::from("x")),
                ("d", promise("nowhere")),
                ("e", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])),
            ]
            .into_iter()
            .collect();
            let once = map.flatten().unwrap();
            let reinserted: PromiseMap = once
                .iter()
                .flat_map(|(id, contributions)| {
                    contributions.iter().map(move |v| (id.clone(), v.clone()))
                })
                .collect();
            assert_eq!(reinserted.flatten().unwrap(), once);
        }

        /// Una catena lineare lunga si risolve tutta sullo stesso valore finale, e ogni id resta
        /// con **un solo** contributo: ogni anello riferisce un bersaglio che ne ha uno, quindi lo
        /// splicing non allunga nulla. (Con riferimenti ripetuti la lunghezza puo' invece
        /// raddoppiare a ogni anello — vedi
        /// `flattening::two_references_to_the_same_multi_contribution_target_splice_twice`.)
        /// Il costo di visita resta lineare grazie alla memoizzazione.
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
                assert_eq!(flat.get(&format!("id{i}")), Some(&[BlockValue::Int(42)][..]), "id{i}");
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
                assert_eq!(flat.get(&format!("src{i}")), Some(&[BlockValue::Int(7)][..]));
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

        /// Se nessun contributo e' una promessa, l'appiattimento non e' una riduzione ma
        /// l'identita': ogni id conserva **tutti** i suoi contributi, nell'ordine, elemento per
        /// elemento — contenitori compresi. E' la proprieta' che il vecchio disegno non poteva
        /// soddisfare, perche' con `n == 1` restituiva il contributo scalarizzato e con `n > 1`
        /// una `List` sintetica.
        #[test]
        fn without_promises_flattening_preserves_every_contribution() {
            for n_contributions in 1..8_usize {
                let mut map = PromiseMap::new();
                for i in 0..n_contributions {
                    map.push("scalari", i as i64);
                    map.push("contenitori", BlockValue::List(vec![BlockValue::Int(i as i64)]));
                    map.push("misti", i as i64);
                    map.push("misti", BlockValue::List(vec![BlockValue::Int(i as i64)]));
                }
                let flat = map.flatten().unwrap();
                assert_eq!(flat.len(), map.len(), "con {n_contributions} contributi");
                for (id, contributions) in map.iter() {
                    assert_eq!(
                        flat.get(id),
                        Some(contributions),
                        "id `{id}` con {n_contributions} contributi"
                    );
                }
            }
        }

        /// Su volume: un contributo-lista in posizione nota (non l'ultima) non viene ne' sciolto
        /// ne' spostato. Copre insieme ordine, non-scioglimento e assenza di appiattimenti
        /// accidentali.
        #[test]
        fn a_long_list_of_contributions_keeps_a_list_valued_one_in_place() {
            const TOTAL: usize = 200;
            const CONTAINER_AT: usize = 137;
            let container = BlockValue::List(vec![BlockValue::Int(-1), BlockValue::Int(-2)]);

            let mut map = PromiseMap::new();
            for i in 0..TOTAL {
                if i == CONTAINER_AT {
                    map.push("x", container.clone());
                } else {
                    map.push("x", i as i64);
                }
            }

            let flat = map.flatten().unwrap();
            let contributions = flat.get("x").expect("l'id ha contributi");
            assert_eq!(contributions.len(), TOTAL);
            assert_eq!(contributions[CONTAINER_AT], container);
            assert_eq!(contributions[0], BlockValue::Int(0));

            let expanded = flat.fulfill(&Promise::new("x[]")).unwrap();
            match expanded {
                BlockValue::List(values) => {
                    assert_eq!(values.len(), TOTAL);
                    assert_eq!(values[CONTAINER_AT], container);
                    assert_eq!(values[TOTAL - 1], BlockValue::Int((TOTAL - 1) as i64));
                }
                other => panic!("attesa una lista, trovato {other:?}"),
            }

            assert_eq!(
                flat.fulfill(&Promise::new("x")).unwrap(),
                BlockValue::Int((TOTAL - 1) as i64)
            );
        }
    }
}
