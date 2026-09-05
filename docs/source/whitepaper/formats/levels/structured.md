# The structured level

**No code at all, only rows.** The most constrained of the three levels, and the one that covers most
formats.

The algorithm is fixed and lives in the library; you supply its parameters as columns of a CSV. The
tables live under `content/algorithms/structured/`, in two groups:

| Directory | For |
|---|---|
| `investments/` | extracting a fund's holdings |
| `page_classify/` | classifying pages |

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
| `Tolerance` | `pdf_extract` | number | `0` |
| `Geometrical indexing` | `text_filter` | `TRUE`/`FALSE` | `TRUE` |
| `Merge previous` | `text_filter` | `TRUE`/`FALSE` | `FALSE` |
| `Interpret dash as zero` | `text_filter` | flag expression | nothing substituted |
| `Interpret quantity as float` | `deserialize` | `TRUE`/`FALSE` | `FALSE` |
| `Interpret cost and value as int` | `deserialize` | `TRUE`/`FALSE` | `TRUE` |

Each column belongs to one segment, and that matters: a pipe that switches a segment **off** in
`partial_pipes.csv` and then fills in one of that segment's columns is rejected when the repository
loads. Declaring a segment off and configuring it is a contradiction, not something to guess at.

### `Algorithm flags`

How the table-coordinates algorithm finds the grid, when its default choices do not suit a layout:
`RETURN_ROWS`, `BIG_CELL_RULE`, `USE_RULER_AREA`, `USE_TEST_POS`.

### `Geometrical indexing` and `Merge previous`

`Geometrical indexing` decides what the numeric columns of `args.csv` mean. `TRUE`, the default,
makes them **linear distances on the grid**, wrapping into the next row when they exceed the table's
width. `FALSE` makes them positions in the flat list of blocks — which shifts every field of any row
whose company name wrapped onto two lines, so it is rarely what you want.

`Merge previous` says which way a cell split across two blocks is put back together: into the
**preceding** block (`TRUE`) or the following one.

### `Interpret dash as zero`

Several reports print `-` where a number belongs — a frozen holding, a position too small to round
to a visible percentage. That dash is not a number, so by default the row is dropped (for the market
value) or the field left empty (for the others), and the `.log.csv` says so.

Where you have checked the report and the dash really does mean zero, this column says so, **one
field at a time**:

```text
ID,…,Merge previous,Interpret dash as zero
AMUNDI-EN24,…,,MARKET_VALUE
DANSKEINVEST-EN24,…,TRUE,MARKET_VALUE | PERC_NET_ASSETS
MEDIOLANUM-EN24,…,,PERC_NET_ASSETS
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

The zero that results is a real value and behaves like one — including the `warn` in the `.log.csv`
saying it sits on the edge of its domain ({doc}`../../usage/logging`). That is the point: the
holding is in the output, where it was being dropped before.

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
unspecified at this level ({doc}`../../design/formats-as-plugins`).

## Two conventions of the structured tables
**An empty cell means *absent*, everywhere.** Not zero, not the default — absent. It is the same rule
the output files follow.

**Every validation error names the row number.** These files are edited in a spreadsheet by people
who are not reading Rust backtraces, and an error that cannot say which row is nearly useless. It is
also why the repository's CSVs are parsed by hand rather than by a dataframe library.
