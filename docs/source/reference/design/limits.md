# Accepted limits, and open questions

Every design has edges. These are recorded rather than hidden, because a limit you know about is a
constraint and a limit you do not is a bug report waiting to happen.

## Accepted limits

*These are decisions, not omissions. Each has a reason and a cost that was judged acceptable.*

**A pipe needing the target companies works only in the first step.**
`FilterData` is an enum with two variants — the target companies at the first step, the accumulated
results at later ones — and they are never both available ({doc}`schedule`). Carrying both
everywhere would make every pipe pay for a case almost none of them want.

**Sequence-dependent reasoning exists only in classification.**
A format may express "this page continues the previous one" in its page-class finalizer, and nowhere
else. Extraction is per page, full stop ({doc}`pages`).

**Panics on paths whose input cannot come from outside.**
Carried over from the Python original, and left as they are rather than converted into errors that
would never fire: a recorded known edge is more honest than a pretence of handling. Where a value
*can* come from outside — a page's own coordinates, a format's configuration, a Python pipe's
arguments — the construction is fallible and the failure is handled where it happens.

A panic is nonetheless **contained at the page**, so that one this reasoning got wrong costs a page
and not a run of 904 jobs. The net does not replace the rule; it makes being wrong about the rule
survivable, and says so in `.log.csv` with the document and the page.

**The PDF line model, the line selections and the tabularizer were not redesigned.**
Ported essentially as they were, and improvements noticed during the port were **reported rather than
applied**. That code has been used for a long time, the existing formats depend on its exact
behaviour, and a rewrite would have been risk with no user asking for it. The place to record an
improvement you cannot justify is a note, not a commit.

**PyMuPDF is a hard dependency.**
There is no mature Rust equivalent that reads a page's text with its layout, and writing one is not
this project. It is called once per document and its output becomes a native page immediately, so
the boundary is one call rather than a per-block round trip — but it is a boundary that cannot be
removed today.

## Known defects, not yet decided

*These are wrong, not merely limited. They are listed so that nobody rediscovers them.*

**`--separate-out` on an unnamed document produces an invalid path.**
The file name is `{table}__{report}.csv`, and an unnamed document's name defaults to its whole
absolute path, which contains separators. Naming the document works around it
({doc}`../../guides/user/documents`).

**`freeports.log.jsonl` is written in the working directory** while `.log.csv` sits next to the
output on the grounds that it is a product of the run. Two files of one run in two places; whether
that is right has not been settled. What is settled is that only `trace` writes the first one at
all, so the working directory of an ordinary run stays empty.

## Deliberately not built

**The fourth segment.** Separating filtering from semantic interpretation, with defaults for the two
segments that are usually boilerplate. Designed, argued for, not implemented — see {doc}`segments`.

**Parallelism of page classes and of deserialization blocks.** Cancelled by measurement, not
forgotten. {doc}`parallelism` says what the measurements were.

**Internal optimisation of the hottest pipe.** 30–54% of a job, single-threaded and deterministic,
never optimised. Possibly worth as much as the entire parallelism effort, with none of its risk.
