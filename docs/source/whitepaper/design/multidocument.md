# Several documents in one run

*Implemented.* Giving `-i` more than once, or naming a directory, is not a loop over single-document
runs. It is one run with more pages in it.

## The rule

> **Classification is per document. The schedule is over the union.**

Each document is opened and classified on its own, so a format's finalizer sees one document's page
sequence ({doc}`classification`). The classified pages of every document are then poured into **one**
pool, and the schedule processes them together, caring only about the class each page was given.

For everything downstream, having several documents changes **nothing** except that each page
carries the information of which document it came from.

## Why this is the right shape

The two halves of the rule are each forced, for opposite reasons.

**Classification must be per document** because a continuation rule is a fact about *adjacent pages
of one file*. Over a union, page 1 of document B follows page 400 of document A, and "this page
continues the previous table" would join tables that have nothing to do with each other.

**The schedule must be over the union** because that is what makes several documents *one*
extraction rather than several concatenated ones. Promise resolution and fund deduplication happen
once, across everything — so a fund named on the cover of document A can supply a value promised by
a table in document B, and the same fund appearing in both is one row, not two.

This is the property that makes the two commands genuinely different:

```console
$ freeports -i a.pdf -i b.pdf …      # one run: shared promises, deduplicated funds
$ freeports -i a.pdf … && freeports -i b.pdf …   # two runs, then concatenate: neither
```

## Identifying a document

Every output row carries a **`Report`** column naming the document it came from, and the name comes
from the document specifier ({doc}`../usage/documents`):

| Given | Name |
|---|---|
| `report.pdf` | the absolute path |
| `report.pdf:EURIZON 2023` | `EURIZON 2023` |
| `https://…/r.pdf` | the URL |
| a directory named `2024` | `2024/<file name>` per document in it |

## What this replaced

The `Report` column exists in **every** run, batch or not, and that uniformity is the point of the
design. Before it, batch mode had a `prefix out` column whose job was to give a document a name,
and non-batch runs had no notion of naming at all — so the output schema depended on which mode
produced it.

Making the name a property of the *document specifier* rather than of *batch mode* removed the
column, removed the special case, and made one schema serve both. A consumer of `investments.csv`
no longer has to know how the run was invoked.

## What it is not

Several documents in one invocation is not the same as {doc}`batch mode <../usage/batch>`. Several
`-i` is **one job** over several documents, all read with one format against one set of lists;
batch is **one job per row**, each with its own format and lists. Batch rows can themselves name
several documents, so the two compose.
