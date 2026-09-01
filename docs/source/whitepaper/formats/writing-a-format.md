# Writing a format

The path from *"this PDF is not supported"* to *"this PDF is supported and the tests say so"*.

## Before anything: choosing a level
A format specifies, for each page class it cares about, up to three segments
({doc}`../design/segments`). There are three **levels** at which a segment can be specified, and they
differ only in how much of the algorithm you supply yourself:

| Level | The algorithm | Its parameters | You write | Page |
|---|---|---|---|---|
| **structured** | in the library, fixed | columns of a CSV | rows in a spreadsheet | {doc}`levels/structured` |
| **semistructured** | in the library *by name*, or your own | YAML | a name and a configuration | {doc}`levels/semistructured` |
| **unstructured** | in the repository | in the code | Python | {doc}`levels/unstructured` |

They **add up** rather than exclude one another, and mixing them is the normal case — structured
extraction with unstructured filtering, say. Merging happens by summing the **same-named** pipelines
of the three levels.

Start at the most constrained level that works and drop down only where you must. A report whose
investment table is perfectly ordinary may still name its funds in a way nothing can parameterise;
inheriting two segments and writing the third is what keeps a hard format from costing three times a
simple one.

## 1. Declare the format

Add a row to `metadata/formats.csv`, by **components** rather than by name:

```text
Name,Locale,Year,Country,Version
CARNE,EN,23,,
```

The name every other file refers to — `CARNE-EN23` — is synthesised as
`Name-Locale<YY>[@Country][.Version]` and written nowhere else, so a name and its components cannot
disagree. See {doc}`repository`.

If the reports come from a stable address, add a prefix to `metadata/url_mapping.csv` so the format
is inferred without `--format`.

## 2. Put the report and the test companies in place

```text
tests/formats/CARNE-EN23/report.pdf
```

and, once per repository, `freeports-dev setup-input-db`.

## 3. Find out which pages matter

```console
$ freeports-dev inspect-document --format CARNE-EN23
```

Nothing is classified yet, so everything comes back `unclassified` — that is the starting point, not
a failure. Open the PDF, find a page of the table you want, and note what makes it recognisable: a
header, a font, a phrase.

## 4. Classify

Write the classification rows or pipe, then run `inspect-document` again until the right pages carry
the right class. At the structured level this is two or three rows:

```text
ID,Header set,Class
CARNE-EN23/0,"Arial-BoldMT ""Description""",investments
CARNE-EN23/0,"Arial-BoldMT ""Currency""",investments
```

Rows sharing an id describe **one** pipe, and a page belongs to the class if it contains **all** of
them.

**Do not go further until this is right.** Chasing the extraction of a page the engine never
selected is the single most common way to lose an afternoon
({doc}`../design/classification`).

## 5. Schedule the class

A class no step mentions is an **error**, so add it to `content/orchestration/algorithms_schedule.csv`:

```text
Format name,Page type,Filter next iteration
CARNE-EN23,investments,
```

Raise `Filter next iteration` on the last class of a step when what that step finds should filter
the next one ({doc}`../design/schedule`).

## 6. Extract, one segment at a time

```console
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -t investments -m pdf_blks
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -t investments -m txt_blks
$ freeports-dev inspect-page -f CARNE-EN23 -p 25 -t investments -m results
```

In that order. Each mode shows what one segment made of the page, so a wrong result tells you
**which segment** is wrong rather than that the page is wrong. {doc}`dev-loop`.

Most of what you write here is a **selection** — a predicate over the lines of a page.
{doc}`selections`.

## 7. Freeze it

```console
$ freeports-dev make-tests -f CARNE-EN23 -p 25 -t investments
```

Three JSON fixtures for that page, one per segment. Read what it prints before confirming:
`make-tests` records *what the code does now*, so confirming a wrong result promotes a bug into the
specification and the test will then defend it.

Repeat for a handful of representative pages — the first page of a table, a continuation page, a page
with an awkward row.

## 8. Run the whole document

```console
$ freeports-dev test --format CARNE-EN23
```

The per-page tests are fast. The whole-document test compares the output tables against
`tests/formats/CARNE-EN23/out/`, and the first time there is nothing to compare against: generate it
deliberately, **after reading it**, because from then on it is the format's specification and not a
snapshot.

## 9. Vouch for it

The reference output of a format is a claim about the world. The project records who is making it
and on what basis — {doc}`../validation` for the mechanism, {doc}`tooling` for the commands.

## Two rules about matching and unresolved pages
**Company matching is first-match-wins, in file order.** The input database's order is meaningful,
not incidental. A format that seems to attribute a holding to the wrong company is often reporting a
database problem rather than an extraction one. See {doc}`../input-db`.

**A page that cannot fully resolve is not a failed page.** Reach for a promise before reaching for a
second pass. If the fund name is on the cover and the table is on page 412, the table *promises* the
name and the cover supplies it. Adding an ordering constraint between pages instead gives up
everything the page-as-unit assumption buys ({doc}`../design/promises`).
