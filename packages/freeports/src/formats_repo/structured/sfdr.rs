//! The structured SFDR pipelines: a periodic annex in **one row of CSV**.
//!
//! Sixteen of the collection's pipelines read the SFDR article from the European periodic annex
//! with the same algorithm, and the three segments of all sixteen carry **three parameters between
//! them**: the language, whether the product name has to be read down the label's column, and a
//! word to put in front of it. That is the condition for a datum to belong to this level — the
//! majority of a data type read by one algorithm — and the annex meets it more plainly than
//! anything else in the repository.
//!
//! # The row is a default, and the whole of it is one cell
//!
//! `Language` is the only required cell, and a format whose annex is laid out the European way is
//! that one cell. Whoever adds a format therefore has the SFDR article working before having
//! measured anything: the words of the annex belong to the annex and the engine already knows
//! them, in four languages.
//!
//! # What the promotion fixes, and it is not only the count of rows
//!
//! The semistructured form writes `language` **twice** — once for the `pdf_extract`, once for the
//! `text_filter` — and nothing checks that the two agree. A format whose two halves disagree reads
//! the tick in one language and strips the label in another, which produces an article for a fund
//! whose name still carries `Product name:`. Here it is one cell, and the question cannot arise.
//!
//! # What is deliberately **not** a column
//!
//! The nine anchor overrides (`question`, `affirmative`, `negative`, `made`, `promoted`, `title`,
//! `product`, `identifier`, `band`) stay in the semistructured form, and so do the two demands on
//! the funds. None of the sixteen measured annexes uses one — the anchors are the annex's own
//! words, not the format's — and a level meant to be *the default guess* is not the place for the
//! cell a format needs when it departs from the standard. A format that has to override an anchor
//! has measured its page, and it writes a YAML block that can say why.
//!
//! # The product prefixes are a table of their own, and the reasons are here
//!
//! What has to come off the product name is a list, ordered, of literals and patterns, so it is a
//! second table joined on the id — the shape `investments/deselection_lists.csv` already has. A
//! CSV cell carries no prose, so the measurements that chose those rows are written here:
//!
//! | format | what comes off, and why |
//! |---|---|
//! | AMUNDI-EN24 · AMUNDI-IT24 | the label, and `AMUNDI FUNDS`: the annex writes the umbrella in front of the sub-fund and the holdings do not (`R24`) |
//! | ASTERIA-EN24 | the label — the colon is **doubled** on one page — and the qualification in brackets that closes every name, `(the "Sub-Fund")` |
//! | CARNE-EN23 · FINECO-EN23@IR · FINECO-EN23@LUX · KAIROS-EN23's second layout | the label, written `Product name.*:` because the line carries something between the word and the colon (`R27`) |
//! | DANSKEINVEST-EN24 | the label, and `Danske Invest SICAV` for the same reason as AMUNDI's umbrella |
//! | MEDIOLANUM-IT24.A | a **literal** label, then the bracketed qualification, then `, un comparto …`: the order counts, and the first is a literal because a label written as a pattern would have to escape itself |
//! | MEDIOLANUM-IT24.B | the label, and the umbrella |
//! | UBS-EN23 | the label alone |
//!
//! # What the sixteen pages are like, and why none of it is a cell
//!
//! The measurements that the promoted YAML blocks carried, kept because they are the argument that
//! these formats need **no** configuration beyond the three cells:
//!
//! - **the tick is drawn in five different ways** across the sixteen — a text `X` in Calibri-Bold
//!   (ANIMA-EN23, AMUNDI-IT24 on all twenty-five of its annexes), in Arial-BoldMT (CARNE-EN23), the
//!   `3` of Wingdings 2 (UBS-EN23 p189 alone, the same form as the rest of its document), a
//!   `funddsymbol` glyph and an image (FINECO-EN23@IR uses all three, on twenty-eight, six and one
//!   annex) — and **not one of them is declared**: the five recognisers of `A3` are all on by
//!   default, and that is what makes the row one cell;
//! - **MEDIOLANUM-IT24.A's annex has no title.** The article used to be guessed from the prose
//!   (*obiettivo di investimento sostenibile di questo* ⇒ 9); it is read from the box, which here
//!   is a single image in the band — the empty box is drawn;
//! - **CARNE-EN23 writes its umbrella two ways in one document** — `Sidera Funds SICAV – Christian
//!   Equity` on one page, `Sidera Funds - Digital Worlds` on the other three — while the holdings
//!   always write `SIDERA FUNDS SICAV`. For `R24` removing it is not enough: it has to be removed
//!   **and rewritten**, which is why a removal and a `Product prepend` live on the same row. One of
//!   its four rows is lost anyway, and not through the annex: the holdings title two sub-funds
//!   `… CHRISTIAN EQUITY*` and `… FINANCIAL BOND*`, with the note mark attached that `R16` would
//!   have off;
//! - **MEDIOLANUM-IT24.B** is the same shape for the opposite reason: the holdings write `Fondo
//!   Mediolanum …` and the annex `Mediolanum …`, so the missing word goes back in front or the two
//!   rows are lost in silence;
//! - **FINECO-EN23@IR is `C33`**: its product name wraps, so it needs the column reading, and the
//!   hand-written pipe it had fixed the column's right edge at `x1=372.94` — measured on a short
//!   name. On p975 `FINECO AM EUROPEAN CORPORATE BOND FAM FUND` reaches `x=391` and fell outside,
//!   so the block came out carrying the label alone and the article was lost without a word. The
//!   engine's column reading has no fixed edge: it closes the column on the **legal entity
//!   identifier**, which on this annex sits *below* the label and therefore closes the block
//!   instead of narrowing it;
//! - **KAIROS-EN23 has two layouts in one document** (`R27`), hence two pipelines: the second
//!   writes something between `name` and the colon, so its prefix is a pattern and not the annex's
//!   wording.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::core::pipeline::{Pipeline, PipelineName};
use crate::formats_utils::deserialize::standard_funcs::DeserializeSfdrArticleStandard;

use super::super::semistructured::native::sfdr_annex::{
    InputSfdrAnnexPdfExtract, InputSfdrAnnexTextFilter, PrefixSpec, pdf_extract, text_filter,
};
use super::StructuredError;
use super::tables::{SfdrConfig, get_sfdr_configs};

/// The `pdf_extract` configuration this row describes, in the form the shared builder takes.
///
/// The two forms build the **same** pipes through the **same** function: a second implementation of
/// the annex would be a second place for the day a fifth language arrives.
fn pdf_extract_input(config: &SfdrConfig) -> InputSfdrAnnexPdfExtract {
    InputSfdrAnnexPdfExtract {
        language: config.args.language.clone().unwrap_or_default(),
        question: None,
        affirmative: None,
        negative: None,
        made: None,
        promoted: None,
        title: None,
        product: None,
        identifier: None,
        band: None,
        product_joined: config.args.product_joined.unwrap_or(false),
        product_prepend: config.args.product_prepend.clone().unwrap_or_default(),
        product_across: None,
    }
}

/// The `text_filter` configuration this row describes.
///
/// An empty prefix table means **absent** and not an empty list: absent leaves the language's own
/// prefix in place, and an empty list would strip nothing at all.
fn text_filter_input(config: &SfdrConfig) -> InputSfdrAnnexTextFilter {
    let product_prefix = match config.prefixes.is_empty() {
        true => None,
        false => Some(
            config
                .prefixes
                .iter()
                .map(|pattern| PrefixSpec::Pattern {
                    pattern: pattern.clone(),
                })
                .collect(),
        ),
    };
    InputSfdrAnnexTextFilter {
        language: config.args.language.clone().unwrap_or_default(),
        product_prefix,
        demand_investment_funds_match: true,
        demand_declared_fund: false,
    }
}

/// The SFDR pipelines the repository defines for `format_name`.
///
/// Three segments per row: the two pipes that read the tick and the product name, the filter that
/// reunites them, and the parameterless builder of the article — each left out where the row's
/// partial-pipes row turns it off.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, StructuredError> {
    let configs = get_sfdr_configs(formats_repo_dir)?;
    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();

    for config in configs.into_iter().filter(|c| c.id.format == format_name) {
        let id = config.id.to_string();
        let annex = |source| StructuredError::SfdrAnnex {
            id: id.clone(),
            source,
        };
        let name = PipelineName::new(&config.id.pipeline);
        let pipeline = pipelines
            .entry(name.clone())
            .or_insert_with(|| Pipeline::new(name));
        if config.segments.pdf_extract {
            for pipe in pdf_extract(&pdf_extract_input(&config)).map_err(annex)? {
                pipeline.pdf_extract.push(pipe);
            }
        }
        if config.segments.text_filter {
            for pipe in text_filter(&text_filter_input(&config)).map_err(annex)? {
                pipeline.text_filter.push(pipe);
            }
        }
        if config.segments.deserialize {
            pipeline
                .deserialize
                .push(Arc::new(DeserializeSfdrArticleStandard));
        }
    }

    tracing::debug!(pipeline_count = pipelines.len(), "built sfdr pipelines");
    Ok(pipelines)
}

#[cfg(test)]
mod tests {
    use super::super::tables::SfdrArgsRow;
    use super::*;
    use crate::formats_repo::id_format::ComputedId;

    fn config(
        language: &str,
        joined: Option<bool>,
        prepend: Option<&str>,
        prefixes: &[&str],
    ) -> SfdrConfig {
        SfdrConfig {
            id: ComputedId {
                format: "A-FORMAT".to_string(),
                pipeline: "sfdr_classification".to_string(),
                index: 0,
            },
            args: SfdrArgsRow {
                id: "A-FORMAT".to_string(),
                language: Some(language.to_string()),
                product_joined: joined,
                product_prepend: prepend.map(str::to_string),
            },
            prefixes: prefixes.iter().map(|t| t.to_string()).collect(),
            segments: super::super::tables::Segments::ALL,
        }
    }

    mod what_the_row_becomes {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The language is one cell and reaches **both** segments, which is the defect the
        /// promotion removes: the semistructured form writes it twice.
        #[test]
        fn the_one_language_cell_reaches_both_segments() {
            let config = config("ENGLISH", None, None, &[]);
            assert_eq!(pdf_extract_input(&config).language, "ENGLISH");
            assert_eq!(text_filter_input(&config).language, "ENGLISH");
        }

        #[test]
        fn an_absent_product_joined_is_false_and_an_absent_prepend_is_empty() {
            let input = pdf_extract_input(&config("ITALIAN", None, None, &[]));
            assert!(!input.product_joined);
            assert_eq!(input.product_prepend, "");
        }

        #[test]
        fn the_two_optional_cells_pass_through() {
            let input = pdf_extract_input(&config(
                "ENGLISH",
                Some(true),
                Some("SIDERA FUNDS SICAV"),
                &[],
            ));
            assert!(input.product_joined);
            assert_eq!(input.product_prepend, "SIDERA FUNDS SICAV");
        }

        /// None of the nine anchor overrides is a cell: see the module documentation.
        #[test]
        fn no_anchor_is_overridden_from_a_row() {
            let input = pdf_extract_input(&config("ENGLISH", None, None, &[]));
            for anchor in [
                &input.question,
                &input.affirmative,
                &input.negative,
                &input.made,
                &input.promoted,
                &input.title,
                &input.product,
                &input.identifier,
                &input.band,
            ] {
                assert!(anchor.is_none());
            }
            assert!(input.product_across.is_none());
        }
    }

    mod the_prefixes {
        use super::*;
        use pretty_assertions::assert_eq;

        /// No prefix row means **absent**, which leaves the annex's own wording in place. An empty
        /// list would strip nothing at all, and every product name would keep its label.
        #[test]
        fn no_prefix_row_is_absent_and_not_an_empty_list() {
            assert!(
                text_filter_input(&config("ENGLISH", None, None, &[]))
                    .product_prefix
                    .is_none()
            );
        }

        #[test]
        fn each_row_keeps_its_place() {
            let input = text_filter_input(&config(
                "ITALIAN",
                None,
                None,
                &["Nome del prodotto: ", " \\(.*\\)"],
            ));
            let specs = input.product_prefix.expect("two rows are a list");
            assert_eq!(specs.len(), 2);
            assert!(
                matches!(&specs[0], PrefixSpec::Pattern { pattern } if pattern == "Nome del prodotto: ")
            );
            assert!(matches!(&specs[1], PrefixSpec::Pattern { pattern } if pattern == " \\(.*\\)"));
        }
    }

    mod the_two_demands_on_the_funds {
        use super::*;

        /// The structured row demands a fund the holdings named — the default of the
        /// semistructured form — and never the weaker `demand_declared_fund`, which exists for an
        /// annex that is a document of its own (`R30`). A format in that shape writes YAML.
        #[test]
        fn a_row_demands_the_holdings_match_and_nothing_else() {
            let input = text_filter_input(&config("ENGLISH", None, None, &[]));
            assert!(input.demand_investment_funds_match);
            assert!(!input.demand_declared_fund);
        }
    }

    mod partial_segments {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::fs;
        use tempfile::TempDir;

        /// The sizes of the three segments of the annex pipeline the tables build for `A-EN24`.
        fn sizes(args: &str, partial: &str) -> (usize, usize, usize) {
            let dir = TempDir::new().expect("temp dir");
            let base = dir
                .path()
                .join(super::super::super::tables::STRUCTURED_DIR)
                .join("sfdr");
            fs::create_dir_all(&base).expect("sfdr dir");
            fs::write(
                base.join("args.csv"),
                format!("ID,Language,Product joined,Product prepend\n{args}"),
            )
            .expect("args");
            fs::write(
                base.join("partial_pipes.csv"),
                format!("ID,pdf_extract,text_filter,deserialize\n{partial}"),
            )
            .expect("partial");
            let pipeline = get_pipelines(dir.path(), "A-EN24")
                .unwrap()
                .remove(&PipelineName::new(
                    super::super::super::tables::SFDR_PIPELINE,
                ))
                .expect("the annex pipeline");
            (
                pipeline.pdf_extract.len(),
                pipeline.text_filter.len(),
                pipeline.deserialize.len(),
            )
        }

        #[test]
        fn with_no_row_every_segment_is_built() {
            let (pdf_extract, text_filter, deserialize) = sizes("A-EN24,ENGLISH,,\n", "");
            assert!(pdf_extract > 0 && text_filter > 0);
            assert_eq!(deserialize, 1);
        }

        #[test]
        fn a_disabled_pdf_extract_leaves_that_segment_empty() {
            let (pdf_extract, text_filter, deserialize) =
                sizes("A-EN24,ENGLISH,,\n", "A-EN24,FALSE,,\n");
            assert_eq!(pdf_extract, 0);
            assert!(text_filter > 0);
            assert_eq!(deserialize, 1);
        }

        #[test]
        fn a_disabled_text_filter_leaves_that_segment_empty() {
            let (pdf_extract, text_filter, _) = sizes("A-EN24,ENGLISH,,\n", "A-EN24,,FALSE,\n");
            assert!(pdf_extract > 0);
            assert_eq!(text_filter, 0);
        }

        /// The deserializer alone needs no language at all.
        #[test]
        fn the_deserializer_alone_is_a_row_with_no_language() {
            assert_eq!(sizes("A-EN24,,,\n", "A-EN24,FALSE,FALSE,\n"), (0, 0, 1));
        }

        #[test]
        fn a_disabled_deserialize_leaves_that_segment_empty() {
            let (_, _, deserialize) = sizes("A-EN24,ENGLISH,,\n", "A-EN24,,,FALSE\n");
            assert_eq!(deserialize, 0);
        }
    }
}
