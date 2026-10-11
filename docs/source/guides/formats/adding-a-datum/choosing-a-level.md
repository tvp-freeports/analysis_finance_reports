# Choosing a level, and raising it later

A datum can be written at three levels — a row of a spreadsheet, a block of YAML, or Python — and
they **sum** rather than exclude one another, by pipeline name. The question is never "which level
is this format", it is "which level is *this reading*", and a format mixing all three is ordinary.

## The rule

**Start at the most constrained level that holds, and go down only where you must. When you go
down, write why.**

That last clause is the part that pays. A module whose first line says *what* configuration could
not express is a module somebody can delete when the engine grows the cell; a module that says
nothing is a module nobody dares touch.

## What the levels can say

| Level | The algorithm | Its parameters | Reach for it when |
|---|---|---|---|
| `structured` | fixed, in the engine | columns of a CSV | the reading is the common shape of its datum |
| `semistructured` | named, in the engine or yours | YAML | the reading needs a criterion a cell cannot hold, or an anchor overridden |
| `unstructured` | yours | Python | the reading is a **computation**, not a description |

The practical frontier between the first two is narrower than it sounds. A CSV cell cannot hold a
mapping, so a selection written there uses the {doc}`compact grammar <../selections>`; the mapping
form in YAML says three things the compact form cannot — the two set operations and `font_of` — and
refuses a key it does not know, where a mistyped compact cell can only be caught by the shape of
the whole string. A format that needs none of those is better off as a row.

## Why the constrained level is worth the effort

Because one algorithm with a few parameters covers far more documents than anybody expects, and the
project has the count.

`PdfExtractInvestmentsStandard` did not start general. It accumulated logic and parameters to serve
documents that kept differing, and today the holdings — the best-covered datum of all — come out
like this:

| | measure |
|---|---|
| parameters of the algorithm | 4, with a default that leaves only the first required |
| columns of parameters per format | 9, in one CSV |
| **formats covered** | **26** from the `structured` level, plus fifteen files at the other two calling the same pipe |

Twenty-six formats from one algorithm, at the price of four parameters. Had each variation produced
an algorithm of its own, there would be twenty-six to maintain and no pattern for the next one to
follow.

## How to tell a parameter from an algorithm

Three questions, in this order.

1. **Is the logic the same?** If the two cases do the same thing and differ in *where* they look or
   *how* the value is written, the difference is a **parameter**. If they do different things —
   reading a ticked box and reading a section heading — they are two algorithms, however many
   parameters you add.
2. **What does the new case cost, and how many does it bring?** One more parameter covering five
   formats is a bargain; one more branch covering one usually is not. The cost is measured on the
   **caller** as well: a parameter with a sensible default is not paid for where it is not needed.
3. **Does the parameter describe the variation, or the format?** A parameter called `table` or
   `company_index` describes the document and generalises. An `if format == "ARCA-IT24"` describes
   **who is calling**, not what is on the page, and never generalises: it is the sign that the
   grouping is wrong or that the right parameter is missing.

The limit past which you do not go: **when the algorithm can no longer be explained in a
paragraph**, or when its parameters interact in ways that have to be tested in pairs, the
complication has outgrown the gain. Split it there — and split it **by logic**, never by format.

## Raising the level afterwards

A reading written in Python because it had to be is not finished business. The project ran one
sweep over every author pipeline it had, and the result is the best guide to what tends to move:

- **fifty-one** author pipelines were measured one at a time; **twenty-three** became
  configuration;
- the largest group was hidden behind a **name**: three formats were calling what looked like three
  algorithms and were writing the same twelve rows. Renaming it made the group visible, and five
  portfolio tables became configuration at once;
- the pieces that resisted were not the complicated ones. They were the ones that **compute** — a
  table whose geometry is recalculated per page — and the ones specific to two formats rather than
  to a kind of datum;
- what was expected to be delicate, a promise deposited in one segment and resolved in another, was
  not. What was actually hard is that **the pipes of a segment are concatenated, not composed**: a
  chain written inside one author function becomes **one** configured pipe with knobs, never three
  rows.

### Proving a promotion changed nothing

A promotion is a rewrite of a reading that is supposed to read the same, so the burden is
invariance, and the suite is not enough on its own:

- **a page fixture compares as a set.** Adding the configured pipe and forgetting to delete the
  author's one leaves duplicated, identical blocks that collapse, and the per-page tests pass with
  the pipe running twice. The whole-document test is what sees it;
- **a mechanical rewrite of selections is proven on every page, not on the frozen ones.** Build
  both forms in a script and compare them line by line — text *and* box — over every page of every
  test report of the formats involved, before running the suite;
- **expect the `.log.csv` to move, and predict by how much.** A log line names the pipe that spoke,
  so promoting a pipe that warns moves that cell at identical message. Count the rows beforehand:
  on one sweep the count said seven and seven is what moved.
