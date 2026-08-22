//! `freeports` — motore di estrazione dati da report finanziari.
//!
//! Riscrittura in Rust di `packages/freeports_core`. L'albero **interno** dei moduli e' quello
//! dichiarato qui sotto; la **API pubblica** e' data dalle sole re-export del modulo `api`, che
//! e' l'unica superficie garantita per chi usa la libreria.
//!
//! Vedi `PLAN.md` per il piano di migrazione, le decisioni di design e lo stile dei test.

// --- albero interno (dev-facing) -------------------------------------------
pub mod cli;
pub mod commons;
pub mod core;
pub mod formats_repo;
pub mod formats_utils;
pub mod input;
pub mod output;

// --- API pubblica ----------------------------------------------------------
// Abilitata milestone per milestone, man mano che i moduli esistono davvero.
// Vedi `PLAN.md`, sezione 9.
pub mod api;
