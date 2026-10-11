//! Integration test of the three standard pipes of the **document** classification, written only
//! against `freeports::api` (plus `PdfLine`, which every test of this folder reaches the same way).
//!
//! What is verified here is the interaction: a [`Pipeline`] built by hand from
//! `PdfExtractDocClassifyStandard`, `TextFilterDocClassifyStandard` and
//! `DeserializerDocClassifyStandard`, applied to a page, gives the outcome §6 of the specification
//! asks for — one `Extracted::DocClass` carrying the pattern, or `None`, or an error when two pipes
//! disagree. It is also the proof that the three pipes and `FormatPattern` are reachable from
//! outside the crate, which the unit tests cannot show.

use std::sync::Arc;

use freeports::api::core::{Extracted, FilterData, Page, PipeError, Pipeline};
use freeports::api::formats_repo::{FormatPattern, FormatPatternError};
use freeports::api::standard_funcs::deserialize::DeserializerDocClassifyStandard;
use freeports::api::standard_funcs::pdf_extract::PdfExtractDocClassifyStandard;
use freeports::api::standard_funcs::text_filter::TextFilterDocClassifyStandard;
use freeports::api::utils::pdf_extract::{InputPdfLineSet, pdfline_selection_from_dict};
use freeports::formats_utils::pdf_extract::pdf_line::PdfLine;
use freeports::formats_utils::pdf_extract::select::relative::PdfLineSelection;

fn pattern(text: &str) -> FormatPattern {
    text.parse().expect("the fixture pattern must be valid")
}

fn text(t: &str) -> PdfLineSelection {
    pdfline_selection_from_dict(&InputPdfLineSet { text: Some(t.to_string()), ..Default::default() })
        .expect("a text selection is always valid")
}

fn cover() -> Page {
    Page::new(
        1,
        (595.0, 842.0),
        vec![
            PdfLine::new("TrebuchetMS", 26.0, "Relazione di gestione", (0.0, 0.0, 300.0, 26.0)),
            PdfLine::new("Arial", 10.0, "Mediolanum Gestione Fondi SGR", (0.0, 40.0, 300.0, 50.0)),
        ],
        vec![],
    )
}

/// A pipeline as a YAML leaf step becomes one (§6): N extraction pipes, the filter, the
/// deserializer.
fn pipeline(steps: Vec<(Vec<PdfLineSelection>, &str)>) -> Pipeline {
    let mut pipeline = Pipeline::new("document_classify");
    for (selections, name) in steps {
        pipeline.pdf_extract.push(Arc::new(PdfExtractDocClassifyStandard::new(selections, pattern(name))));
    }
    pipeline.text_filter.push(Arc::new(TextFilterDocClassifyStandard));
    pipeline.deserialize.push(Arc::new(DeserializerDocClassifyStandard));
    pipeline
}

fn outcome(pipeline: &Pipeline, page: &Page) -> Result<Vec<Extracted>, PipeError> {
    pipeline.apply(page, &FilterData::EMPTY)
}

mod outcomes {
    use super::*;

    #[test]
    fn a_partial_pattern_crosses_the_three_pipes() {
        let p = pipeline(vec![(vec![text("Relazione"), text("Mediolanum Gestione")], "MEDIOLANUM-IT24.*")]);
        assert_eq!(outcome(&p, &cover()).unwrap(), vec![Extracted::DocClass(Some(pattern("MEDIOLANUM-IT24.*")))]);
    }

    #[test]
    fn a_complete_pattern_crosses_the_three_pipes() {
        let p = pipeline(vec![(vec![text("Mediolanum")], "MEDIOLANUM-IT24.A")]);
        let out = outcome(&p, &cover()).unwrap();
        assert_eq!(out, vec![Extracted::DocClass(Some(pattern("MEDIOLANUM-IT24.A")))]);
        assert!(matches!(&out[0], Extracted::DocClass(Some(p)) if p.is_complete()));
    }

    #[test]
    fn nothing_recognised_is_one_explicit_empty_doc_class() {
        let p = pipeline(vec![(vec![text("Amundi")], "AMUNDI-EN24"), (vec![text("Annual Report")], "MEDIOLANUM-EN")]);
        assert_eq!(outcome(&p, &cover()).unwrap(), vec![Extracted::DocClass(None)]);
    }

    #[test]
    fn one_recognising_pipe_among_failing_ones_decides() {
        let p = pipeline(vec![
            (vec![text("Amundi")], "AMUNDI-EN24"),
            (vec![text("Mediolanum")], "MEDIOLANUM"),
            (vec![text("Fideuram")], "FIDEURAM-IT23@*"),
        ]);
        assert_eq!(outcome(&p, &cover()).unwrap(), vec![Extracted::DocClass(Some(pattern("MEDIOLANUM")))]);
    }

    #[test]
    fn zero_selections_make_an_unconditional_step() {
        let p = pipeline(vec![(Vec::new(), "ASTERIA-EN23")]);
        let image_cover = Page::new(1, (595.0, 842.0), vec![], vec![]);
        assert_eq!(outcome(&p, &image_cover).unwrap(), vec![Extracted::DocClass(Some(pattern("ASTERIA-EN23")))]);
    }

    #[test]
    fn two_pipes_recognising_the_same_pattern_give_it_once() {
        let p = pipeline(vec![(vec![text("Relazione")], "MEDIOLANUM"), (vec![text("Mediolanum")], "MEDIOLANUM")]);
        assert_eq!(outcome(&p, &cover()).unwrap(), vec![Extracted::DocClass(Some(pattern("MEDIOLANUM")))]);
    }

    #[test]
    fn two_pipes_recognising_different_patterns_fail_the_pipeline_naming_the_filter() {
        let p = pipeline(vec![(vec![text("Relazione")], "MEDIOLANUM"), (vec![text("Mediolanum")], "AMUNDI")]);
        let err = outcome(&p, &cover()).unwrap_err();
        assert_eq!(err.pipe(), "TextFilterDocClassifyStandard");
        assert!(!err.is_page_failure());
    }

    /// A pipeline with no extraction pipe is a wiring mistake, as in the page classification.
    #[test]
    fn a_pipeline_without_extraction_pipes_is_an_error() {
        let p = pipeline(Vec::new());
        assert!(outcome(&p, &cover()).is_err());
    }
}

mod public_surface {
    use super::*;

    /// `FormatPatternError` is the companion of `FormatPattern` in the facade: a caller parsing a
    /// pattern must be able to name the error it gets.
    #[test]
    fn the_pattern_and_its_error_are_reachable() {
        let err: FormatPatternError = "-IT24".parse::<FormatPattern>().unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
