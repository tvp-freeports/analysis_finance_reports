# The design of the algorithm

Why the engine has the shape it has. Not *how to run it* — that is {doc}`../usage/index` — and not
which libraries it is built from, which is a question for contributors and lives in
{doc}`the implementation notes <../../dev/implementation-notes>`.

Every page here states what it describes and whether that thing **exists**:

| Marker | Means |
|---|---|
| *implemented* | it is in the engine today, and the tests defend it |
| *planned* | designed, argued for, not built. The page says what would change |
| *accepted limit* | a known edge deliberately left as it is, with the reason |

## The whole algorithm at a glance

```{figure} assets/algorithm-overview.svg
:alt: Documents are classified per document, the union is scheduled into steps, each step runs page classes through pipeline bundles, and the results resolve against the promise map before the tables are written
:width: 100%

**One run, end to end.** Each document is opened and classified **on its own**, so that a
format's page-class finalizer sees one document's pages. The classified pages of every document are
then poured into **one** schedule, which processes them in steps; a step's results become the next
step's filter. Every page of a class goes through that class's **bundle of pipelines**, each three
segments long, and what comes out is either an entity or a promise. Entities resolve against the
accumulated promise map at the end, and the whole run writes **one** set of tables.
```

The two halves of that picture are the two halves of this section. Everything up to the schedule is
about *which pages get processed and in what order*; everything after it is about *what a page
becomes*.

## The pages

**The foundations** — the one assumption everything else rests on, and what it costs.

| Page | Says |
|---|---|
| {doc}`pages` | the page is the unit of work: the assumption, what follows from it, where it cedes |
| {doc}`promises` | what to do when a page genuinely cannot know something |

**Deciding what to process** — from a pile of PDFs to an ordered list of work.

| Page | Says |
|---|---|
| {doc}`classification` | what kind of page is this, and why classifying is itself a pipeline |
| {doc}`schedule` | steps, and why a step's output is the next step's filter |
| {doc}`multidocument` | several documents in one run: classified apart, scheduled together |

**Processing a page** — the pipeline and the values that travel through it.

| Page | Says |
|---|---|
| {doc}`segments` | three segments, why three, and the fourth that is designed but not built |
| {doc}`blocks` | `PdfBlock` and `TextBlock`: an open type, a typed value |
| {doc}`entities-and-output` | what the project models, and the schema as an invariant of the product |

**Properties of the whole** — things that are true of the engine rather than of any one part.

| Page | Says |
|---|---|
| {doc}`formats-as-plugins` | why the recipes live outside the engine, and how three levels merge |
| {doc}`determinism` | what order is guaranteed, and what is not |
| {doc}`parallelism` | parallelism as a consequence of the page assumption, not an addition |
| {doc}`limits` | the accepted limits, stated rather than hidden |

```{toctree}
:maxdepth: 1
:hidden:

pages
promises
classification
schedule
multidocument
segments
blocks
entities-and-output
formats-as-plugins
determinism
parallelism
limits
```
