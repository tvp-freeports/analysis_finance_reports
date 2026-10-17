# Recipes, and the tests that freeze them

## What a recipe is

A **format** is *the specification of an operating recipe for extracting information*. A **recipe**
is one pair: an algorithm, and one way a document writes a datum.

That is the unit the tests freeze — not the format, and not the output table. Two formats using the
same recipe with different parameters need **one** test between them; one format using five recipes
needs **five**.

It is also why formats are split only when their algorithms diverge, and never because two
documents look different.

## What a test of a recipe is

Under `tests/formats/<FORMAT>/[n/]pages/<page class>/`, three files per page:

| File | Freezes |
|---|---|
| `<page>-pdf_blks.json` | what `pdf_extract` made of the page |
| `<page>-txt_blks.json` | what `text_filter` kept |
| `<page>-results.json` | what `deserialize` built |

`freeports-dev make-tests` writes all three. **Read what it prints before confirming**: it records
*what the code does now*, so confirming a wrong result promotes a defect into the specification,
and the test will then defend it.

```console
$ freeports-dev make-tests -f BLACKROCK-EN25 -t fund_assets -p 37
```

Three files and not one, because a wrong result then says **which segment** is wrong rather than
that the page is wrong.

## Which pages to freeze

One per recipe, chosen for what makes it different — not the first page of the class, and not a
sample. A real set, from a format whose statement of net assets is a matrix:

| Page | The recipe it freezes |
|---|---|
| 37 | sub-fund titles on one or two lines, a combined column to drop, and a footnote number beside the net assets |
| 42 | titles on **four** lines, footnote marks in another face, and three sub-funds with only the current year's column because they launched during it |

Two pages, two recipes, and between them every shape the six pages of that statement take.

## The `filter_data.json`, and the four conditions

For a page class whose `text_filter` needs the sub-funds an earlier step found — the statement of
net assets, the investment managers, the SFDR annex, the ESG indicators, the renamings — the
fixture needs filter data, and **it is built by hand**.

A `filter_data.json` taken as it comes produces a test that passes while demonstrating nothing: if
the filter recognises no fund, the deserializer yields nothing and the fixture freezes the void.

A useful one satisfies four conditions:

1. **at least two sub-funds that pass the filter**, so the reading is exercised more than once;
2. **at least one sub-fund on the page that is discarded**, so the fixture proves the filter
   filters;
3. **the names spelled the way the earlier phases spell them.** This is the condition that is
   easiest to get wrong and hardest to notice;
4. **for the investment managers, a residual that is not empty** — the residual is the thing the
   algorithm actually computes.

### Condition 3, and why it is the dangerous one

Sub-fund identities are compared after a deep normalisation, so case and punctuation do not matter:
a statement that writes `BlackRock Euro Credit Enhanced Index Fund` and a schedule of investments
that writes `BLACKROCK EURO CREDIT ENHANCED INDEX FUND` are the same sub-fund, and a fixture may
use either.

What normalisation does **not** remove is a footnote mark. `Christian Equity*` and
`Christian Equity` are two different sub-funds, and a mark left on the name is not a blemish in one
column: it is a row that quietly matches nothing and disappears. The same goes for a `(continued)`
the running head carries, for an umbrella prefix one section writes and another does not, and for a
name the layout split in two.

So the fixture's names are the **output** of the earlier phase, not what you read off the page by
eye.

### A filter data that says three things at once

One measured example, for a class of four SFDR annexes. The filter kept there demands that the
annex name a sub-fund the **holdings** named — not merely one declared as a fund — so the fixture
was built to exercise all three outcomes:

| Entry | Outcome |
|---|---|
| two sub-funds with a holding | their annexes are kept, one Article 8 and one Article 9 |
| one sub-fund declared as a `Fund` but with no holding | its annex is **discarded** |
| one sub-fund absent altogether | its annex is **discarded** |

That file is worth more than three green tests: it is where the difference between *declared* and
*held* is written down.

## The classes that need no filter data

The holdings do not. Their filter datum is the **list of companies**, which the pytest plugin
supplies, and a hand-written file there would only pin the target list into the fixture.

An empty `filter_data.json` is not a neutral default — it is a claim. Write one only where the
class needs one.

## Freezing as the scope widens

The staging is: one reference page, then the whole document, then several documents of the same
format — and **at each step, freeze before moving on**.

Each widening is a different kind of failure. A page exercises the reading of the datum, a document
exercises the variation between its pages, and several documents exercise what actually varies
across the format. An algorithm written straight against the last of the three cannot tell you
which of them it got wrong.

The whole-document test compares against `tests/formats/<FORMAT>/[n/]out/`, and the first time
there is nothing to compare against. Generate it deliberately, **after reading it**: from then on
it is the format's specification and not a snapshot.

```{note}
`make-tests` does the per-page fixtures only. A whole-document reference output is written by the
command line, run from inside the formats repository with every input the test uses named
explicitly — the repository, its own input database, the `TEST` list, and the document named
`report`, which is what the test calls it:

    freeports -f <FORMAT> -i tests/formats/<FORMAT>/report.pdf:report -F . -I tests/input_db \
      -T TEST -o tests/formats/<FORMAT>/out

The result is byte for byte what the test computes, `.log.csv` included. Reference outputs are not
rewritten as a side effect of other work — regenerate only when those rows changing **is** the
work, and read the diff before keeping it.
```
