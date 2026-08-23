//! OptionallyRelative e macchinario generico per selezioni relative.
//!
//! Porting verbatim (`PLAN.md` §0/§12 D14) della meta' generica del vecchio
//! `freeports_core::formats_utils::pdf_extract::select::relative` (la parte che *non*
//! menziona `SelectPdfLineSet`/`RelativeSelectPdfLineSet`), rilocata un livello sopra
//! (`pdf_extract::relative`, sibling di `pdf_extract::select` e `pdf_extract::pdf_line`) cosi'
//! da poter essere importata sia da `select::relative` (le selezioni relative vere e proprie)
//! sia, in futuro, da altri moduli di `pdf_extract` senza passare da `select`.
//!
//! **Decisione R3 (`PLAN.md`)**: `RelativeInfo<V>` **non** e' generico sul tipo di contesto —
//! `contextualize` prende `&[PdfLine]`, punto, esattamente come nel riferimento. Non e' un
//! `RelativeInfo<V, Ctx>` generico: il contesto di una selezione "relativa" in questo crate e'
//! sempre e solo "le altre righe della stessa pagina".
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub trait RelativeInfo<V> { fn contextualize(self, lines: &[PdfLine]) -> V; }`.
//! - `pub enum OptionallyRelative<V,R> { Absolute(V), Relative(R) }`, con un `impl<V,R> Clone`
//!   manuale (bound `V: Clone, R: Clone`) — verbatim dal riferimento, che lo scrive a mano
//!   invece di `#[derive(Clone)]` (nessuna differenza di comportamento, solo di stile: il
//!   riferimento lo fa per poter scrivere `#[derive(Debug)]` in futuro senza che `derive`
//!   costringa `V`/`R` a implementarlo entrambi contemporaneamente — vedi i commenti `// #[derive(Debug)]`
//!   lasciati nel riferimento).
//! - `impl<V,R> RelativeInfo<V> for OptionallyRelative<V,R> where R: RelativeInfo<V>`:
//!   `Absolute(v).contextualize(lines) == v` per qualunque `lines` (il contesto e' ignorato);
//!   `Relative(r).contextualize(lines)` delega a `r.contextualize(lines)`.

use super::pdf_line::PdfLine;

/// Un valore "contestualizzabile": data la lista delle altre righe della pagina, produce un `V`.
pub trait RelativeInfo<V> {
    fn contextualize(self, lines: &[PdfLine]) -> V;
}

/// Un valore assoluto, oppure relativo (contestualizzabile) al contesto della pagina.
// #[derive(Debug)]
pub enum OptionallyRelative<V, R> {
    Absolute(V),
    Relative(R),
}

impl<V, R> Clone for OptionallyRelative<V, R>
where
    V: Clone,
    R: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Absolute(a) => Self::Absolute(a.clone()),
            Self::Relative(a) => Self::Relative(a.clone()),
        }
    }
}

impl<V, R> RelativeInfo<V> for OptionallyRelative<V, R>
where
    R: RelativeInfo<V>,
{
    fn contextualize(self, lines: &[PdfLine]) -> V {
        match self {
            Self::Absolute(a) => a,
            Self::Relative(ra) => ra.contextualize(lines),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::pdf_line::PdfLine;

    /// Riga PDF di comodo per i test: il contenuto non conta, solo la lunghezza dello slice
    /// passato a `contextualize` (usata per dimostrare che l'implementazione relativa riceve
    /// davvero il contesto).
    fn some_lines(n: usize) -> Vec<PdfLine> {
        (0..n)
            .map(|i| PdfLine::new("Arial", 10.0, &format!("line {i}"), (0.0, i as f32, 1.0, i as f32 + 1.0)))
            .collect()
    }

    /// Implementazione minimale di `RelativeInfo<usize>` usata solo nei test: restituisce il
    /// numero di righe del contesto, cosi' da poter distinguere "il contesto e' stato letto" da
    /// "il contesto e' stato ignorato" (il comportamento di `Absolute`).
    #[derive(Clone, PartialEq, Debug)]
    struct CountLines;

    impl RelativeInfo<usize> for CountLines {
        fn contextualize(self, lines: &[PdfLine]) -> usize {
            lines.len()
        }
    }

    mod optionally_relative {
        use super::*;

        #[test]
        fn absolute_variant_ignores_the_context_entirely() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Absolute(42);
            assert_eq!(value.contextualize(&some_lines(7)), 42);
        }

        #[test]
        fn absolute_variant_ignores_an_empty_context_too() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Absolute(42);
            assert_eq!(value.contextualize(&[]), 42);
        }

        #[test]
        fn relative_variant_delegates_contextualize_to_the_inner_r() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Relative(CountLines);
            assert_eq!(value.contextualize(&some_lines(3)), 3);
        }

        #[test]
        fn clone_of_absolute_variant_contextualizes_to_the_same_value() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Absolute(9);
            let cloned = value.clone();
            let lines = some_lines(2);
            assert_eq!(value.contextualize(&lines), cloned.contextualize(&lines));
        }

        #[test]
        fn clone_of_relative_variant_contextualizes_to_the_same_value() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Relative(CountLines);
            let cloned = value.clone();
            let lines = some_lines(5);
            assert_eq!(value.contextualize(&lines), cloned.contextualize(&lines));
        }
    }
}
