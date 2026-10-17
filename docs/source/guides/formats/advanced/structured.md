# The structured level

**No code at all, only rows.** The most constrained of the three levels, and the one that covers most
formats.

The algorithm is fixed and lives in the library; you supply its parameters as columns of a CSV. The
tables live under `content/algorithms/structured/`, one directory per datum:

| Directory | For | Required cells |
|---|---|---|
| `page_classify/` | classifying pages | all three |
| `investments/` | a fund's holdings | the body selection and the market value's column |
| `fund_assets/` | a statement of net assets | the fund column and the three amounts |
| `manco/` | the management company | the label |
| `sfdr/` | the periodic SFDR annex | the language |

**A directory a repository does not need is a file it does not have.** Four of the five were added
after the first, each when the same reading turned out to be the majority way of reading its datum —
which is the condition for a datum to belong to this level at all.

Every directory a repository does have carries a `partial_pipes.csv` beside its arguments, saying
which segments each pipe builds (see [Partial pipelines](#partial-pipelines)). A required cell is
required while the segment that reads it is built.

## The `ID` column

Every row is keyed by an `ID` saying which pipe of which pipeline of which format it configures. The
full form is:

```text
<format>(<pipeline>)/<index>
```

The pipeline and the index are almost always omitted and derived. The index in particular is derived
from **the other rows of the same `(format, pipeline)` group**, not from the row alone — so rows
sharing an id describe one pipe, and consecutive groups get consecutive pipes.

## Extraction rows

An investments row supplies the selection identifying the table body, the one identifying the fund
name, the one identifying the currency, and the column positions of market value, quantity,
percentage of net assets and acquisition cost:

```text
ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,…
AMUNDI-EN24,ArialMT(:27),ArialNarrow(:208),ArialNarrow(:768),1,-1,2,…
```

The `set` columns are {doc}`../selections` in their compact textual form. The numeric columns are
column indices, and negative values count from the right — `-1` being the last column, which is what
you want on a table whose leading columns vary.

## `additional_args.csv`

A second table, joined onto the first by `ID`, holding the parameters a pipe usually leaves alone.
**At most one row per pipe**, and no row at all is the normal case — every column defaults, and a
pipe that says nothing behaves the way the library does.

| Column | Segment | Cell | Default |
|---|---|---|---|
| `Algorithm flags` | `pdf_extract` | flag expression | no flags |
| `Column tolerance` | `pdf_extract` | number | `0` |
| `Row tolerance` | `pdf_extract` | number | `0` |
| `Geometrical indexing` | `text_filter` | `TRUE`/`FALSE` | `TRUE` |
| `Interpret dash as zero` | `text_filter` | flag expression | nothing substituted |
| `Remove from fund` | `text_filter` | one regular expression | nothing cut |
| `Interpret quantity as float` | `deserialize` | `TRUE`/`FALSE` | `FALSE` |
| `Interpret cost and value as int` | `deserialize` | `TRUE`/`FALSE` | `TRUE` |
| `Collapse` | `pdf_extract` | column indices, separated by `\|` | no column may break |
| `Collapse direction` | `pdf_extract` | `down`/`up` | `down` |
| `Collapse algorithm` | `pdf_extract` | `pattern`/`geometry` | `pattern` |
| `Quantity in name` | `text_filter` | one regular expression | the quantity is a column |

The two tolerances say how far apart two cells may sit and still count as the same column, or the
same row. They are **independent**: leaving one empty means zero for that axis, never the other
one's value. A table whose column of names breaks up because the names differ in length wants a
wide column tolerance and a row tolerance of zero — one number for both would weld consecutive
holdings into a single row while it was busy reuniting the names.

A column the table does not take is refused when the repository loads, rather than ignored — a
file written for an older engine would otherwise carry cells that silently do nothing. So is a
column that only refines another one, filled without it.

Each column belongs to one segment, and that matters: a pipe that switches a segment **off** in
`partial_pipes.csv` and then fills in one of that segment's columns is rejected when the repository
loads. Declaring a segment off and configuring it is a contradiction, not something to guess at.

### `Algorithm flags`

How the table-coordinates algorithm finds the grid, when its default choices do not suit a layout:
`RETURN_ROWS`, `BIG_CELL_RULE`, `USE_RULER_AREA`, `USE_TEST_POS`.

### The three `Collapse` cells

A name too long for its column wraps, and the remainder arrives as a line of its own with its own
coordinates. Naming the column that may break **is** asking for the collapse — with the cell filled
the `pdf_extract` emits one block per **cell** instead of one per line — and the other two cells say
how:

- **`Collapse direction`** — `down` (the default) puts the pieces on the **first** row of the run,
  `up` on the last. Pick the one that leaves the cell beside **its own values**;
- **`Collapse algorithm`** — `pattern` (the default) takes every run of consecutive cells of the
  column; `geometry` only the runs whose rows carry nothing but the name.

Several columns, because a page often carries **two tables side by side** and then the column of
names is two columns. The pieces are joined with nothing between them, which is not a choice: the
page already says how they meet.

{doc}`../adding-a-datum/parameters` shows the case on the page that produced it, together with the
trap — a heading in the column of names is geometrically indistinguishable from a wrapped name, and
a format whose headings share the body's face wants no collapse at all.

### `Geometrical indexing`

`Geometrical indexing` decides what the numeric columns of `args.csv` mean. `TRUE`, the default,
makes them **linear distances on the grid**, wrapping into the next row when they exceed the table's
width. `FALSE` makes them positions in the flat list of blocks — which shifts every field of any row
whose company name wrapped onto two lines, so it is rarely what you want.

### `Interpret dash as zero`

Several reports print `-` where a number belongs — a frozen holding, a position too small to round
to a visible percentage. That dash is not a number, so by default the row is dropped (for the market
value) or the field left empty (for the others), and the `.log.csv` says so.

Where you have checked the report and the dash really does mean zero, this column says so, **one
field at a time**:

```text
ID,…,Interpret dash as zero
AMUNDI-EN24,…,MARKET_VALUE
DANSKEINVEST-EN24,…,MARKET_VALUE | PERC_NET_ASSETS
MEDIOLANUM-EN24,…,PERC_NET_ASSETS
```

| Flag | Field |
|---|---|
| `MARKET_VALUE` | the market value |
| `QUANTITY` | the nominal quantity |
| `PERC_NET_ASSETS` | the percentage of net assets |
| `ACQUISITION_COST` | the acquisition cost |
| `ALL` | the four together |

The expression is the same little language `Algorithm flags` uses — `|`, `&`, `^`, `~` and
parentheses — so `ALL & ~QUANTITY` is the three fields other than the quantity.

**Per field, and not one switch,** because the need really is per field: a report that prints `-`
for a percentage it cannot show may print nothing of the sort in the quantity column, and a single
switch would invent a quantity of zero there.

**Only a dash.** The substitution recognises the dash family and nothing else — `-`, `–`, `—` and
their kin, alone or repeated, with any surrounding space. Not an empty cell, which in these tables
means *absent*; not `n/a`; not a lone `.` or `,`. That line is drawn deliberately: by far the most
common cause of "this cell is not a number" is a **misaligned offset**, where the engine is reading
a currency code, a header or a company name one column away from where it should be. Those must keep
failing loudly, and a rule that turned any unreadable cell into zero would bury them.

There is no flag for the acquisition **currency**: a dash there says "no currency", and zero is not
a currency. `ALL` does not touch it.

The zero that results is a real value and behaves like one. It is on the lower edge of its domain,
which the engine notes at `debug` and not in the `.log.csv` ({doc}`../../user/logging`) — zero is
ordinary, and a file that said so on every written-off holding would say nothing else. That is the
point: the holding is in the output, where it was being dropped before.

### `Remove from fund`

One regular expression whose matches are cut out of the sub-fund's name before it becomes the
sub-fund's identity. One cell and not a list: several cuts are one alternation,
`\(continued\)|\(cont\.\)`, and the space a cut leaves behind goes with it.

The commonest use is the **footnote mark**. Some reports hang a footnote on a sub-fund's name in
the heading of its own pages — `Sidera Funds SICAV – Christian Equity*`, `European Corporate Green
Bond*` — and print the note at the foot of the page. The mark is part of the heading's text, in the
same font and the same size as the name, so no selection can leave it out: it has to come off here,
where the name is read, with a cut **anchored at the end**:

```text
ID,…,Interpret dash as zero,Remove from fund
CARNE-EN23,…,,\s*[*]+\s*$
DANSKEINVEST-EN24,…,MARKET_VALUE | PERC_NET_ASSETS,\s*[*]+\s*$
```

Anchored, because `A*B Fund` keeps its asterisk: that one is not a footnote. A report that uses
several marks lists them in the class, `\s*[*¹†]+\s*$`; one that also writes `(continued)` on its
continuation pages joins the two, `\(continued\)|\s*[*]+\s*$`.

It is not cosmetic. The name this pipe publishes *is* the sub-fund's identity: it goes into
`funds.csv`, into the `fund` field of every holding on the page, and it is what every later phase —
the statement of net assets, the investment managers, the SFDR article — looks the sub-fund up by.
Those comparisons normalise the name, and normalisation does **not** remove an asterisk, so
`Christian Equity*` and `Christian Equity` are two different sub-funds. A mark left on is therefore
not a blemish in one column: it is a row that quietly matches nothing and disappears.

The same column exists for the statement of net assets, where the heading carries the same mark for
the same reason.

## Classification rows

A classification row declares a page class and **one** header to look for. Rows sharing an id
describe one pipe, and the page belongs to that class if it contains **all** of them:

```text
ID,Header set,Class
CARNE-EN23/0,"Arial-BoldMT ""Description""",investments
CARNE-EN23/0,"Arial-BoldMT ""Currency""",investments
```

Two rows, one id, both required. Splitting a conjunction across rows rather than packing it into one
cell is what keeps these files editable in a spreadsheet.

## Partial pipelines

Each of the three segments can be switched off **individually**. That is what a *partial* pipeline
is, and it is what makes the merge with the other levels useful rather than theoretical: a format can
take its `pdf_extract` from here and its `text_filter` from a Python module, by leaving the second
unspecified at this level ({doc}`../../../reference/design/formats-as-plugins`).

Every algorithm has the same table for it, `partial_pipes.csv` in its own directory, with the same
header:

```text
ID,pdf_extract,text_filter,deserialize
BLACKROCK-EN25,TRUE,TRUE,FALSE
```

`FALSE` switches a segment off; `TRUE` or an empty cell leaves it on, and so does having no row at
all. At most one row per pipe, and a row naming a pipe the arguments do not declare is refused. The
file is required wherever the arguments are: a repository with `manco/args.csv` has
`manco/partial_pipes.csv` too, even if it holds only its header.

A segment switched off may not be configured, and a cell a built segment needs may not be empty. So
each column belongs to the segment that reads it:

| Directory | `pdf_extract` | `text_filter` | `deserialize` |
|---|---|---|---|
| `investments/` | the four selections, `Algorithm flags`, the two tolerances, the three `Collapse` cells | the five column positions, `Geometrical indexing`, `Interpret dash as zero`, `Remove from fund` | the two `Interpret … as …` flags |
| `fund_assets/` | the fund column and the three amounts (required), `Currency set`, `Date set`, the three windows, `Drop set`, `Drop fund set`, `Table condition`, `Algorithm flags` | `Date regex`, `Remove from fund` | `Amount cast`, `Date cast` |
| `manco/` | `Label set` (required), `Where`, `Take`, `Reach`, `Stop set` | `After`, `Before`, `Among` | `Also investments manager` |
| `sfdr/` | `Language` (required), `Product joined`, `Product prepend` | `Language` (required), the rows of `product_prefixes.csv` | — |
| `page_classify/` | `Class` (required), `Header set` | — | — |

Two consequences are worth stating.

**A column read by two segments goes only with both.** The SFDR language is read by the extraction
and by the filter, so it stays required while either is built.

**A cell that refines another segment's cell is free when that segment is off.** `Date regex`
refines `Date set`, and `Among` chooses among the lines `Take: all` hands over; with the extraction
built elsewhere, the cell they refine is empty by necessity, and what it would have said is not this
row's to say.

**A classification pipeline is a special case.** Its filter and its deserializer take no parameter
and there is one of each for the whole pipeline, while each classifier has its own extraction. So
`pdf_extract` can be switched off per pipe, but the pipes of one pipeline must **agree** on
`text_filter` and `deserialize`: if they do not, loading fails and names the two pipes. Building the
segment as soon as any pipe asked for it would be a rule written nowhere, covering a contradiction in
the repository.

## Two conventions of the structured tables
**An empty cell means *absent*, everywhere.** Not zero, not the default — absent. It is the same rule
the output files follow.

**Every validation error names the row number.** These files are edited in a spreadsheet by people
who are not reading Rust backtraces, and an error that cannot say which row is nearly useless. It is
also why the repository's CSVs are parsed by hand rather than by a dataframe library.
