# Several documents in one run

*Implemented.* Giving `-i` more than once, or naming a directory, is not a loop over single-document
runs. It is **one run with more pages in it** — the documents are a *bundle*, and the engine treats
the bundle the way it would treat one large document that happened to be bound in several volumes.

## Why a report arrives as several files

The obvious reason to pass several documents is that you have many reports of the same layout and
want them all extracted. That case works, but it is the uninteresting one: it would work just as well
as several separate runs.

The reason the feature exists is the other one. **A single financial report is often published as
several files**, because its sections are addressed to different audiences and are printed, dated and
distributed separately — the holdings schedule for one readership, the management and governance
sections for another, an annex for a regulator. Nothing about that split follows the structure of the
information; it follows the structure of the publisher's obligations.

The consequence is that **no one file contains what you need**. A concrete shape of it, and a common
one:

| Wanted | Where it is |
|---|---|
| the investments | document A |
| the investment managers of those funds | document B |
| the asset managers | back in document A |

Extracting the investments *with their managers attached* is then not possible from either file
alone. Run separately, document A yields holdings whose manager column is empty and a manager table
with nothing to attach it to; document B yields managers belonging to funds it never mentions.
Joining the two afterwards means reimplementing, outside the engine and by hand, the matching the
engine already does — on names that were normalised inside it and are no longer normalised in the
CSV.

Passed together, the two files are one report:

```console
$ freeports -i "fund-report.pdf:2024" -i "governance-annex.pdf:2024" -f EURIZON-EN23 …
```

## What "as if it were one document" means precisely

It means the **schedule** — the sequence of steps, where each step's results are the next step's
input filter ({doc}`schedule`) — runs once, over the pages of the whole bundle:

1. every document is classified on its own, so each page gets a class;
2. the classified pages of all documents go into **one** pool;
3. step 1 runs over every page of that pool, whichever file it came from, and its results become the
   filter data of step 2;
4. step 2 runs over every page of the pool, again regardless of file.

So in the example above, the step that reads the investment managers out of document B produces
results that the later step over document A's holdings pages **receives as its input filter**. That
is the whole mechanism, and it is why the bundle yields information neither file yields alone. The
same applies to promises ({doc}`promises`): a value promised by a table in one document can be
supplied by a page in another, because resolution happens once, over everything.

Fund deduplication is also once, across the bundle — a fund named in both documents is one row, not
two.

## The rule: classify per document, schedule over the union
> **Classification is per document. The schedule is over the union.**

Each document is opened and classified on its own, so a format's finalizer sees one document's page
sequence ({doc}`classification`). The classified pages of every document are then poured into **one**
pool, and the schedule processes them together, caring only about the class each page was given.

For everything downstream, having several documents changes **nothing** except that each page
carries the information of which document it came from.

## Why that is the right shape
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

## What the `Report` column replaced
The `Report` column exists in **every** run, batch or not, and that uniformity is the point of the
design. Before it, batch mode had a `prefix out` column whose job was to give a document a name,
and non-batch runs had no notion of naming at all — so the output schema depended on which mode
produced it.

Making the name a property of the *document specifier* rather than of *batch mode* removed the
column, removed the special case, and made one schema serve both. A consumer of `investments.csv`
no longer has to know how the run was invoked.

## How this differs from batch mode
Several documents in one invocation is not the same as {doc}`batch mode <../usage/batch>`, and the
difference is exactly the one this page is about. Several `-i` is **one job** over several documents:
one schedule, one pool of pages, shared filter data, shared promises, shared deduplication. Batch is
**one job per row**, each with its own format and lists, each with its own schedule that sees nothing
of the others — which is precisely what you do *not* want for a report split across files.

A rule of thumb: if the files are pieces of one report, they belong to one job, several `-i`. If they
are separate reports that merely happen to be extracted at the same time, they belong to separate
rows of a batch. Batch rows can themselves name several documents, so a batch of bundles composes
naturally.
