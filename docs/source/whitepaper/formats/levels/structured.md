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

## Two conventions worth knowing

**An empty cell means *absent*, everywhere.** Not zero, not the default — absent. It is the same rule
the output files follow.

**Every validation error names the row number.** These files are edited in a spreadsheet by people
who are not reading Rust backtraces, and an error that cannot say which row is nearly useless. It is
also why the repository's CSVs are parsed by hand rather than by a dataframe library.
