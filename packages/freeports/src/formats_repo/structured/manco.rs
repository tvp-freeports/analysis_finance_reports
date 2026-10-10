//! The structured management-company pipelines: *the company that runs the fund*, in one row of CSV.
//!
//! Fourteen of the collection's twenty-six formats read the management company the same way — a
//! **label** declares it, the value is next to it, and a crop takes the name out of the line — and
//! that is the majority of a data type read by one algorithm, which is the condition for belonging
//! to this level. Twelve of the fourteen are rows of this table; the two that are not have a
//! `text_filter` of a different kind, for which see the end of this page.
//!
//! # The row is a default: one cell, and one or two more
//!
//! Only `Label set` is required. Everything else is what a page adds to the common shape, and the
//! shape is common enough that eight of the twelve are a label and a crop:
//!
//! | cell | what it is for | how many of the twelve |
//! |---|---|---|
//! | `Label set` | the label, in the compact grammar | twelve |
//! | `After` · `Before` | the crop: the name is inside the line, between two words (`R23`) | five · four |
//! | `Also investments manager` | the same company runs the investments of every sub-fund (`R15`) | seven |
//! | `Where` · `Take` · `Reach` · `Stop set` | the value is below the label, or there are several | two · one · one · one |
//! | `Among` | `U6`: the page names two companies with their periods, and the one at closing wins | one |
//! | `Imposed name` | `R3`: the report does not say, so the format declares — **with** a warning | one |
//!
//! # Why the label is a compact string here and a mapping in YAML
//!
//! A CSV cell cannot hold a mapping, so the cells of this table are written in the compact grammar —
//! which, since the mapping became the form to write in YAML, is a deliberate step back for this
//! one level. It is affordable because a label is short: ten of the twelve are a font, a size and
//! an anchored text, and the two that are not — CARNE-EN23's area-bounded window and
//! EURIZON-EN23's band between two headings — are dense cells and are the price of having the data
//! type in one place. A format that prefers the mapping writes a YAML block instead: the two levels
//! sum, and nothing here forbids it.
//!
//! # The two that are not rows, and why
//!
//! UBS-EN23 and FINECO-EN23@IR reach the management company through the **investment managers'**
//! filter (`inv_managers`), not through the crop: on those pages the company is named inside a
//! sentence that also delegates to somebody else, and that reading is a different algorithm with
//! ten knobs of its own. They stay semistructured, and that is the right place for them.
//!
//! # What the thirteen pages are like
//!
//! A CSV cell carries no prose, so the measurements that chose these rows are written here — as
//! `fund_assets.rs` and `sfdr.rs` do for theirs. They are also the argument that the row is
//! enough: every one of these is a **page**, not a parameter.
//!
//! | format | what the page does, and what the row answers with |
//! |---|---|
//! | AMUNDI-EN24 | `C01`: the investment manager here does **not** fall back on the management company — pages 8-9 say so, and it is `IM-2` — so the row leaves `Also investments manager` empty |
//! | AMUNDI-IT24 | `C02`: the report **does not declare** the company. `Amundi SGR S.p.A.` appears three times and never in its role — the registered office, the 100% ownership, the remuneration policy — so reading the output datum there would rest it on a coincidence of words. The name is therefore **imposed** by the format, with its warning (`R3`), and the label only says **on which page** to declare it: `^SOMMARIO`, the summary, which every document of this format has exactly one of |
//! | ANIMA-EN23 · FIDEURAM-IT24 | the label is above the value, so what the filter is handed is a **window built around the value** and not the label — the case `window(..)` exists for. Nothing is cropped afterwards, and the text block carries the PDF block it came from (`P9`). ANIMA-EN23's label is `Manager, Promoter and Distributor`: the comma inside the anchor is safe, because a window's four numbers are read **from the right** |
//! | ANIMA_SGR-IT24 | the name is inside the line — `… Societa di Gestione del Risparmio S.p.A. …` — in the running head of **every** page, and the face is the union of the three variants the format uses (`R8`) |
//! | ANIMA_SICAV-EN24 | `C03`/`C12`: the name is the whole line below the label, so the reading is a `strip` and not a crop — neither `After` nor `Before` |
//! | CARNE-EN23 | the window below `Management Company` would take the right-hand column too; what closes it is the **abscissa** of the heading beside it, `Legal adviser`. An anchored side takes its line's **inner** edge, so the bound falls between the two columns and not over the second |
//! | EURIZON-EN21 | `MC-4`: 2021 is the year Pramerica became Eurizon, and the report names **both** companies with their period in brackets. The one in charge at closing is kept, which is `Among: most_recent` (`U6`); every line of the section is handed over, because choosing is text interpretation (`R23`). The bold **closes** the section, because below the last address comes `Cabinet de Revision Agree:` and then the auditors, who carry a period of their own and would choose for us. `Reach` is not a limit here: it is the permission to run to the end of a section eighty-six points tall |
//! | EURIZON-EN23 | the name is the `frutiger-black` line inside the **section** `MANAGEMENT COMPANY AND PROMOTER`, which closes on the next heading: both sides of the area are lines of the page |
//! | EURIZON-IT24 | the name comes after the word `gestione` and ends at the comma |
//! | FIDEURAM-EN23 | `MC-5`: the report has no `Management Company` section and no label — the name is inside a **sentence** of note 1 on p67, *«it is managed by Fideuram Asset Management (Ireland) dac (the "Management Company")»*. The anchor is the sentence and not the note's heading: `A2` would bring down twenty lines of prose and *which* line is the right one would be a question again, while the sentence occurs **once** in all 81 pages. `Before: \(the` really is needed: the name itself contains a bracket, `(Ireland)`, and stopping at the first one would extract `Fideuram Asset Management` |
//! | KAIROS-EN23 | the band below the label also carries the all-space lines the generator leaves between entries, and `Take: first` would pick one of those. They are subtracted |
//! | MEDIOLANUM-IT24.B | `C09`/`C14`: the name sits between the formula and the comma, and the investment managers' residue falls on it because no delegate is declared |

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::core::classes::BlockType;
use crate::core::pipeline::{Pipeline, PipelineName};
use crate::formats_utils::deserialize::standard_funcs::{
    DeserializerInvestmentsManagerFromManco, DeserializerManagmentCompanyStandard,
};
use crate::formats_utils::pdf_extract::labelled_value::{
    LabelledValueArgs, LabelledValueTake, LabelledValueWhere, LabelledValueWindow,
    PdfExtractLabelledValueStandard,
};
use crate::formats_utils::pdf_extract::section::SectionEnd;
use crate::formats_utils::text_filter::managment_company::{
    AmongLines, TextFilterManagmentCompanyImposed, TextFilterManagmentCompanyInLine,
};
use crate::input::document::selection::pdfline_selection_from_str;

use super::StructuredError;
use super::tables::{MancoArgsRow, get_manco_configs};

/// The block type and the pipe's name, which no row writes because no row differs in them.
const BLOCK: BlockType = BlockType::MANAGEMENT_COMPANY;
const PIPE_NAME: &str = "managment company";

/// The management-company pipelines the repository defines for `format_name`.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, StructuredError> {
    let configs = get_manco_configs(formats_repo_dir)?;
    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();

    for (id, row) in configs.into_iter().filter(|(id, _)| id.format == format_name) {
        let name = PipelineName::new(&id.pipeline);
        let pipeline = pipelines.entry(name.clone()).or_insert_with(|| Pipeline::new(name));

        pipeline.pdf_extract.push(Arc::new(extraction(&id.to_string(), &row)?));
        pipeline.text_filter.push(filtering(&id.to_string(), &row)?);
        pipeline.deserialize.push(Arc::new(DeserializerManagmentCompanyStandard));
        if row.also_investments_manager.unwrap_or(false) {
            pipeline.deserialize.push(Arc::new(DeserializerInvestmentsManagerFromManco));
        }
    }

    tracing::debug!(pipeline_count = pipelines.len(), "built manco pipelines");
    Ok(pipelines)
}

/// The `labelled_value` pipe the row describes.
fn extraction(id: &str, row: &MancoArgsRow) -> Result<PdfExtractLabelledValueStandard, StructuredError> {
    let selection = |column: &'static str, cell: &str| {
        pdfline_selection_from_str(cell)
            .map_err(|source| StructuredError::LineSelection { id: id.to_string(), column, source })
    };

    let placement = match row.placement.as_deref().map(str::trim) {
        None | Some("") | Some("on_line") => LabelledValueWhere::OnLine,
        Some("below") => LabelledValueWhere::Below,
        Some("right") => LabelledValueWhere::Right,
        Some(other) => {
            return Err(StructuredError::MancoCell {
                id: id.to_string(),
                column: "Where",
                cell: other.to_string(),
                expected: "on_line, below or right",
            });
        }
    };
    let take = match row.take.as_deref().map(str::trim) {
        None | Some("") | Some("first") => LabelledValueTake::First,
        Some("all") => LabelledValueTake::All,
        Some(other) => {
            return Err(StructuredError::MancoCell {
                id: id.to_string(),
                column: "Take",
                cell: other.to_string(),
                expected: "first or all",
            });
        }
    };

    let mut args = LabelledValueArgs::new(selection("Label set", &row.label_set)?, BLOCK, PIPE_NAME);
    args.r#where = placement;
    args.take = take;
    // A window only where the row asked for one, so that the side it did not name keeps the
    // default its placement carries rather than an arbitrary zero — the same rule the YAML form
    // follows. `Across` is not a column: none of the twelve measured pipelines sets it.
    if let Some(reach) = row.reach {
        let default = LabelledValueWindow::default_for(placement);
        args.window = Some(LabelledValueWindow { reach: Some(reach), across: default.across });
    }
    if let Some(cell) = &row.stop_set {
        args.stop = Some(SectionEnd::Selection(selection("Stop set", cell)?));
    }
    Ok(PdfExtractLabelledValueStandard::new(args))
}

/// The `text_filter` pipe the row describes: the crop, or the declared name.
fn filtering(
    id: &str,
    row: &MancoArgsRow,
) -> Result<Arc<dyn crate::core::pipeline::TextFilterPipe>, StructuredError> {
    if let Some(name) = &row.imposed_name {
        return Ok(Arc::new(TextFilterManagmentCompanyImposed::new(name.clone())));
    }
    let among = match row.among.as_deref().map(str::trim) {
        None | Some("") | Some("first") => AmongLines::First,
        Some("most_recent") => AmongLines::MostRecent,
        Some(other) => {
            return Err(StructuredError::MancoCell {
                id: id.to_string(),
                column: "Among",
                cell: other.to_string(),
                expected: "first or most_recent",
            });
        }
    };
    let pipe = TextFilterManagmentCompanyInLine::new(
        row.after.as_deref().unwrap_or(""),
        row.before.as_deref().unwrap_or(""),
        among,
    )
    .map_err(|source| StructuredError::Manco { id: id.to_string(), source })?;
    Ok(Arc::new(pipe))
}
