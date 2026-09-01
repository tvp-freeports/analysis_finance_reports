# The problem

## What is being extracted

A fund's annual report is a public document with a predictable set of facts inside it:

- the **investments** the fund holds — the issuer, the instrument, the quantity, the market value,
  the share of net assets, sometimes the acquisition cost and the currency it was bought in;
- the fund's **assets** — total assets, liabilities, net assets at the close of the year;
- the **management company** and the **investment managers** attached to each fund;
- **ESG indicators** and the fund's **SFDR classification**;
- and the fund's own identity over time, including **renamings and mergers**, without which the
  same fund looks like two different ones in two consecutive years.

Those facts are why the report exists. Someone who wants to know whether a pension fund holds a
particular arms manufacturer, or how a family of funds classifies itself under sustainability
regulation, is asking a question the report already answers — on page 412, in a table with no
header repeated from page 411, in a font chosen by a designer.

## Why this is harder than scraping

The reports are not databases with an inconvenient wrapper. Three things make them resist the
obvious approach.

**A PDF has no structure, only appearance.** It records glyphs at coordinates. A "table" is a
belief the reader forms about glyphs that happen to line up. Two reports can look identical to a
person and share nothing at the level a program sees. So the first job is not parsing but
*re-deriving the structure a human eye reconstructs for free*: which lines belong to one row, which
column a number is in, which heading governs which block.

**Every issuer is different, and the same issuer changes.** There is no schema to conform to, so
each layout needs its own recipe. Worse, layouts change between reporting years, which is why
formats in this project carry the year in their name (`EURIZON-EN23`, `AMUNDI-IT24`) — a format is a
recipe for *one layout*, not for one company forever.

**Context is not local.** The row that says a fund holds 12,000 shares of something does not say
*which* fund; that was printed on a cover page eleven pages earlier, and the currency was declared
in a header. A page-at-a-time reader that insists on producing complete records will produce none.

## The shape of the answer: three commitments
Three commitments follow from those three difficulties, and between them they explain most of the
engine's design. They are developed properly in {doc}`design/index`; here is what they are for.

**Formats are plugins, not features.** The engine has no knowledge of any issuer. Support for a
report lives in a **formats repository** — an ordinary directory of CSV, YAML and, where a layout is
genuinely irregular, Python — maintained separately from the engine and often by different people.
Adding a report does not touch the engine, does not require a release of it, and cannot break the
formats someone else maintains. This is the single decision that lets coverage grow faster than the
core team.

**A page is the unit of work, and it is assumed self-contained.** The engine reads one page,
decides what kind of page it is, and applies the recipe for that kind. This assumption is
restrictive and it is deliberate: it is what makes the work divisible, parallelisable, and
individually testable — a format's behaviour on page 412 is a fixture you can check, not a
consequence of the 411 pages before it.

**Where the assumption fails, it fails gracefully.** A value a page genuinely cannot know becomes a
**promise**: a named placeholder, resolved once the whole document has been read. The fund name that
appeared only on the cover page is filled in at the end, from whichever page did see it. So
cross-page dependency is expressed as data rather than as an ordering constraint on the code, and
the page stays the unit of work.

**Three separable questions, three replaceable stages.** Reading a page is split into *what is on
it* (`pdf_extract`), *does any of it concern the funds we are looking for* (`text_filter`), and
*what do the survivors mean* (`deserialize`). A format author usually needs to write only one of
the three and can inherit the rest.

## What the project will not claim

Extraction from documents made for human eyes is not exact, and a tool that presents its output as
simply correct is misrepresenting itself. This project's answer is neither a disclaimer that pushes
all responsibility onto the user nor a promise of accuracy nobody can keep: it is a **published
methodology** and a record, per file, of who vouched for it and under which method. That is the
subject of {doc}`validation`, and reading it is part of using the output responsibly.
