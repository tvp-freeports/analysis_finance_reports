# The page is the unit of work

*Implemented.* This is the assumption everything else in the engine rests on, and the one worth
understanding before any other.

## The assumption

> **A page carries the context needed to understand what is on it.**

A document is opened once, through PyMuPDF, and immediately becomes a sequence of native pages. From
that point nothing downstream knows a PDF reader was involved. Each page is then processed **on its
own**, with no knowledge of the pages before or after it.

The assumption is restrictive, and it is deliberately restrictive. It is false of some pages of some
reports, and the engine does not pretend otherwise — see {doc}`promises` for what happens there.

## What follows from it

Three good properties, and they are not independent benefits so much as three faces of the same one:

**Work divides.** Nothing in the description of the algorithm mentions threads or processes, and
that is why parallelism could be added without redesigning anything: pages that do not depend on
each other can be processed in any order, anywhere. See {doc}`parallelism`.

**A page is a testable unit.** A format's behaviour on one page can be frozen as a fixture and
checked in isolation — this is exactly what `freeports-dev make-tests` writes, three JSON files per
page, one per segment. A regression tells you *which segment* of *which page* changed, rather than
that a 1,800-page document now produces a different table.

**A failure is local.** A page the engine cannot read is a page that is skipped, with a log row
naming it and a coordinate inside it. It is not a run that dies, and it is not a silent hole either.

## What it costs

The cost is real and is paid in two places.

**Some values genuinely are not on the page.** A fund name printed once on the section's cover, a
currency declared in a heading forty pages earlier. The assumption says nothing about these, and the
answer is not to weaken it — see {doc}`promises`.

**Some layouts encode meaning in sequence.** "This table continues from the previous page" is a fact
about a pair of pages, not about a page. The engine's answer is to confine that reasoning to
**classification**, where a per-document finalizer may look at the whole sequence, and to keep it
out of extraction entirely. See {doc}`classification`.

## The alternative that was rejected

A second pass over the document: read it once to gather context, once more to use it.

It was rejected because it doubles the most expensive part of a run — PDF loading and page parsing
are 35–75% of a job — to serve a minority of values, and because it reintroduces exactly the
ordering dependency between pages that this assumption exists to remove. Once page 412 needs page 3
to have been read first, work no longer divides, a page is no longer a testable unit, and a failure
is no longer local. All three properties go at once.

Promises keep them. That is the whole argument for them.
