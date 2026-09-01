# The three segments

*Implemented — with a fourth that is designed and not built.*

Each page class maps to a **bundle**: the pipelines applied to a page of that class. A pipeline is
three segments, in a fixed order.

```{figure} ../../dev/assets/schema_algorithm.svg
:alt: One page through a pipeline: pdf_extract, text_filter, deserialize, and the promise map
:width: 100%

**One page through one pipeline.** The page is cut into `PdfBlock`s; the blocks that concern the
funds being looked for become `TextBlock`s; each of those is deserialized independently into an
entity or into promise entries. This diagram covers the pipeline only — what decides *which*
pipeline a page gets is {doc}`classification` and {doc}`schedule`.
```

## The three questions the segments answer
**`pdf_extract` — what is on this page.** Takes the page, returns `PdfBlock`s. By convention it
looks only at **graphical** evidence: font, size, position, and text that is part of the layout
rather than of the content. Keeping it away from meaning is what makes it reusable — a table is a
table regardless of what the numbers in it are about.

**`text_filter` — does any of this concern us.** Takes the blocks — *all* of them, because
interpreting one may depend on another — and returns `TextBlock`s, each keeping a reference back to
the PDF block it came from. This is where the target companies enter, and where most of a run's time
is spent: measured, 85–96% of the engine's work, and a single standard pipe inside it accounts for
30–54% of a whole job.

**`deserialize` — what do the survivors mean.** Takes each text block **independently** and produces
a typed entity. No filtering happens here: once meaning is settled the transformation is per block,
so the segment is *defined* to be per block. Measured at under 0.2% of a run.

## Why three segments, and not a graph
**Rejected: a general graph of transformations**, which is more expressive and would let a format do
anything.

The three segments answer three **separable** questions, and separability is the entire benefit: an
author replaces one segment and inherits the other two. That is what keeps a hard format from
costing three times a simple one, and it is what the merge across the three levels operates on
({doc}`formats-as-plugins`).

A general graph gives up exactly that. Nothing then says which node an author may replace in
isolation, and "inherit the rest" stops having a meaning.

## Why there is no fourth segment yet
A fourth was considered and not added, and the machinery is **generic in the number of segments**,
so adding one is cheap if a fourth separable question ever appears.

None had — until one was proposed. It is described below, marked as what it is.

## The planned fourth segment

```{admonition} Planned, not implemented
:class: important

Today `text_filter` does two jobs: it **filters** (does this concern the companies we are looking
for) and it **interprets semantically** (what do these blocks mean as a unit). Those are separable,
and the design to separate them exists:

- one segment for **layout interpretation** and one for **semantic interpretation**, both of which
  an author must specify, because they are what a format actually is;
- **filtering becomes its own segment with a default that filters nothing**, so a format that does
  not need it never writes it;
- **`deserialize` gains a standard implementation too**, dispatching on the block type to the
  standard constructor for that kind of value — ideally by deserializing the metadata coming out of
  the filter, since the outgoing classes are already typed.

The shape of the win is that the two segments a format *must* describe are the two that are
genuinely format-specific, and the two that are usually boilerplate get defaults. It is why the
segment count was kept generic rather than hard-coded at three.

Nothing in the engine implements this yet, and no format can use it.
```
