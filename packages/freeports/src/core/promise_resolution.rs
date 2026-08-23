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
        let mut in_corso = Vec::new();
        for id in self.entries.keys() {
            self.resolve_id(id, &mut in_corso, &mut resolved)?;
        }
        Ok(FlatPromiseMap { entries: resolved })
    }

    /// Visita in profondita' un singolo id. `in_corso` e' il cammino corrente (rileva i cicli),
    /// `resolved` e' la memoizzazione (ogni id si appiattisce una volta sola, anche se molti altri
    /// lo riferiscono).
    fn resolve_id(
        &self,
        id: &str,
        in_corso: &mut Vec<String>,
        resolved: &mut BTreeMap<String, BlockValue>,
    ) -> Result<(), PromiseError> {
        if resolved.contains_key(id) {
            return Ok(());
        }
        if let Some(inizio) = in_corso.iter().position(|visitato| visitato == id) {
            let mut chain: Vec<String> = in_corso[inizio..].to_vec();
            chain.push(id.to_string());
            return Err(PromiseError::Circular { chain });
        }
        // Un id di cui non sappiamo nulla non e' un errore: chi lo riferisce si tiene la promessa.
        let Some(contributi) = self.entries.get(id) else {
            return Ok(());
        };

        in_corso.push(id.to_string());
        let mut appiattiti = Vec::with_capacity(contributi.len());
        for contributo in contributi {
            match contributo {
                BlockValue::Promise(promise) => {
                    self.resolve_id(promise.id(), in_corso, resolved)?;
                    match resolved.get(promise.id()) {
                        Some(valore) => appiattiti.push(valore.clone()),
                        // L'id riferito non esiste, o esiste senza contributi: la promessa resta.
                        None => appiattiti.push(contributo.clone()),
                    }
                }
                altro => appiattiti.push(altro.clone()),
            }
        }
        in_corso.pop();

        let unico = if appiattiti.len() == 1 {
            appiattiti.pop()
        } else if appiattiti.is_empty() {
            None
        } else {
            Some(BlockValue::List(appiattiti))
        };
        if let Some(valore) = unico {
            resolved.insert(id.to_string(), valore);
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
        let registrato = self.entries.get(promise.id()).ok_or_else(|| promise.unresolved())?;
        let candidati: Vec<&BlockValue> = match registrato {
            BlockValue::Null | BlockValue::Promise(_) => Vec::new(),
            BlockValue::List(valori) => valori.iter().filter(|v| !v.is_promise()).collect(),
            scalare => vec![scalare],
        };
        if promise.multiple() {
            if candidati.is_empty() {
                return Err(promise.unresolved());
            }
            return Ok(BlockValue::List(candidati.into_iter().cloned().collect()));
        }
        candidati.last().map(|v| (*v).clone()).ok_or_else(|| promise.unresolved())
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

    mod multimappa {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn accumula_i_contributi_in_ordine() {
            let mut map = PromiseMap::new();
            map.push("fund", 1_i64);
            map.push("fund", 2_i64);
            assert_eq!(map.get("fund"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
        }

        #[test]
        fn merge_versa_piu_coppie_alla_volta() {
            let mut map = PromiseMap::new();
            map.merge([("a", 1_i64), ("b", 2_i64)]);
            map.merge([("a", 3_i64)]);
            assert_eq!(map.get("a"), Some(&[BlockValue::Int(1), BlockValue::Int(3)][..]));
            assert_eq!(map.get("b"), Some(&[BlockValue::Int(2)][..]));
            assert_eq!(map.len(), 2);
        }

        #[test]
        fn una_mappa_nuova_e_vuota() {
            let map = PromiseMap::new();
            assert!(map.is_empty());
            assert_eq!(map.len(), 0);
            assert_eq!(map.get("assente"), None);
        }

        #[test]
        fn si_costruisce_da_un_iteratore() {
            let map: PromiseMap = [("a", 1_i64), ("a", 2_i64), ("b", 3_i64)].into_iter().collect();
            assert_eq!(map.get("a"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
            assert_eq!(map.len(), 2);
        }

        #[test]
        fn itera_in_ordine_di_chiave() {
            let map: PromiseMap = [("z", 1_i64), ("a", 2_i64), ("m", 3_i64)].into_iter().collect();
            let chiavi: Vec<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(chiavi, vec!["a", "m", "z"]);
        }
    }

    mod appiattimento {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn un_contributo_solo_resta_scalare() {
            let map: PromiseMap = [("x", 42_i64)].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("x"), Some(&BlockValue::Int(42)));
        }

        #[test]
        fn piu_contributi_diventano_una_lista() {
            let map: PromiseMap = [("x", 1_i64), ("x", 2_i64), ("x", 3_i64)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)]))
            );
        }

        #[test]
        fn una_mappa_vuota_si_appiattisce_in_una_mappa_vuota() {
            assert!(PromiseMap::new().flatten().unwrap().is_empty());
        }

        #[test]
        fn un_id_senza_contributi_sparisce() {
            let mut map = PromiseMap::new();
            map.entries.insert("vuoto".into(), Vec::new());
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("vuoto"), None);
            assert!(flat.is_empty());
        }

        #[test]
        fn risolve_un_riferimento_semplice() {
            let map: PromiseMap =
                [("source", promise("target")), ("target", BlockValue::Int(99))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&BlockValue::Int(99)));
            assert_eq!(flat.get("target"), Some(&BlockValue::Int(99)));
        }

        #[test]
        fn risolve_una_catena_di_riferimenti() {
            let map: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", BlockValue::Int(7))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&BlockValue::Int(7)));
            assert_eq!(flat.get("b"), Some(&BlockValue::Int(7)));
        }

        #[test]
        fn un_riferimento_a_una_lista_riceve_la_lista_intera() {
            let map: PromiseMap =
                [("src", promise("t")), ("t", BlockValue::Int(1)), ("t", BlockValue::Int(2))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            let attesa = BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]);
            assert_eq!(flat.get("src"), Some(&attesa));
            assert_eq!(flat.get("t"), Some(&attesa));
        }

        /// Caso che nel riferimento costava una passata a vuoto in piu': un id con due contributi
        /// che sono entrambi promesse. Qui e' una visita sola, e il risultato e' la lista dei due
        /// valori riferiti, nell'ordine di inserimento.
        #[test]
        fn un_id_con_due_contributi_promessi_diventa_la_lista_dei_valori() {
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
        fn i_contributi_non_promessi_restano_accanto_a_quelli_risolti() {
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
        fn i_flag_della_promessa_non_influenzano_l_appiattimento() {
            // Strict e multiple contano al momento di risolvere un'entita', non qui: `flatten`
            // sostituisce comunque il valore registrato per l'id riferito.
            let map: PromiseMap =
                [("src", promise("t[]!")), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&BlockValue::Int(1)));
        }

        #[test]
        fn non_scende_dentro_i_contenitori() {
            let annidata = BlockValue::List(vec![promise("t")]);
            let map: PromiseMap =
                [("src", annidata.clone()), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&annidata));
        }
    }

    /// La politica scelta dall'utente (2026-08-22): un riferimento che non porta da nessuna parte
    /// non e' un errore di appiattimento.
    mod riferimenti_pendenti {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn un_id_sconosciuto_lascia_la_promessa_al_suo_posto() {
            let map: PromiseMap = [("source", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&promise("nowhere")));
        }

        #[test]
        fn un_riferimento_a_un_id_senza_contributi_lascia_la_promessa() {
            let mut map = PromiseMap::new();
            map.push("source", Promise::new("vuoto"));
            map.entries.insert("vuoto".into(), Vec::new());
            assert_eq!(map.flatten().unwrap().get("source"), Some(&promise("vuoto")));
        }

        #[test]
        fn una_catena_che_finisce_nel_nulla_si_ferma_sulla_promessa_pendente() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("b"), Some(&promise("nowhere")));
            assert_eq!(flat.get("a"), Some(&promise("nowhere")));
        }

        #[test]
        fn un_riferimento_pendente_non_impedisce_agli_altri_di_risolversi() {
            let map: PromiseMap =
                [("a", promise("nowhere")), ("b", promise("c")), ("c", BlockValue::Int(3))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&promise("nowhere")));
            assert_eq!(flat.get("b"), Some(&BlockValue::Int(3)));
        }
    }

    mod cicli {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn un_riferimento_a_se_stesso_e_un_ciclo() {
            let map: PromiseMap = [("a", promise("a"))].into_iter().collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "a".into()] }
            );
        }

        #[test]
        fn due_id_che_si_riferiscono_a_vicenda_sono_un_ciclo() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("a"))].into_iter().collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] }
            );
        }

        #[test]
        fn la_catena_riportata_copre_l_intero_ciclo() {
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
        fn un_cammino_che_entra_in_un_ciclo_riporta_anche_l_ingresso() {
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
        fn la_catena_riportata_e_deterministica() {
            let dritta: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", promise("a"))].into_iter().collect();
            let rovescia: PromiseMap =
                [("c", promise("a")), ("b", promise("c")), ("a", promise("b"))].into_iter().collect();
            assert_eq!(dritta.flatten().unwrap_err(), rovescia.flatten().unwrap_err());
        }

        #[test]
        fn un_ciclo_fa_fallire_l_intero_appiattimento() {
            let map: PromiseMap =
                [("sano", BlockValue::Int(1)), ("a", promise("b")), ("b", promise("a"))]
                    .into_iter()
                    .collect();
            assert!(map.flatten().is_err());
        }
    }

    mod risoluzione {
        use super::*;
        use pretty_assertions::assert_eq;

        fn flat(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
            pairs.into_iter().collect()
        }

        #[test]
        fn un_valore_scalare_si_risolve_in_se_stesso() {
            let map = flat(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::from("Acme"));
        }

        #[test]
        fn su_una_lista_vince_l_ultimo_valore() {
            let map = flat(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)]),
            )]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(3));
        }

        #[test]
        fn una_promessa_multiple_ottiene_sempre_una_lista() {
            let scalare = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(
                scalare.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1)])
            );

            let lista = flat(vec![("fund", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]))]);
            assert_eq!(
                lista.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn un_id_assente_e_irrisolvibile() {
            let map = flat(vec![("altro", BlockValue::Int(1))]);
            assert_eq!(
                map.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }

        /// Un `Null` registrato conta come valore *assente*, non come valore nullo: e' la
        /// semantica del riferimento (`if value.is_none(): raise KeyError`).
        #[test]
        fn un_valore_null_e_irrisolvibile() {
            let map = flat(vec![("fund", BlockValue::Null)]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// Il seguito della politica sui riferimenti pendenti: una promessa sopravvissuta
        /// all'appiattimento non risolve nulla, ed e' qui che diventa un errore.
        #[test]
        fn un_valore_ancora_promessa_e_irrisolvibile() {
            let map = flat(vec![("fund", promise("nowhere"))]);
            assert_eq!(
                map.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }

        #[test]
        fn dentro_una_lista_le_promesse_pendenti_vengono_scartate() {
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
        fn una_lista_di_sole_promesse_pendenti_e_irrisolvibile() {
            let map = flat(vec![("fund", BlockValue::List(vec![promise("a"), promise("b")]))]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        #[test]
        fn una_lista_vuota_e_irrisolvibile() {
            let map = flat(vec![("fund", BlockValue::List(Vec::new()))]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// `strict` non cambia *se* una promessa si risolve, solo cosa succede a chi la contiene
        /// quando non si risolve — decisione che spetta a `promisable`.
        #[test]
        fn strict_non_cambia_l_esito_della_risoluzione() {
            let map = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(map.fulfill(&Promise::new("fund!")).unwrap(), BlockValue::Int(1));
            let vuota = FlatPromiseMap::new();
            assert_eq!(
                vuota.fulfill(&Promise::new("fund!")).unwrap_err(),
                vuota.fulfill(&Promise::new("fund")).unwrap_err()
            );
        }

        #[test]
        fn l_errore_nomina_l_id_senza_suffissi() {
            let vuota = FlatPromiseMap::new();
            assert_eq!(
                vuota.fulfill(&Promise::new("fund[]!")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }
    }

    /// Le proprieta' che devono valere su input generati, non solo sui casi scritti a mano.
    mod invarianti {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Appiattire due volte non cambia nulla: la mappa appiattita, reinserita in una
        /// multimappa, si appiattisce in se stessa.
        #[test]
        fn l_appiattimento_e_idempotente() {
            let map: PromiseMap = [
                ("a", promise("b")),
                ("b", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
                ("c", BlockValue::from("x")),
                ("d", promise("nowhere")),
            ]
            .into_iter()
            .collect();
            let una_volta = map.flatten().unwrap();
            let reinserita: PromiseMap =
                una_volta.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            assert_eq!(reinserita.flatten().unwrap(), una_volta);
        }

        /// Una catena lineare lunga si risolve tutta sullo stesso valore finale, senza esplosioni
        /// combinatorie: ogni id viene appiattito una volta sola grazie alla memoizzazione.
        #[test]
        fn una_catena_lunga_si_risolve_tutta_sul_valore_finale() {
            const LUNGHEZZA: usize = 500;
            let mut map = PromiseMap::new();
            for i in 0..LUNGHEZZA {
                map.push(format!("id{i}"), Promise::new(&format!("id{}", i + 1)));
            }
            map.push(format!("id{LUNGHEZZA}"), 42_i64);
            let flat = map.flatten().unwrap();
            for i in 0..=LUNGHEZZA {
                assert_eq!(flat.get(&format!("id{i}")), Some(&BlockValue::Int(42)), "id{i}");
            }
        }

        /// Molti id che puntano tutti allo stesso bersaglio: nessun ciclo, tutti risolti.
        #[test]
        fn molti_riferimenti_allo_stesso_id_si_risolvono_tutti() {
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
        fn un_ciclo_lungo_viene_rilevato() {
            const LUNGHEZZA: usize = 300;
            let mut map = PromiseMap::new();
            for i in 0..LUNGHEZZA {
                map.push(format!("id{i:03}"), Promise::new(&format!("id{:03}", (i + 1) % LUNGHEZZA)));
            }
            match map.flatten().unwrap_err() {
                PromiseError::Circular { chain } => assert_eq!(chain.len(), LUNGHEZZA + 1),
                altro => panic!("atteso un ciclo, trovato {altro:?}"),
            }
        }

        /// Se nessun contributo e' una promessa, l'appiattimento e' pura riduzione: ogni id
        /// conserva i suoi valori, nell'ordine, e nessuno puo' fallire.
        #[test]
        fn senza_promesse_l_appiattimento_conserva_i_contributi() {
            for n_contributi in 1..8_usize {
                let mut map = PromiseMap::new();
                for i in 0..n_contributi {
                    map.push("x", i as i64);
                }
                let flat = map.flatten().unwrap();
                let atteso = if n_contributi == 1 {
                    BlockValue::Int(0)
                } else {
                    BlockValue::List((0..n_contributi).map(|i| BlockValue::Int(i as i64)).collect())
                };
                assert_eq!(flat.get("x"), Some(&atteso), "con {n_contributi} contributi");
            }
        }
    }
}
