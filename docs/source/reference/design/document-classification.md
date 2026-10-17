# Document classification

*Implemented.* Before a single page is classified ({doc}`classification`), a job has to know which
**format** its documents are, because the format is what says how their pages are to be read. This
page is about how that answer is reached when nobody gives it, and how it is checked when somebody
does.

## Why the document, and not only its URL

The engine has long been able to infer a format from where a document was downloaded:
`url_mapping.csv` maps address prefixes to format names. That is cheap and often right, but it is
evidence *about* the document, not *from* it. A local file has no URL at all; a publisher moves its
reports and the mapping falls behind; one address serves several layouts.

Document classification reads the evidence that is actually there: a line on the cover, the
typeface of a title. The two sources are kept side by side, on purpose. Each can be wrong in its own
way, and two independent sources that disagree are a signal no single source can give.

That is also why classification **runs even when the format is given**. A wrong `-f` does not fail:
it runs another format's recipe over the pages and produces empty tables, or worse, plausible ones.
The warning that the configured format is not the one the document looks like is the main reason
the mechanism exists, so it cannot be something that only happens when the format is left out.

## One machine, two ways of writing it

A repository writes its rules in `metadata/document_classify.yaml`
({doc}`../formats/document-classify-yaml`), and only when the grammar is not enough adds a Python
module ({doc}`../formats/document-classify-python`). The module runs first. A complete name from it
ends the search; a partial one becomes the starting point of the YAML file, which can only refine
it; nothing leaves the YAML file to start from scratch.

The Python module is not a second mechanism. A step of the YAML file is *exactly* a pipeline of three
standard pipes, and its criteria are the line selections the semistructured level already uses
({doc}`../../guides/formats/selections`) — the same choice {doc}`classification` made for pages: an
author classifies with the tools they already extract with. The price is that a criterion can only
say what a selection can say. That has been enough: the rules for all 25 formats of the reference
repository are about seventy lines of YAML, and none needs Python.

## The order of pages is the cost that matters

Turning a page into lines is by far the most expensive thing classification does. One page of a
1,824-page report takes 0.16 s to open and convert; the whole document takes 7 s. So the file lists
the pages to look at **in the order the author writes them**, and the search stops at the first one
that settles a complete name. A file that puts page 1 first and adds a later page only where page 1
is not enough usually reads a single page. Each physical page is converted at most once per
document, however many entries name it.

The pages read to classify are converted **again** when the job loads the document in full. Sharing
them between the two moments would save a fraction of a second at the price of a second way into a
document, and one way in was judged worth more. Measured with the classification always on, the
whole cost is roughly fixed, 0.1–0.2 s per job: +0.5% on the 1,824-page report, about +10% on a
68-page report whose title only appears on page 5.

What is deliberately absent is a criterion based on images. Of 942 PDFs — the test reports and a
905-document corpus — every one has text somewhere among its first six pages and its last two, and
comparing images would mean rendering pages, which is precisely the cost the page order exists to
contain. A criterion is one item in a list of criteria, so an image criterion can be added the day
a document needs one, without changing the grammar of steps.

## A state that can only grow more precise

Format names are hierarchical ({doc}`../formats/names`): entity, language, year, then place and
variant. A name cut short — `MEDIOLANUM`, `MEDIOLANUM-IT24.*` — stands for a set of formats, and that
is what makes classification a small state machine rather than a lookup.

The state is the **result so far**, initially nothing. A step whose name is complete and whose
criteria match ends the search. A step whose name is partial and matches becomes the new state —
and it is only looked at if its name *refines* the current one. The result can therefore only become
more precise, never be replaced by something else. The questions such a design usually raises —
does a later match override an earlier one, what if two partial matches contradict each other —
cannot arise: the successive states form a chain. The state carries from one page to the next, which
is what lets the cover say the entity and the table of contents the variant.

The steps are visited depth first, in the order written, and that order is part of the meaning: a
breadth-first visit gives a different answer on some files. A nested step is context, looked at only
once its enclosing block has been entered, so a generic criterion inside `MEDIOLANUM` cannot fire on
another group's report.

The price is that **a partial match is sticky**. Once a document is taken for `MEDIOLANUM`, no step
of any other entity is looked at again, on any page — so a report that mentions Mediolanum on its
cover as a distributor stops being recognised as what it is. Under a block with an unconditional
child (the *otherwise* of a block) it can even be classified wrongly. That is the trade accepted for
short criteria in the children, and it is why the reference page asks for a partial step's criteria
to be written with the care of a complete one.

The outcome is one of three: a complete name, a partial name with the formats it still allows, or
nothing. A run never extracts with a partial name — picking one of the candidates would be a guess —
so the job fails and the error lists the candidates; {doc}`../cli/freeports-classify` prints them as
a starting point.

## How the run weighs its sources

A job settles its format **inside the job**, after its documents are resolved and before the
format's recipe is loaded, because the documents are the evidence. The priority is fixed:
**configuration, then the document classification, then the URL**.

- Two names *agree* when some format of the repository falls under both, and the result is the most
  precise name both allow. The documents of one job are first combined source by source, then the
  two sources compared.
- The classification and the URL disagreeing is an `ERROR`, and the classification wins: it read
  the document, and a mapping that has fallen behind is the likelier mistake.
- A configured format that disagrees with either source is a `WARNING`, and the configured format is
  used. No setting turns that warning off: being explicit wins, but not silently.
- A classification that breaks — the repository's rules will not load, or a rule fails on one
  document — is an `ERROR` and counts as nothing when a format is configured, and fails the job when
  none is. The run never falls back on the URL alone when the author's classification is broken;
  that would be guessing too.

A job that cannot settle its format costs that job only. In a batch, rows may leave the format out;
the others run and the run exits `3`. Every crossing and its log level is listed in
{doc}`../cli/freeports`.

## Validated when loaded, not when used

The rules are checked as soon as they are read, against `formats.csv`, before any document is
opened. A complete name the repository does not declare is an error, even when it looks like a
refinement of real formats; being partial is always written out (`MEDIOLANUM-IT24.*`), so a typo
cannot quietly become a partial. A partial name that covers no format is an error, and so is a step
no document can ever reach. That last check carries, step by step, the set of states the machine
could be in at that point, and errs only on the side of silence: it names only steps no document can
reach. Python's answers cannot be known in advance, but the partial names it could return are a
finite set, so the check still runs with a module present. A misspelt or shadowed step costs the
author one error at load time rather than an unexplained miss months later.

The property this buys is the one the design was asked for: **no complete outcome of a valid YAML
file is a name the repository does not have.**

## What it achieved

On the 905 reports of the `bocc25` priority corpus, 904 are classified as the format their file name
says, none is left unclassified, and none is classified wrongly: the one exception is a file whose
label still names a format since merged into another, and the engine's answer is the right one. The
figure should be read for what it is. Two rules were widened after a first pass over this very
corpus, and only 13 of the 25 formats occur in it; the other twelve are checked on their test
reports alone, by the classification test every formats repository now runs on each `report.pdf`.
