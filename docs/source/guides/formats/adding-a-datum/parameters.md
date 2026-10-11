# The parameters, and the page that needed each one

A cell of a configuration table is a short name for a long story. This page tells the stories: one
case at a time, with the crop of the page that produced it, and the cell that answers it.

It is organised by **case** and not by column, because a case is a property of documents and
outlives any particular spelling. For the full list of columns a table has, with their defaults,
see {doc}`../advanced/structured` and {doc}`../advanced/semistructured`; for what a selection may
say, {doc}`../selections`.

Every crop below is from one report — *BlackRock Fixed Income Dublin Funds plc*, 1 261 pages — and
every number beside it was measured on that document.

---

## The holdings

### A name that wraps, and values level with its last line

```{image} ../assets/holdings-rows.png
:alt: Five lines of one instrument name in the third column, with the currency, the nominal and the two figures all level with the last of them.
:width: 100%
```

The instrument's name is one cell, and the generator broke it into five lines with five sets of
coordinates. Naming the column that may break **is** asking for the pieces to be put back
together — there is no second cell saying "yes" — and the direction says which row of the run the
rebuilt cell lands on.

| What the page shows | The cell |
|---|---|
| the name occupies several lines | `Collapse`, the index of the column that breaks |
| the figures sit level with the name's **last** line | `Collapse direction: up`. Level with the **first** would be `down`, the default |
| two tables side by side, so the column of names is **two** columns | `Collapse: 2 \| 7` — the separator is `\|`, because a comma is part of a real font name |

The pieces are joined with **nothing** between them, and that is not a choice: a generator breaking
at a word boundary leaves the space at the end of the first piece, and breaking inside a word
leaves none.

```{warning}
A lone cell in the column of names is not always a wrapped name. It can be a heading —
`Germany (continued)` in the crop above — and geometrically the two are identical: one column
filled, the others empty.

Here they are **not** identical, because the heading is bold and the names are not, so a body
selection on the face excludes it. A format whose headings share the body's face wants no collapse
at all, or a deselection that keeps them out of the table.
```

### Where the sub-fund's name is, and where the currency is

```{image} ../assets/holdings-header.png
:alt: A page head reading BLACKROCK EURO CREDIT ENHANCED INDEX FUND, a date, and the column headings Currency Holdings, Investment, Fair value EUR '000, and % of net asset value.
:width: 95%
```

Three selections, and the third one is the one people get wrong.

| Cell | What it takes here |
|---|---|
| `Subfund set` | the running head, by face and a band of ordinates: `arial-boldmt[8.5](64:81)` |
| `Body set` | `arialmt[8.5]` — the face alone, because nothing else on the page is set in it |
| `Currency set` | the line reading `EUR '000`, which is the heading of the column of **figures** |

The temptation is to take the currency from the `Currency` column, which every row fills in. That
column holds the currency the **instrument** is denominated in; on a sub-fund reporting in dollars
and holding euro bonds the two are different, and the one the figures are expressed in is the one
the output means. `Currency set` is read once per page and the engine finds the code inside
whatever text it is given, so the column heading is both honest and sufficient.

### A running head that says `(continued)`

The same crop, two pages later, reads `… INDEX FUND (continued)`, and on 1 038 pages of 1 061 it
does. Left in, the document has **46** sub-funds instead of 23, and nothing says so: a sub-fund
name is an identity, and two spellings are two sub-funds.

`Remove from fund` takes a **regular expression** — `\(continued\)` — and is applied before the
name becomes that identity. It is a different thing from `Note marks`, which takes a *set of
characters* off the end of the name, for the reports that hang a footnote on a sub-fund's title.

### Figures the generator spelled oddly

Thirty-one amounts in this document carry a **zero-width non-joiner** after the digits. No cell is
needed: the engine strips it, reads the number, and writes a line of `out/.log.csv` saying it did —

```text
forced "311<U+200C>" to "311" to read it as a number
```

— which is the right shape for this kind of thing. The datum is not lost, and the oddity is not
silent.

---

## The statement of net assets

### Why an ordinate band is the wrong anchor

These two crops are the **same six-page statement**, two pages apart.

```{image} ../assets/balance-sheet-two-lines.png
:alt: Column headings where each sub-fund title takes two lines, with the year row and the currency row immediately below.
:width: 100%
```

```{image} ../assets/balance-sheet-four-lines.png
:alt: The same headings on another page, where the titles take four lines and the year and currency rows have been pushed much further down.
:width: 100%
```

A sub-fund's title takes one, two, three or four lines according to its length, and **everything
below it moves**: the row of currencies sits at 109.7 points on the first page, 119.9 two pages
later and 143.3 on the last. A `Fund set` written as a band of ordinates read off the first page
gives **three** sub-funds of twenty-three, and gives them without an error — the band simply
catches the row of years instead, and `2025` is not a currency.

The anchor that holds is written against the **lines**, not the page:

```text
Fund set      arial-boldmt[8.5]((200:)(75:<arial-boldmt[8.5] "'000">)) / "^2025" / "^2024"
Currency set  arial-boldmt[8.5] "'000"
```

*The bold, to the right of the column of row labels, from the top margin down to the row that says
`'000`, minus the two rows of years.* An anchored side takes the anchor's **inner** edge, and the
two subtractions remove what sits in between. The same configuration then reads **23 of 23**, and
the accounting equation closes on every one.

### A window that caught the wrong column

```{image} ../assets/balance-sheet-net-assets.png
:alt: The row labelled Net assets attributable to redeemable shareholders, with the note number 14 between the label and the first figure.
:width: 100%
```

Each amount is read in a window hung off its **own label**: the label's box, displaced by
`(dx, dy)` of its own size and scaled by `(width, height)`. One window serves all three amounts on
five statements out of eight.

Here it does not. The net-assets row — and only that row — carries the note number `14` between the
label and the figures, and the common window takes it: the first sub-fund comes out with net assets
of **14**. The correction is one cell, the per-column override, which starts the window past the
column of notes:

```text
Total assets window       1.2, 0.0, 100.0, 1.02
Total liabilities window  1.2, 0.0, 100.0, 1.02
Net assets window         1.5, 0.0, 100.0, 1.02
```

That is what the per-column cells are for: a statement whose columns differ in one respect and
agree in the others.

### Columns that are not sub-funds, and columns that are last year

The same statement prints a **combined** column for the whole company, and a previous-financial-year
column beside each current one. Two cells, and they say different things:

| Cell | What it removes |
|---|---|
| `Drop fund set` | a **column that is not a sub-fund at all**, named by one of its own title lines — here `BlackRock Fixed Income Dublin` — taken away whole, title and figures |
| `Drop set` | the **heading of a column of numbers** to leave out — here `2024` |

### A sign that is typography

The liabilities are printed `(21,301)`. The brackets are a convention for a deduction, not a
negative number, and the schema wants the magnitude because it checks that liabilities plus net
assets make the total. That is neither geometry nor meaning but a property of the cast:

```text
Amount cast   int + dash_as_zero + magnitude
```

A base — `int` or `float` — and then the conventions the page uses, added with `+`: `dash_as_zero`
for a page that writes a dash where the figure is nil, `magnitude` for the sign above. They compose
because real statements combine them.

---

## The management company, and the investment managers

### A label that wraps, and the value beside its last line

```{image} ../assets/inv-managers-label.png
:alt: A two-column list of related parties: labels on the left, company names on the right, one label wrapping onto a second line that reads Agent.
:width: 100%
```

The value is to the **right** of the label, so the placement is `right` rather than `on_line` —
`on_line` is for the reports that write the name *inside* the label's own line, where `After` and
`Before` then crop it out.

The subtlety is which line of the label to anchor on. The band the engine opens for a value to the
right starts at the label's **top** edge and grows downwards, because a value sits level with its
label or below it and never above. Here the rows are tight enough that the *previous* row's value —
`BlackRock Asset Management Ireland Limited`, which is the Manager and not the Investment
Manager — still overlaps that band, comes first in reading order, and wins.

The label's **last** line is the one level with its value, so that is the anchor: `^Agent:$`,
measured as one line on **one** page of 1 261. A page that lost the wrap would carry no `Agent:`
and the pipe would fail the page, which is the safe direction to fail in.

### A table of delegations, with a cell spanning two rows

```{image} ../assets/delegation-table.png
:alt: A table of sub-fund names beside their sub-investment managers, where one sub-fund has two managers on two rows and the second row's first column is empty.
:width: 100%
```

The `table` reader of the investment managers' filter wants one row per sub-fund, with the sub-fund
in one column and the manager in another:

```yaml
read:
  - kind: table
    fund_col: 0
    manager_col: 1
```

Two of these six sub-funds have **two** delegates, and the second sits on a row of its own with the
fund column empty — a cell spanning two rows. Repeating the sub-fund there is reading the layout,
so it belongs to `pdf_extract`; without it the second delegate does not appear at all, for either
sub-fund, and nothing says so.

### Who gets what is left

A report states the **exceptions** and leaves the rest implied: unless said otherwise, the manager
is the management company. Working out that residual is the filter's real job, and four cells
decide it.

| Cell | On this page |
|---|---|
| `manco` | `blocks` — the entity that takes the residual is named on this same page, by another pipe of this pipeline |
| `emit_manco` | `never` — that entity is the *Investment Manager*; the management company is declared elsewhere and is not it |
| `already_assigned` | `true` — this page closes the account |
| `funds_include_assigned` | `false` — the table names sub-funds that no holding of interest touches, and letting them in writes a `funds.csv` row with no provenance |

The management company's own row, read on a different page, needed two cells of eleven: the label,
and `Where: below`. Its `managed_funds` comes out **empty**, which is correct — a report that
declares a distinct Investment Manager leaves the management company only a management company.

---

## The SFDR annex

### The article is in the box, not in the title

```{image} ../assets/sfdr-boxes.png
:alt: The question "Did this financial product have a sustainable investment objective?", with an empty box beside Yes and a crossed box beside No.
:width: 95%
```

The annex of the European periodic template asks the question once and offers two boxes. The ticked
one is the answer: affirmative means Article 9, negative Article 8.

Reading the **title** instead — matching `…referred to in Article 8…` as a substring — works only
where the title is there and says so, and across the reference collection and a 905-document corpus
the title is **missing on 236 disclosures**. So the box is what is read, and the pipe decides only
*where the tick is* and *which label it belongs to*; naming an article is an interpretation of what
the question means, and belongs to the next segment.

For a format whose annex is laid out the European way, the whole configuration is **one cell**:

```text
ID,Language,Product joined,Product prepend
DANSKEINVEST-EN24(sfdr_classification),ENGLISH,,
```

The words of the annex belong to the annex, and the engine knows them in four languages along with
five ways of drawing the tick.

### What the marks are, and why they have to be named

In the crop the empty box and the crossed one are **glyphs of a symbol font**, and the two green
bullets beside them are glyphs of the same font that mean nothing at all. A recogniser that looked
for "a character outside ordinary text" would be wrong on the second-largest block of disclosures
in the corpus, where the **empty** box is the letter `U` and the **ticked** one is the letter `C`:
two ordinary letters, indistinguishable by codepoint, with the empty one printed first.

So the table of marks names what counts as ticked, what counts as blank, and — per font — what the
only tick is. It carries the measured defaults, and a format may add to all three parts.

### When a format has to override one of the annex's own words

The `structured` level deliberately does **not** expose the nine anchors of the form. The annex's
words are the annex's, not the format's, and a level meant to be *the default guess* is not the
place for the cell a format needs when it departs from the standard.

A format that must override one has measured its page, and it writes a YAML block that can say why.
One real case: of four disclosures in a document, the Article 9 one closes the product-name column
with `Sustainable Investment Objective`, where the measured wording is `Sustainable investment
objective` — the same phrase with one capital letter. A text anchor is a **verbatim** substring, so
the column did not close, and the product name ran to the foot of the page and took the module, the
percentages and the printed page number with it.

```yaml
BLACKROCK-EN25(sfdr_classification):
  language: ENGLISH
  product_joined: true
  band:
    - Environmental and/or social characteristics
    - Sustainable investment objective
    - Sustainable Investment Objective
```

**An override replaces the list rather than adding to it**, so all three wordings are written: three
of the four disclosures close on the second one.

A name read that way does not produce an error. It produces a sub-fund that does not exist, and the
article disappears.

---

## The ESG indicators

```{image} ../assets/esg-table.png
:alt: A table with the columns Sustainability Indicator, Metric, 2025, 2024 and 2023, one row per indicator, the indicator name wrapping over several lines.
:width: 100%
```

The reader keeps the blocks whose **series** and **column** are the ones asked for, takes the
block's text as the value, and the block's `indicator` key as the name:

```yaml
read:
  kind: dated_rows
  series: 0
  column: 0
```

`column: 0` is the rule that the **previous year is not extracted**: this table repeats two previous
reference periods, and writing them would mean two indistinguishable rows for the same indicator of
the same sub-fund. The columns are numbered **from the most recent**, so the current one is always
`0` whatever a document chooses to print beside it.

`series: 0` is the sub-fund's own series. Where a table carries the sub-fund's figures and its
benchmark's, the benchmark's row is not a datum of the sub-fund, and the index is what leaves it
out.

### Whose page is this

The question the reader cannot answer by itself, because the answer changes per format: the name may
be on the indicators' own page, on the page before, or in a promise an earlier pipe deposited. Four
spellings, and the page above is the first:

| `fund:` | When |
|---|---|
| `blocks` | the page names its own sub-fund — here the running head does, on all 33 pages of the section |
| `promise` | one document, one sub-fund |
| `page_promise` | the name is on the page *n − offset*, keyed by the **printed** page number both pages carry |
| absent | nobody: the section answers in prose, finds no number, and attributing it never arises |

---

## Two additions this report asked of the engine

Both are small, and both are here because the shape of the addition is as instructive as the
parameter.

**A tick the table had not met.** This document answers with a glyph at `U+F0FD`. Wingdings draws
four marks on four consecutive codepoints — a check, a cross, a boxed check, a boxed cross — and the
measured table held the first and the third. The plain cross and the boxed cross were already ticks,
so a cross in a dingbat font is not a new kind of answer: it is the same answer in a font nobody had
met. One character, one test.

Beside it, a second and smaller thing that mattered more: the answer arrives as **two characters in
one text object**, the symbol font's own space and then the mark. `U+F020` is byte `0x20` in
Wingdings and a space in every sense except Unicode's, so trimming left it in place and the line's
first character was not a mark. All four disclosures went unread, with nothing but the pipe's
warning to say so.

**A sub-fund named on the page itself.** The ESG reader could take its sub-fund from a promise for
the whole document, or from the promise of a neighbouring page keyed by the printed page number.
Neither says *the running head of this page*, which is what a report whose indicators have a page to
themselves provides. The new spelling is `fund: {kind: blocks}`, and it reads the **first non-empty**
block of the named type — the same rule the SFDR filter already applies to the product name, and for
the same reason: a format may read the title with two pipes of which only one applies to a page.
