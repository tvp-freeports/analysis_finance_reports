# Where each logic lives

A format's reading is split across three segments, and the split is not a matter of taste. Putting
a piece of reasoning in the wrong one is the easiest mistake to make in this whole area, and the
one that costs most later, because the misplacement is invisible until a second document arrives.

| Segment | What belongs to it | What it is handed |
|---|---|---|
| `pdf_extract` | **layout and geometry**: which lines, which column, which band, which cell continues which | the page's lines, and nothing else |
| `text_filter` | **all the interpretation of the text**: what a word means, which entity a name is, what to keep | the blocks the first segment made |
| `deserialize` | **only the casts**: a string to a number, a string to a date | the blocks the second segment kept |

## The rule, in one sentence each

**`pdf_extract` may not know what anything means.** It does not know which companies are of
interest, which sub-funds the job has seen, or what a word says. It knows where things are.

**`text_filter` may not look at the page.** It never sees a line, a coordinate or a font: it sees
blocks. Everything that is *where* has already been decided.

**`deserialize` may not decide anything.** `1.234` to `1234`, `15/03/2029` to a date. If a
conversion needs to know which convention the page uses, the convention is a **parameter** of the
cast and not a branch in it.

## Why it is drawn there

Because the two halves vary independently. A report that changes its typesetting between two years
changes its geometry and not its vocabulary; a report translated into another language changes its
vocabulary and not its geometry. Code that mixes them has to be rewritten for either.

The clearest evidence is a datum written in several ways by one issuer. When a report states its
sub-fund renamings as prose in one document and as a dated list in another, the **text_filter**
changes and the `pdf_extract` does not: both documents hand over the same section of the same page,
and what differs is what the sentences say. Had the reading of the sentence been written into the
extraction, there would now be two extractions of one section.

## Three decisions, and where each one went

These are measured cases, and each of them could plausibly have gone into a different segment.

**A name that wrapped.** A table cell too long for its column arrives as two lines with their own
coordinates. Putting them back together is *layout*: it happens in `pdf_extract`, and the pieces
are joined with **nothing** between them, because a generator breaking at a word boundary leaves
the space at the end of the first piece and breaking inside a word leaves none — the page already
says how they meet.

**A table row whose first column is empty.** Note 16 of one report lists two delegates for one
sub-fund, the second on a row of its own with the fund column blank — a cell spanning two rows.
Repeating the fund on that row is reading the **layout**, so it is `pdf_extract`. Without it the
second delegate vanishes silently, which is the failure this whole division is meant to make
visible.

**A liability printed in brackets.** `(21,301)` is not a negative number: it is a typographic
convention for a deduction, and the output schema wants the magnitude because it checks that
liabilities plus net assets make the total. Reading it is therefore neither geometry nor meaning
but a **convention of the cast** — `Amount cast: int + magnitude` — and the brackets come off
before the conversion, which is the only way the conversion can happen at all.

## The one that is always tempting, and always wrong

Where a datum is hard to isolate, it is tempting to extract generously and let the `text_filter`
throw away the excess — it is going to filter against the funds of interest anyway, so the noise
will not reach the output.

It will not reach the output *of this run*. It will reach the output of a run with a different
target list, which is the run the next person makes. The filter is a safety net, not an anchor:
find an honest anchor first, and let the filter stand behind it.
