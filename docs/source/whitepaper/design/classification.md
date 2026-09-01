# Classification

*Implemented.* Before any extraction, every page is assigned a **page class** — or nothing.

## Classification is itself a pipeline

The same machinery as extraction, run for a different purpose. A format author classifies pages
with the tools already used to extract from them, rather than learning a second mechanism, and a
classifier can be written at any of the three levels ({doc}`formats-as-plugins`).

The consequence worth stating: the structured level's page-classification table is *rows of a
spreadsheet* — a page class, and the headers a page must contain to belong to it — and that covers
most formats without a line of code.

## Classified as nothing is normal

Most pages of an annual report are prose. They belong to no class, enter no step, and are never
looked at again. That is the common case, not an error.

## Classified as something no step wants **is** an error

A page assigned a class that no step in the schedule mentions means the format's **classifier and
its schedule disagree**. One of the two is wrong, and the run says so rather than silently dropping
the pages.

The asymmetry is deliberate. "No class" is a statement about the page: there was nothing here. "A
class nobody processes" is a statement about the format: two files that must agree do not. Treating
the second like the first would hide a configuration error behind an empty output table, which is
the failure mode hardest to notice and most expensive to debug.

## Classification runs per document, always
Classification runs **once per document**, even in a multi-document run, and this is not an
implementation detail — see {doc}`multidocument`.

Where a format needs cross-page reasoning to classify — *"this page is a continuation of the table
that started earlier"* — it supplies a **finalizer**, `compute_page_class`. It receives the raw
per-page classification **of one document** and returns the resolved one.

That is the whole of the engine's tolerance for sequence-dependent reasoning, and it is confined
here on purpose. Classification is where "which pages" is decided, so it is where a fact about a
*pair* of pages can be expressed; extraction is where "what is on this page" is decided, and it
stays per page ({doc}`pages`).

Running the finalizer over the union of several documents would be wrong in an obvious way: page 40
of document B would be treated as following page 39 of document A, and a continuation rule would
join tables that have nothing to do with each other.

## What classification costs
Measured: classification is between one eighth and one hundred-and-fiftieth of the work of the
extraction steps, and it weighs anything at all only where it is written in Python. On one large
report with an author's classifier it was 1.08 s of a 17.7 s job — 6.1%; on the others it did not
register.

That measurement is why classification was **not** given its own parallelism level even though it
is trivially parallel across pages. It is already inside the pages-in-threads level, and it was
never the cost.
