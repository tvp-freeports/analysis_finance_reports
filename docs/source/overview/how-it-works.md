# How a run works

One diagram and one paragraph. Every claim on this page is argued for properly in
{doc}`../reference/design/index`, which is thirteen chapters and says of each thing whether it
exists, is planned, or is an accepted limit.

```{figure} ../reference/design/assets/algorithm-overview.svg
:alt: Documents are classified per document, the union is scheduled into steps, each step runs page classes through pipeline bundles, and the results resolve against the promise map before the tables are written
:width: 100%

**One run, end to end.** Each document is opened and classified **on its own**, so that a
format's page-class finalizer sees one document's pages. The classified pages of every document are
then poured into **one** schedule, which processes them in steps; a step's results become the next
step's filter. Every page of a class goes through that class's **bundle of pipelines**, each three
segments long, and what comes out is either an entity or a promise. Entities resolve against the
accumulated promise map at the end, and the whole run writes **one** set of tables.
```

## The whole project in one paragraph
A document becomes a sequence of pages, and **the page is the unit of work**: it is assumed to carry
the context needed to understand what is on it. Pages are classified — per document — then poured
into one **schedule** of steps, where each step's results filter the next. Every page of a class goes
through a **bundle of pipelines**, each three segments long: what is on this page, does it concern
us, what does it mean. What comes out is an entity, or a **promise** for a value the page could not
know. At the end, promises resolve, entities accumulate, and the whole run writes **one** set of
tables. Everything else — parallelism, testability, localised failures — follows from that first
assumption.

## The consequence people trip over
One invocation of `freeports` is one or more **jobs**, and the results of every job are accumulated
together and written **once**, at the end, as one set of tables. So running two reports in one
invocation is not the same as running them twice and concatenating: in one invocation they share the
promise resolution and the deduplication of funds; in two they do not.

## Where each part is written down
| The part | Argued in | Operated in |
|---|---|---|
| the page as the unit of work | {doc}`../reference/design/pages` | — |
| promises, for what a page cannot know | {doc}`../reference/design/promises` | — |
| classification and the schedule | {doc}`../reference/design/classification`, {doc}`../reference/design/schedule` | — |
| the three segments of a pipeline | {doc}`../reference/design/segments` | {doc}`../guides/formats/index` |
| formats living outside the engine | {doc}`../reference/design/formats-as-plugins` | {doc}`../guides/formats/repository` |
| what comes out, and in what order | {doc}`../reference/design/entities-and-output`, {doc}`../reference/design/determinism` | {doc}`../guides/user/output` |
| parallelism, and what it costs | {doc}`../reference/design/parallelism` | {doc}`../guides/user/advanced/parallelism` |
| what is accepted as a limit | {doc}`../reference/design/limits` | — |
