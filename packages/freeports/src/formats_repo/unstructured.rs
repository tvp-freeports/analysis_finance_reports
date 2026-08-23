//! Il livello unstructured: pipeline definite in Python dall'autore del formato.
//!
//! "Unstructured" significa che l'algoritmo di un segmento è unico per quel formato e non si
//! lascia parametrizzare: l'unico modo di esprimerlo è scriverne il codice, e quel codice vive nel
//! repo formati, non nella libreria. È uno dei due punti di contatto con Python del crate
//! (`PLAN.md` §3), insieme al caricamento del PDF.
//!
//! - [`loader`] trova e importa il modulo Python del formato e ne legge `pipelines` e
//!   `compute_page_class`;
//! - [`py_pipe`] avvolge i callable che ne escono nei trait dei pipe, così che il motore non
//!   distingua un pipe d'autore da uno nativo.
//!
//! Vedi il doc-comment di [`loader`] per il limite di questa fase (nessun binding Python, quindi
//! nessun repo formati reale caricabile) e per il contratto duck-typed che lo aggira.

pub mod loader;
pub mod py_pipe;
