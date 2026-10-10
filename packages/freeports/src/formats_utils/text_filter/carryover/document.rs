//! What the page does not write and another page of the same document has already said.
//!
//! `U7` is not only the promise. The plan defines it more broadly — *read `data.previous()` and
//! carry the value into the block's metadata* — and the promise is the case where the value is an
//! amount. This is the case where the value is **the sub-fund's identity**.
//!
//! EURIZON-IT24 forces it: its statement of net assets does not repeat which fund it is, because the
//! report carries one only and the name is on the holdings page. Attributing it is legitimate —
//! `R7` wants statements per sub-fund, and there is one sub-fund — but it stays an **imposed**
//! value, and `R3` wants whoever reads the `.log.csv` to know.

use crate::core::pipeline::{Extracted, FilterData};
use crate::core::tracing_setup::warn_imposed;
use crate::output::classes::fund::Fund;

/// The name of the sub-fund the document speaks of, if it declares **one only**.
///
/// `None` when there is nothing to attribute with certainty — no fund seen, or more than one — and
/// it says so as an error: a statement page nobody can be billed to is a row lost, not a row that
/// was not there.
pub fn sole_fund(data: &FilterData<'_>) -> Option<String> {
    let names: std::collections::BTreeSet<String> =
        data.previous().iter().filter_map(Extracted::as_fund).filter_map(Fund::name).collect();
    let mut found = names.into_iter();
    let Some(first) = found.next() else {
        tracing::error!(
            "this statement does not name its fund and the job has not seen one yet: \
             there is nobody to attribute it to"
        );
        return None;
    };
    let others = found.count();
    if others > 0 {
        tracing::error!(
            "this statement does not name its fund and the report declares {}: \
             attributing it to any of them would be a guess",
            others + 1
        );
        return None;
    }
    warn_imposed("fund", &first, "the statement does not name it and the report declares one fund only");
    Some(first)
}
