//! Shim di `freeports.utils`: le utilità che un autore di formato compone nei propri pipe.
//!
//! Tre sottomoduli, uno per segmento della pipeline, esattamente come nel riferimento:
//! [`pdf_extract`] (selezioni di righe, geometria, tabelle), [`text_filter`] (normalizzazione,
//! confronto fra nomi di fondo, valute) e [`deserialize`] (i cast e i due decoratori che
//! restringono un deserializer a certi tipi di blocco).

pub mod deserialize;
pub mod pdf_extract;
pub mod text_filter;
