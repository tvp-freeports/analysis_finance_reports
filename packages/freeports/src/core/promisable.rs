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
    let mut multipli = Vec::new();

    // Fase 1: promesse normali, risolte sul posto.
    for (field, promise) in entity.pending() {
        if promise.multiple() {
            multipli.push((field, promise));
            continue;
        }
        match map.fulfill(&promise) {
            Ok(value) => assign(entity, field, value)?,
            Err(err) if promise.strict() => return Err(err.into()),
            Err(_) => return Ok(Fulfilled::Dropped),
        }
    }

    if multipli.is_empty() {
        return Ok(Fulfilled::InPlace);
    }

    // Fase 2: promesse multiple, una copia per valore.
    let mut espansioni = vec![entity.clone()];
    for (field, promise) in multipli {
        let valori = match map.fulfill(&promise) {
            Ok(v) => v,
            Err(err) if promise.strict() => return Err(err.into()),
            Err(_) => return Ok(Fulfilled::Dropped),
        };
        // `FlatPromiseMap::fulfill` su una promessa `multiple` restituisce sempre una `List` non
        // vuota; il ramo `altro` copre solo il caso in cui quel contratto cambiasse.
        let valori = match valori {
            BlockValue::List(items) => items,
            altro => vec![altro],
        };
        let mut prossime = Vec::with_capacity(espansioni.len() * valori.len());
        for base in &espansioni {
            for valore in &valori {
                let mut copia = base.clone();
                assign(&mut copia, field, valore.clone())?;
                prossime.push(copia);
            }
        }
        espansioni = prossime;
    }

    Ok(Fulfilled::Expanded(espansioni))
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
    struct Investimento {
        fondo: Promised<String>,
        quantita: Promised<i64>,
        nota: String,
    }

    impl Investimento {
        fn new(fondo: Promised<String>, quantita: Promised<i64>) -> Self {
            Investimento { fondo, quantita, nota: "fissa".into() }
        }

        fn risolto(fondo: &str, quantita: i64) -> Self {
            Investimento::new(Promised::Resolved(fondo.into()), Promised::Resolved(quantita))
        }
    }

    impl PromisableFields for Investimento {
        fn pending(&self) -> Vec<(&'static str, Promise)> {
            let mut out = Vec::new();
            if let Some(p) = self.fondo.pending() {
                out.push(("fondo", p.clone()));
            }
            if let Some(p) = self.quantita.pending() {
                out.push(("quantita", p.clone()));
            }
            out
        }

        fn resolve_field(
            &mut self,
            field: &'static str,
            value: BlockValue,
        ) -> Result<(), BlockValueError> {
            match field {
                "fondo" => self.fondo = Promised::Resolved(value.str_or_fail(field)?.to_string()),
                "quantita" => self.quantita = Promised::Resolved(value.int_or_fail(field)?),
                altro => return Err(BlockValueError::MissingField { field: altro.to_string() }),
            }
            Ok(())
        }
    }

    fn mappa(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
        pairs.into_iter().collect()
    }

    fn pendente(raw: &str) -> Promised<String> {
        Promised::Pending(Promise::new(raw))
    }

    fn pendente_i64(raw: &str) -> Promised<i64> {
        Promised::Pending(Promise::new(raw))
    }

    mod campo_promesso {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn distingue_risolto_da_pendente() {
            let risolto: Promised<i64> = Promised::Resolved(3);
            assert!(risolto.is_resolved());
            assert!(!risolto.is_pending());
            assert_eq!(risolto.resolved(), Some(&3));
            assert_eq!(risolto.pending(), None);

            let pendente = pendente_i64("x!");
            assert!(pendente.is_pending());
            assert!(!pendente.is_resolved());
            assert_eq!(pendente.resolved(), None);
            assert_eq!(pendente.pending(), Some(&Promise::new("x!")));
        }

        #[test]
        fn into_resolved_consuma_il_campo() {
            assert_eq!(Promised::Resolved("x".to_string()).into_resolved(), Some("x".to_string()));
            assert_eq!(pendente("x").into_resolved(), None);
        }

        #[test]
        fn map_trasforma_solo_il_valore_risolto() {
            assert_eq!(Promised::Resolved(2_i64).map(|v| v * 2), Promised::Resolved(4));
            assert_eq!(pendente_i64("x").map(|v| v * 2), Promised::Pending(Promise::new("x")));
        }

        #[test]
        fn si_costruisce_da_una_promessa() {
            let campo: Promised<i64> = Promise::new("x[]").into();
            assert_eq!(campo, Promised::Pending(Promise::new("x[]")));
        }

        #[test]
        fn serializza_come_valore_o_come_promessa() {
            assert_eq!(serde_json::to_string(&Promised::Resolved(3_i64)).unwrap(), "3");
            assert_eq!(serde_json::to_string(&pendente_i64("fund[]!")).unwrap(), "\"fund[]!\"");
        }
    }

    mod nessuna_promessa {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn un_entita_gia_risolta_resta_sul_posto_e_intatta() {
            let mut entita = Investimento::risolto("Acme", 10);
            let prima = entita.clone();
            assert_eq!(fulfill_promises(&mut entita, &FlatPromiseMap::new()).unwrap(), Fulfilled::InPlace);
            assert_eq!(entita, prima);
        }
    }

    mod fase_sul_posto {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn risolve_un_campo_promesso() {
            let mut entita = Investimento::new(pendente("fund"), Promised::Resolved(10));
            let map = mappa(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(fulfill_promises(&mut entita, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(entita.fondo, Promised::Resolved("Acme".into()));
        }

        #[test]
        fn risolve_piu_campi_nella_stessa_passata() {
            let mut entita = Investimento::new(pendente("fund"), pendente_i64("qty"));
            let map = mappa(vec![("fund", BlockValue::from("Acme")), ("qty", BlockValue::Int(7))]);
            assert_eq!(fulfill_promises(&mut entita, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(entita, Investimento::risolto("Acme", 7));
        }

        #[test]
        fn non_tocca_i_campi_non_promessi() {
            let mut entita = Investimento::new(pendente("fund"), Promised::Resolved(10));
            let map = mappa(vec![("fund", BlockValue::from("Acme"))]);
            fulfill_promises(&mut entita, &map).unwrap();
            assert_eq!(entita.nota, "fissa");
            assert_eq!(entita.quantita, Promised::Resolved(10));
        }

        #[test]
        fn su_una_lista_prende_l_ultimo_valore() {
            let mut entita = Investimento::new(pendente("fund"), Promised::Resolved(1));
            let map = mappa(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::from("Vecchio"), BlockValue::from("Nuovo")]),
            )]);
            fulfill_promises(&mut entita, &map).unwrap();
            assert_eq!(entita.fondo, Promised::Resolved("Nuovo".into()));
        }
    }

    mod promessa_irrisolvibile {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn non_strict_fa_sparire_l_entita() {
            let mut entita = Investimento::new(pendente("assente"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entita, &FlatPromiseMap::new()).unwrap(),
                Fulfilled::Dropped
            );
        }

        #[test]
        fn strict_e_un_errore() {
            let mut entita = Investimento::new(pendente("assente!"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entita, &FlatPromiseMap::new()).unwrap_err(),
                PromisableError::Promise(PromiseError::Unresolved { id: "assente".into() })
            );
        }

        /// La politica sui riferimenti pendenti chiude il cerchio qui: una promessa sopravvissuta
        /// all'appiattimento (`promise_resolution`) diventa un drop o un errore a seconda di
        /// `strict`, non un errore di appiattimento.
        #[test]
        fn una_promessa_sopravvissuta_all_appiattimento_si_comporta_come_un_id_assente() {
            let map = mappa(vec![("fund", BlockValue::Promise(Promise::new("nowhere")))]);

            let mut non_strict = Investimento::new(pendente("fund"), Promised::Resolved(1));
            assert_eq!(fulfill_promises(&mut non_strict, &map).unwrap(), Fulfilled::Dropped);

            let mut strict = Investimento::new(pendente("fund!"), Promised::Resolved(1));
            assert!(fulfill_promises(&mut strict, &map).is_err());
        }

        #[test]
        fn un_valore_null_conta_come_assente() {
            let map = mappa(vec![("fund", BlockValue::Null)]);
            let mut entita = Investimento::new(pendente("fund"), Promised::Resolved(1));
            assert_eq!(fulfill_promises(&mut entita, &map).unwrap(), Fulfilled::Dropped);
        }

        #[test]
        fn una_multiple_non_strict_irrisolvibile_fa_sparire_l_entita() {
            let mut entita = Investimento::new(pendente("assente[]"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entita, &FlatPromiseMap::new()).unwrap(),
                Fulfilled::Dropped
            );
        }

        #[test]
        fn una_multiple_strict_irrisolvibile_e_un_errore() {
            let mut entita = Investimento::new(pendente("assente[]!"), Promised::Resolved(1));
            assert!(fulfill_promises(&mut entita, &FlatPromiseMap::new()).is_err());
        }

        /// Il drop vince sull'espansione: se un campo normale non si risolve, la fase 2 non parte
        /// nemmeno.
        #[test]
        fn un_campo_normale_irrisolvibile_impedisce_l_espansione() {
            let mut entita = Investimento::new(pendente("assente"), pendente_i64("qty[]"));
            let map = mappa(vec![(
                "qty",
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]),
            )]);
            assert_eq!(fulfill_promises(&mut entita, &map).unwrap(), Fulfilled::Dropped);
        }
    }

    mod fase_di_espansione {
        use super::*;
        use pretty_assertions::assert_eq;

        fn espansioni(esito: Fulfilled<Investimento>) -> Vec<Investimento> {
            match esito {
                Fulfilled::Expanded(v) => v,
                altro => panic!("attesa un'espansione, trovato {altro:?}"),
            }
        }

        #[test]
        fn una_copia_per_valore() {
            let mut entita = Investimento::new(pendente("fund[]"), Promised::Resolved(1));
            let map = mappa(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::from("A"), BlockValue::from("B"), BlockValue::from("C")]),
            )]);
            let copie = espansioni(fulfill_promises(&mut entita, &map).unwrap());
            let nomi: Vec<&str> = copie.iter().filter_map(|c| c.fondo.resolved()).map(String::as_str).collect();
            assert_eq!(nomi, vec!["A", "B", "C"]);
        }

        /// Un solo valore resta comunque un'espansione, non un `InPlace`: il chiamante deve
        /// sostituire l'entita' con il contenuto della lista in entrambi i casi.
        #[test]
        fn un_valore_solo_produce_comunque_un_espansione() {
            let mut entita = Investimento::new(pendente("fund[]"), Promised::Resolved(1));
            let map = mappa(vec![("fund", BlockValue::from("A"))]);
            let copie = espansioni(fulfill_promises(&mut entita, &map).unwrap());
            assert_eq!(copie, vec![Investimento::risolto("A", 1)]);
        }

        #[test]
        fn due_campi_multiple_danno_il_prodotto_cartesiano() {
            let mut entita = Investimento::new(pendente("fund[]"), pendente_i64("qty[]"));
            let map = mappa(vec![
                ("fund", BlockValue::List(vec![BlockValue::from("A"), BlockValue::from("B")])),
                ("qty", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)])),
            ]);
            let copie = espansioni(fulfill_promises(&mut entita, &map).unwrap());
            assert_eq!(copie.len(), 6);
            let coppie: Vec<(&str, i64)> = copie
                .iter()
                .filter_map(|c| Some((c.fondo.resolved()?.as_str(), *c.quantita.resolved()?)))
                .collect();
            // Il campo che compare per primo in `pending` varia piu' lentamente.
            assert_eq!(
                coppie,
                vec![("A", 1), ("A", 2), ("A", 3), ("B", 1), ("B", 2), ("B", 3)]
            );
        }

        /// L'ordine delle due fasi, reso osservabile: il campo normale e' gia' risolto in *ogni*
        /// copia, quindi e' stato risolto una volta sola, prima dell'espansione.
        #[test]
        fn le_copie_portano_gia_i_campi_risolti_nella_prima_fase() {
            let mut entita = Investimento::new(pendente("fund"), pendente_i64("qty[]"));
            let map = mappa(vec![
                ("fund", BlockValue::from("Acme")),
                ("qty", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])),
            ]);
            let copie = espansioni(fulfill_promises(&mut entita, &map).unwrap());
            assert_eq!(copie, vec![Investimento::risolto("Acme", 1), Investimento::risolto("Acme", 2)]);
        }

        #[test]
        fn una_multiple_su_un_valore_scalare_produce_una_copia_sola() {
            let mut entita = Investimento::new(Promised::Resolved("Acme".into()), pendente_i64("qty[]"));
            let map = mappa(vec![("qty", BlockValue::Int(9))]);
            assert_eq!(
                espansioni(fulfill_promises(&mut entita, &map).unwrap()),
                vec![Investimento::risolto("Acme", 9)]
            );
        }

        #[test]
        fn le_copie_sono_indipendenti_fra_loro() {
            let mut entita = Investimento::new(pendente("fund[]"), Promised::Resolved(1));
            let map = mappa(vec![(
                "fund",
                BlockValue::List(vec![BlockValue::from("A"), BlockValue::from("B")]),
            )]);
            let mut copie = espansioni(fulfill_promises(&mut entita, &map).unwrap());
            copie[0].nota = "cambiata".into();
            assert_eq!(copie[1].nota, "fissa");
        }
    }

    mod errori_di_tipo {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn un_valore_del_tipo_sbagliato_nomina_il_campo() {
            let mut entita = Investimento::new(pendente("fund"), Promised::Resolved(1));
            let map = mappa(vec![("fund", BlockValue::Int(3))]);
            let err = fulfill_promises(&mut entita, &map).unwrap_err();
            assert_eq!(
                err,
                PromisableError::Field {
                    field: "fondo",
                    source: BlockValueError::TypeMismatch {
                        field: "fondo".into(),
                        expected: "str",
                        found: "int",
                    },
                }
            );
            assert_eq!(err.to_string(), "field 'fondo': field 'fondo' expected str, found int");
        }

        #[test]
        fn un_errore_di_tipo_in_espansione_interrompe_tutto() {
            let mut entita = Investimento::new(Promised::Resolved("Acme".into()), pendente_i64("qty[]"));
            let map = mappa(vec![(
                "qty",
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::from("non un numero")]),
            )]);
            let err = fulfill_promises(&mut entita, &map).unwrap_err();
            assert!(matches!(err, PromisableError::Field { field: "quantita", .. }), "{err:?}");
        }
    }
}
