//! Assigning funds to managers: `A5`, and the **only** place where it is decided.
//!
//! # Why it exists
//!
//! A report almost never says who manages what in full. It states the **exceptions** — the
//! sub-funds entrusted to a delegate — and leaves the rest implied: *unless said otherwise, the
//! manager is the management company* (`R6`). That "rest" is the **residual**, and working it out
//! is set arithmetic.
//!
//! Nothing here reads a page: it receives what the `text_filter` has already read — pairs of *name
//! → funds*, where the funds may be missing — plus the set of funds the job has seen in the
//! holdings, and **assigns**.
//!
//! # The seven rules, which are seven lines
//!
//! They are `R6` corrected by `R15`, corrected in turn by `R21` and `R22`.
//!
//! | what the page declares | what happens | rule |
//! |---|---|---|
//! | no manager | every fund to the management company, **warning** | `R6` + `R3` |
//! | **one** manager with no funds | it takes the whole residual; the `manco` stays only `manco` | `R15` |
//! | the first with no funds, the others with | the first takes the residual, the others their own | `R15` |
//! | **more than one** with no funds | the residual to **every one of them**, **warning** | `R21` |
//! | a manager of **part** of a fund | it enters for that fund **without taking it** from the residual | `R21` |
//! | several companies under one specification | **all** of them enter | `R21` |
//! | `SUB-`, `SUB-SUB-`, `SUB-SUB-SUB-` | **flattened**: no levels, and one name is one entry | `R22` |
//!
//! Rows two and three are **the same rule** once written properly: *if there is exactly one manager
//! without funds, the residual is its own*. When it is also the only one declared, the residual is
//! everything.
//!
//! # What this module does not do
//!
//! - **it does not read the page.** The pairs are handed to it by the `text_filter`, which is where
//!   `R23` puts every interpretation of the text — recognising a parenthesis, a `for` connector, a
//!   temporal qualification `(until …)`. Everything arrives already decided;
//! - **it does not build blocks.** [`super::blocks`] does that;
//! - **it does not decide which funds matter.** The `funds` set is passed in by the caller, usually
//!   from the holdings already extracted — that is, spelled as they spell it, which is what `R24`
//!   asks.

use std::collections::BTreeSet;

use crate::core::classes::PdfBlock;
use crate::core::match_fund::MatchFund;
use crate::core::pipeline::{Extracted, FilterData};
use crate::core::tracing_setup::warn_imposed;

/// A manager **as the page declares it**, before any assignment.
///
/// - `name` — the name, trimmed. When the declaration was read off a block, `block` keeps it, so
///   that the output row knows which page it came from;
/// - `funds` — the sub-funds the page attributes to it, or `None` when the page **does not say**.
///   `None` and the empty set are not the same thing: the first is the manager that takes the
///   residual, the second a manager that manages nothing;
/// - `partial` — true when the specification covers **part** of a sub-fund and not the sub-fund
///   (CARNE-EN23 p4: *«in charge for the USD nominated securities in the subfund …»*). The manager
///   enters for that fund, but the fund **stays** in the residual, because the default manager goes
///   on answering for the rest;
/// - `retired` — true when the page qualifies the manager as **no longer in charge** at the closing
///   date (EURIZON-EN23 p10: `(until 10th of May 2023)`). Only whoever is in charge at closing is
///   extracted, so the declaration **does not become a row**; but the funds it names are still
///   funds of the document, and being no longer claimed they return to the residual, that is to
///   whoever the document says manages them now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub name: String,
    pub funds: Option<BTreeSet<MatchFund>>,
    pub partial: bool,
    pub block: Option<PdfBlock>,
    pub retired: bool,
}

impl Declared {
    /// A declaration from a bare name: no funds said, nothing partial, no page behind it.
    pub fn new(name: impl AsRef<str>) -> Self {
        Declared {
            name: name.as_ref().trim().to_string(),
            funds: None,
            partial: false,
            block: None,
            retired: false,
        }
    }

    /// A declaration read **off a block**, whose content is the name.
    ///
    /// The block travels all the way to the output row, and [`super::blocks::text_blocks`] writes
    /// its content as the name — which is why a reader that rebuilds a name rewrites the block
    /// rather than the string (see [`super::reading::renamed`]).
    pub fn from_block(block: PdfBlock) -> Self {
        let name = block_text(&block);
        Declared {
            name,
            funds: None,
            partial: false,
            block: Some(block),
            retired: false,
        }
    }

    /// The same declaration with the funds the page attributes to it.
    pub fn with_funds<I, S>(mut self, funds: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.funds = Some(fund_set(funds));
        self
    }

    /// The same declaration with an already-built fund set, which may be `None`.
    pub fn with_fund_set(mut self, funds: Option<BTreeSet<MatchFund>>) -> Self {
        self.funds = funds;
        self
    }

    pub fn with_partial(mut self, partial: bool) -> Self {
        self.partial = partial;
        self
    }

    pub fn with_retired(mut self, retired: bool) -> Self {
        self.retired = retired;
        self
    }
}

/// A manager **after** the assignment: the residual has been worked out, the funds are final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    pub name: String,
    pub funds: BTreeSet<MatchFund>,
    pub block: Option<PdfBlock>,
    pub is_manco: bool,
}

/// The outcome of [`assign`]: everything the `text_filter` has to write.
///
/// - `managers` — the managers, in the order the page declares them, with the management company
///   last when it is the one taking the residual;
/// - `funds` — **every** fund of the document, which are the management company's `managed_funds`:
///   `R26` wants it one-to-many over everything, while `inv_managers` is many-to-many;
/// - `new_funds` — the sub-funds a manager claims and the job had not yet seen. They have to be
///   declared as funds, or the rows naming them fall;
/// - `to_manco` — true if the management company ended up among the managers, that is if the
///   residual had nobody else to go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub managers: Vec<Assigned>,
    pub funds: BTreeSet<MatchFund>,
    pub new_funds: BTreeSet<MatchFund>,
    pub to_manco: bool,
}

/// The management company handed to [`assign`]: a name, or the block it was read from.
///
/// The distinction is not cosmetic — a company read off a page keeps the page in its output row,
/// one named by the configuration has no page to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Manco {
    Name(String),
    Block(PdfBlock),
}

impl Manco {
    /// The name written in it.
    pub fn name(&self) -> String {
        match self {
            Manco::Name(name) => name.trim().to_string(),
            Manco::Block(block) => block_text(block),
        }
    }

    /// The block it came from, or `None` when it is only a name.
    pub fn block(&self) -> Option<PdfBlock> {
        match self {
            Manco::Name(_) => None,
            Manco::Block(block) => Some(block.clone()),
        }
    }
}

/// The set of fund identities corresponding to `funds`.
///
/// Identities compare themselves after deep normalisation, so `"Alpha Fund"` and `"ALPHA  fund"`
/// are the same element — and **which of the two spellings is kept is an output value**, because the
/// name that goes into `managed_funds` is the kept element's.
///
/// So the elements go in one by one, through [`collect_funds`], and not through `collect()`:
/// `BTreeSet`'s `FromIterator` **replaces** an equal element already there, while `insert` keeps it.
/// The first spelling in is the one kept, which is what a `set` does on the Python side of the
/// boundary and therefore what the reference runs were frozen with.
pub fn fund_set<I, S>(funds: I) -> BTreeSet<MatchFund>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    collect_funds(funds.into_iter().map(|f| MatchFund::new(f.as_ref())))
}

/// The identities gathered into a set **keeping the first** of two equal spellings. See [`fund_set`].
pub fn collect_funds<I: IntoIterator<Item = MatchFund>>(funds: I) -> BTreeSet<MatchFund> {
    let mut set = BTreeSet::new();
    for fund in funds {
        set.insert(fund);
    }
    set
}

/// The sub-funds the managers **already extracted in this job** have taken.
///
/// A managers' section that starts on one page and ends on the next sees, on the second, the
/// results of the first: the residual has to be worked out over what is left.
pub fn assigned_funds(data: &FilterData<'_>) -> BTreeSet<MatchFund> {
    collect_funds(
        data.previous()
            .iter()
            .filter_map(Extracted::as_investments_manager)
            .flat_map(|manager| manager.data.managed_funds.iter())
            .map(MatchFund::new),
    )
}

/// The text written in a block, without the spaces a PDF line drags along.
pub(super) fn block_text(block: &PdfBlock) -> String {
    block
        .content
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Assigns the funds to the managers, and returns the [`Assignment`] that comes out.
///
/// `declared` are the pairs read off the page, `funds` the funds the job has seen, `manco` the
/// management company — a name or its block — or `None` when the format does not read it.
///
/// `already_assigned` are the sub-funds that **other pages of the same job** have already given to
/// a manager: a section that starts on one page and ends on the next works the residual out over
/// what is left, not over everything. They produce no row here — the page that read them wrote it.
///
/// It warns in two cases, which are the two in which the output value is **not** written in the
/// document: when the residual goes to the management company (`R3`, an imposed value) and when
/// more than one manager is left without funds and the residual goes to all of them (`R21`, an
/// ambiguity resolved by excess).
///
/// `settle = false` says that **the section does not end here**: the managers the page declares are
/// written, but what nobody claims is given to nobody and nothing is reported. It is for sections
/// spanning several pages in which no manager is declared *without* sub-funds — UBS-EN23 p4→5 is
/// the case — because there the first page would decide the residual on the half it has read, and
/// the management company would take sub-funds the next page attributes to a delegate. The last
/// page closes the account, and knows what the others took from `already_assigned`.
pub fn assign(
    declared: Vec<Declared>,
    funds: &BTreeSet<MatchFund>,
    manco: Option<&Manco>,
    already_assigned: &BTreeSet<MatchFund>,
    settle: bool,
) -> Assignment {
    let declarations: Vec<Declared> = flatten(declared)
        .into_iter()
        .map(|d| respell(d, funds))
        .collect();
    let kept: Vec<Declared> = declarations
        .into_iter()
        .filter(|d| is_of_interest(d, funds))
        .collect();

    // Managers who left during the year write no rows, but the sub-funds they name are sub-funds of
    // the document all the same: `named` counts them, `claimed` does not.
    let live: Vec<&Declared> = kept.iter().filter(|d| !d.retired).collect();

    let mut claimed: BTreeSet<MatchFund> = already_assigned.clone();
    for d in &live {
        if let Some(own) = &d.funds
            && !d.partial
        {
            claimed.extend(own.iter().cloned());
        }
    }
    let mut named: BTreeSet<MatchFund> = BTreeSet::new();
    for d in &kept {
        if let Some(own) = &d.funds {
            named.extend(own.iter().cloned());
        }
    }

    let new_funds: BTreeSet<MatchFund> = named.difference(funds).cloned().collect();
    let residual: BTreeSet<MatchFund> = funds.difference(&claimed).cloned().collect();

    // `R3`: the link coming out of a **partial** specification says more than the document says. The
    // row `manager → fund` cannot tell *manages the fund* from *manages the dollar-denominated part
    // of the fund*, and the qualification is in prose, so the only place it stays written is the
    // log. CARNE-EN23 p4 is the measured case.
    for d in &live {
        if d.partial
            && let Some(own) = &d.funds
            && !own.is_empty()
        {
            let names: Vec<&str> = own.iter().map(MatchFund::name).collect();
            tracing::warn!(
                "\"{}\" is credited with {} but the report declares it for a **part** of them: \
                 the fund stays with whoever manages the rest",
                d.name,
                names.join(", ")
            );
        }
    }

    let unspecified = live.iter().filter(|d| d.funds.is_none()).count();
    let mut managers: Vec<Assigned> = live
        .iter()
        .map(|d| Assigned {
            name: d.name.clone(),
            funds: d.funds.clone().unwrap_or_else(|| residual.clone()),
            block: d.block.clone(),
            is_manco: false,
        })
        .collect();
    let mut to_manco = false;

    if settle && unspecified == 0 {
        match (residual.is_empty(), manco) {
            (false, Some(manco)) => {
                let name = manco.name();
                managers.push(Assigned {
                    name: name.clone(),
                    funds: residual.clone(),
                    block: manco.block(),
                    is_manco: true,
                });
                to_manco = true;
                warn_imposed(
                    "investments_manager",
                    &name,
                    "the residual of a report that names no manager for these funds",
                );
            }
            (false, None) => tracing::error!(
                "{} funds have no investments manager and the format reads no management company",
                residual.len()
            ),
            (true, _) => {}
        }
    } else if settle && unspecified > 1 {
        tracing::warn!(
            "the report names {} managers without saying which funds each has: every one of them \
             is credited with all {} residual funds",
            unspecified,
            residual.len()
        );
    }

    Assignment {
        managers,
        funds: {
            let mut all = funds.clone();
            all.extend(new_funds.iter().cloned());
            all
        },
        new_funds,
        to_manco,
    }
}

/// The declarations with the same name merged into one, the page's order kept (`R22`).
///
/// This is what "flattening the sub-managers" means once we have got this far: the hierarchy has
/// already been removed by the `text_filter`, and what is left to do is not to count the same
/// company twice when it appears under two labels. `None` is absorbing — a name declared once
/// without funds stays the residual's manager even if elsewhere it carries a list.
fn flatten(declared: Vec<Declared>) -> Vec<Declared> {
    let mut merged: Vec<Declared> = Vec::with_capacity(declared.len());
    for d in declared {
        let Some(position) = merged.iter().position(|kept| kept.name == d.name) else {
            merged.push(d);
            continue;
        };
        let before = &mut merged[position];
        // A name declared **without** the temporal qualification too is in charge: the document
        // names it twice and one of the two times does not qualify it.
        before.retired = before.retired && d.retired;
        before.partial = before.partial && d.partial;
        before.funds = match (before.funds.take(), d.funds) {
            (Some(mine), Some(theirs)) => Some(mine.union(&theirs).cloned().collect()),
            _ => None,
        };
    }
    merged
}

/// The declaration's funds respelled **as the holdings spell them** (`R24`).
///
/// Identities compare normalised, so the fund is already the same element however it is written; but
/// the name that ends up in the output is the kept element's, and it has to be the earlier phases'
/// and not the managers' page's. A sub-fund the job does not know stays spelled as the page spells
/// it: that is all there is.
fn respell(mut declaration: Declared, funds: &BTreeSet<MatchFund>) -> Declared {
    if let Some(own) = declaration.funds.take() {
        declaration.funds = Some(collect_funds(
            own.into_iter().map(|f| funds.get(&f).cloned().unwrap_or(f)),
        ));
    }
    declaration
}

/// Whether the declaration concerns this document, or is noise to let fall.
///
/// A manager naming no fund always concerns the document — the residual is its own. One naming them
/// all outside the job's set is talking about something else (another umbrella, a neighbouring
/// table): it falls, and says so to whoever is writing the format, because a row missing from the
/// output is nearly always the `text_filter` (`R12`) and this is the point where it disappears.
fn is_of_interest(declaration: &Declared, funds: &BTreeSet<MatchFund>) -> bool {
    let Some(own) = &declaration.funds else {
        return true;
    };
    if own.iter().any(|f| funds.contains(f)) {
        return true;
    }
    tracing::debug!(
        "'{}' names only funds this job has not seen: dropped",
        declaration.name
    );
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::BlockType;
    use crate::output::classes::assets_manager::AssetsManagerData;
    use crate::output::classes::fund::Fund;

    /// The three funds every branch is measured on, spelled as the holdings spell them.
    fn three() -> BTreeSet<MatchFund> {
        fund_set(["Alpha Fund", "Beta Fund", "Gamma Fund"])
    }

    /// The names of an assignment's managers, in the order they come out.
    fn names(assignment: &Assignment) -> Vec<&str> {
        assignment
            .managers
            .iter()
            .map(|m| m.name.as_str())
            .collect()
    }

    /// The funds credited to `name`, sorted, as written.
    fn credited(assignment: &Assignment, name: &str) -> Vec<String> {
        assignment
            .managers
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.funds.iter().map(|f| f.name().to_string()).collect())
            .unwrap_or_default()
    }

    fn manager_result(name: &str, funds: &[&str]) -> Extracted {
        Extracted::InvestmentsManager(crate::output::classes::assets_manager::InvestmentsManager {
            data: AssetsManagerData {
                name: name.to_string(),
                managed_funds: funds.iter().map(|f| f.to_string()).collect(),
            },
        })
    }

    mod the_seven_branches {
        use super::*;

        #[test]
        fn no_manager_at_all_sends_every_fund_to_the_management_company() {
            let manco = Manco::Name("Acme SGR".to_string());
            let got = assign(Vec::new(), &three(), Some(&manco), &BTreeSet::new(), true);
            assert_eq!(names(&got), ["Acme SGR"]);
            assert_eq!(credited(&got, "Acme SGR").len(), 3);
            assert!(got.to_manco);
        }

        #[test]
        fn one_manager_without_funds_takes_them_all() {
            let manco = Manco::Name("Acme SGR".to_string());
            let got = assign(
                vec![Declared::new("Delegate AM")],
                &three(),
                Some(&manco),
                &BTreeSet::new(),
                true,
            );
            // `R15`: the residual is the delegate's, and the management company stays only that.
            assert_eq!(names(&got), ["Delegate AM"]);
            assert_eq!(credited(&got, "Delegate AM").len(), 3);
            assert!(!got.to_manco);
        }

        #[test]
        fn the_one_without_funds_takes_what_the_others_left() {
            let declared = vec![
                Declared::new("Residual AM"),
                Declared::new("Specific AM").with_funds(["Beta Fund"]),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(credited(&got, "Specific AM"), ["Beta Fund"]);
            assert_eq!(credited(&got, "Residual AM"), ["Alpha Fund", "Gamma Fund"]);
        }

        #[test]
        fn more_than_one_without_funds_gives_the_residual_to_every_one_of_them() {
            // `R21`: an ambiguity resolved by excess, and the warning is in the next module.
            let declared = vec![Declared::new("One AM"), Declared::new("Two AM")];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(credited(&got, "One AM").len(), 3);
            assert_eq!(credited(&got, "Two AM").len(), 3);
        }

        #[test]
        fn a_manager_of_part_of_a_fund_does_not_take_the_fund_from_the_default() {
            let declared = vec![
                Declared::new("Partial AM")
                    .with_funds(["Beta Fund"])
                    .with_partial(true),
                Declared::new("Residual AM"),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(credited(&got, "Partial AM"), ["Beta Fund"]);
            // Beta is still the residual's: the default manager answers for the rest of it.
            assert_eq!(credited(&got, "Residual AM").len(), 3);
        }

        #[test]
        fn several_companies_under_one_specification_all_enter() {
            let declared = vec![
                Declared::new("One AM").with_funds(["Beta Fund"]),
                Declared::new("Two AM").with_funds(["Beta Fund"]),
                Declared::new("Three AM").with_funds(["Beta Fund"]),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(names(&got), ["One AM", "Two AM", "Three AM"]);
        }

        #[test]
        fn the_same_company_under_two_labels_is_one_manager() {
            // `R22`: `SUB-`, `SUB-SUB-` and the rest are flattened, and one name is one entry.
            let declared = vec![
                Declared::new("One AM").with_funds(["Alpha Fund"]),
                Declared::new("One AM").with_funds(["Beta Fund"]),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(names(&got), ["One AM"]);
            assert_eq!(credited(&got, "One AM"), ["Alpha Fund", "Beta Fund"]);
        }

        #[test]
        fn no_funds_declared_once_wins_over_a_list_declared_elsewhere() {
            let declared = vec![
                Declared::new("One AM").with_funds(["Alpha Fund"]),
                Declared::new("One AM"),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(credited(&got, "One AM").len(), 3);
        }
    }

    mod retired_managers {
        use super::*;

        #[test]
        fn a_retired_manager_writes_no_row() {
            let declared = vec![
                Declared::new("Gone AM")
                    .with_funds(["Alpha Fund"])
                    .with_retired(true),
                Declared::new("Current AM"),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(names(&got), ["Current AM"]);
        }

        #[test]
        fn the_funds_it_named_go_back_into_the_residual() {
            let declared = vec![
                Declared::new("Gone AM")
                    .with_funds(["Alpha Fund"])
                    .with_retired(true),
                Declared::new("Current AM"),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(credited(&got, "Current AM").len(), 3);
        }

        #[test]
        fn a_fund_only_a_retired_manager_names_is_still_a_fund_of_the_document() {
            let declared = vec![
                Declared::new("Gone AM")
                    .with_funds(["Alpha Fund", "Delta Fund"])
                    .with_retired(true),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(
                got.new_funds.iter().map(|f| f.name()).collect::<Vec<_>>(),
                ["Delta Fund"]
            );
        }

        #[test]
        fn a_name_declared_once_without_the_qualification_is_in_charge() {
            let declared = vec![
                Declared::new("One AM")
                    .with_funds(["Alpha Fund"])
                    .with_retired(true),
                Declared::new("One AM").with_funds(["Beta Fund"]),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(names(&got), ["One AM"]);
        }
    }

    mod the_fund_names {
        use super::*;

        #[test]
        fn a_declared_fund_is_respelled_as_the_investments_wrote_it() {
            // `R24`: identities match normalised, but the name that goes out is the holdings'.
            let declared = vec![Declared::new("One AM").with_funds(["ALPHA  FUND"])];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(credited(&got, "One AM"), ["Alpha Fund"]);
        }

        #[test]
        fn a_fund_the_job_had_not_seen_comes_out_to_be_declared() {
            let declared = vec![Declared::new("One AM").with_funds(["Alpha Fund", "Delta Fund"])];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(
                got.new_funds.iter().map(|f| f.name()).collect::<Vec<_>>(),
                ["Delta Fund"]
            );
            assert_eq!(got.funds.len(), 4);
        }

        #[test]
        fn a_manager_of_funds_this_job_never_saw_is_dropped() {
            let declared = vec![Declared::new("Elsewhere AM").with_funds(["Omega Fund"])];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert!(names(&got).is_empty(), "{:?}", names(&got));
        }

        #[test]
        fn a_manager_of_one_known_fund_among_unknown_ones_stays() {
            let declared = vec![Declared::new("One AM").with_funds(["Omega Fund", "Beta Fund"])];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(names(&got), ["One AM"]);
        }
    }

    mod what_the_management_company_gets {
        use super::*;

        #[test]
        fn the_assignment_covers_every_fund_even_when_the_company_manages_none() {
            let manco = Manco::Name("Acme SGR".to_string());
            let declared = vec![Declared::new("Delegate AM")];
            let got = assign(declared, &three(), Some(&manco), &BTreeSet::new(), true);
            assert_eq!(got.funds.len(), 3);
        }

        #[test]
        fn it_enters_as_a_manager_only_for_what_is_left_over() {
            let manco = Manco::Name("Acme SGR".to_string());
            let declared = vec![Declared::new("Delegate AM").with_funds(["Beta Fund"])];
            let got = assign(declared, &three(), Some(&manco), &BTreeSet::new(), true);
            assert_eq!(credited(&got, "Acme SGR"), ["Alpha Fund", "Gamma Fund"]);
        }

        #[test]
        fn nothing_left_over_means_no_manager_row_for_it() {
            let manco = Manco::Name("Acme SGR".to_string());
            let declared = vec![Declared::new("Delegate AM").with_funds([
                "Alpha Fund",
                "Beta Fund",
                "Gamma Fund",
            ])];
            let got = assign(declared, &three(), Some(&manco), &BTreeSet::new(), true);
            assert_eq!(names(&got), ["Delegate AM"]);
            assert!(!got.to_manco);
        }

        #[test]
        fn a_company_read_off_a_page_keeps_its_block() {
            let block = PdfBlock::bare(BlockType::MANAGEMENT_COMPANY, "Acme SGR");
            let manco = Manco::Block(block.clone());
            let got = assign(Vec::new(), &three(), Some(&manco), &BTreeSet::new(), true);
            assert_eq!(got.managers[0].block, Some(block));
        }

        #[test]
        fn a_company_named_by_the_format_has_no_block_to_keep() {
            let manco = Manco::Name("  Acme SGR  ".to_string());
            let got = assign(Vec::new(), &three(), Some(&manco), &BTreeSet::new(), true);
            assert_eq!(got.managers[0].name, "Acme SGR");
            assert_eq!(got.managers[0].block, None);
        }
    }

    mod the_shape_of_a_declaration {
        use super::*;

        #[test]
        fn no_funds_and_no_fund_are_not_the_same_declaration() {
            let residual = Declared::new("One AM");
            let nothing: Declared = Declared::new("Two AM").with_funds(Vec::<String>::new());
            assert_eq!(residual.funds, None);
            assert_eq!(nothing.funds, Some(BTreeSet::new()));
        }

        #[test]
        fn a_declaration_keeps_the_block_it_was_read_from() {
            let block = PdfBlock::bare(BlockType::INVESTMENTS_MANAGER, "  One AM  ");
            let declared = Declared::from_block(block.clone());
            assert_eq!(declared.name, "One AM");
            assert_eq!(declared.block, Some(block));
        }

        #[test]
        fn the_declaration_order_of_the_page_is_the_order_of_the_output() {
            let declared = vec![
                Declared::new("Third AM").with_funds(["Gamma Fund"]),
                Declared::new("First AM").with_funds(["Alpha Fund"]),
                Declared::new("Second AM").with_funds(["Beta Fund"]),
            ];
            let got = assign(declared, &three(), None, &BTreeSet::new(), true);
            assert_eq!(names(&got), ["Third AM", "First AM", "Second AM"]);
        }
    }

    mod what_other_pages_have_already_taken {
        use super::*;

        #[test]
        fn funds_another_page_already_assigned_are_out_of_the_residual() {
            let taken = fund_set(["Alpha Fund"]);
            let got = assign(vec![Declared::new("One AM")], &three(), None, &taken, true);
            assert_eq!(credited(&got, "One AM"), ["Beta Fund", "Gamma Fund"]);
        }

        #[test]
        fn they_produce_no_row_of_their_own() {
            let taken = fund_set(["Alpha Fund"]);
            let got = assign(vec![Declared::new("One AM")], &three(), None, &taken, true);
            assert_eq!(names(&got), ["One AM"]);
        }

        #[test]
        fn the_already_assigned_funds_are_read_from_the_filter_data() {
            let previous = vec![
                manager_result("Earlier AM", &["Alpha Fund", "Beta Fund"]),
                Extracted::Fund(Fund::new("Gamma Fund")),
            ];
            let got = assigned_funds(&FilterData::Previous(&previous));
            assert_eq!(got, fund_set(["Alpha Fund", "Beta Fund"]));
        }
    }

    mod when_the_section_does_not_end_here {
        use super::*;

        #[test]
        fn nobody_takes_what_nobody_claimed() {
            let manco = Manco::Name("Acme SGR".to_string());
            let declared = vec![Declared::new("Delegate AM").with_funds(["Beta Fund"])];
            let got = assign(declared, &three(), Some(&manco), &BTreeSet::new(), false);
            assert_eq!(names(&got), ["Delegate AM"]);
            assert!(!got.to_manco);
        }

        #[test]
        fn a_manager_without_funds_still_takes_the_residual_as_it_stands() {
            let declared = vec![Declared::new("Residual AM")];
            let got = assign(declared, &three(), None, &BTreeSet::new(), false);
            assert_eq!(credited(&got, "Residual AM").len(), 3);
        }

        #[test]
        fn the_page_that_does_close_it_sees_what_the_others_took() {
            let manco = Manco::Name("Acme SGR".to_string());
            let taken = fund_set(["Beta Fund"]);
            let got = assign(Vec::new(), &three(), Some(&manco), &taken, true);
            assert_eq!(credited(&got, "Acme SGR"), ["Alpha Fund", "Gamma Fund"]);
        }
    }

    mod fund_identities {
        use super::*;

        #[test]
        fn a_fund_set_compares_after_deep_normalisation() {
            assert_eq!(fund_set(["Alpha Fund"]), fund_set(["ALPHA  fund"]));
        }

        #[test]
        fn the_spelling_of_the_first_one_in_is_the_one_kept() {
            let set = fund_set(["Alpha Fund", "ALPHA  fund"]);
            assert_eq!(set.len(), 1);
            assert_eq!(set.iter().next().expect("one element").name(), "Alpha Fund");
        }
    }
}
