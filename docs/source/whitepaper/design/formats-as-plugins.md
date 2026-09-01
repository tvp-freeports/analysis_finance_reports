# Formats live outside the engine

*Implemented.* The project's main structural bet, and the reason there is an engine at all rather
than a program per issuer.

## The bet: the engine knows no report
The engine knows **nothing** about any particular report. What it knows is how to run a schedule of
steps over classified pages, feed each page through pipelines, and resolve what comes out. Which
pages, and what to do with them, comes from a **formats repository** — an ordinary directory, in its
own version control, maintained by whoever cares about those reports.

The consequence is the point: **supporting a new report needs no release of the engine**, touches
nothing anyone else maintains, and can be published independently. `analysis_finance_reports_formats`
is one such repository, and there is nothing privileged about it.

## What keeping formats outside buys, and what it costs
**Buys:** the part that does not change and the part that grows constantly are versioned, tested and
released separately. A layout that breaks costs one repository a fix, not a release of everything.

**Costs:** the engine cannot validate a format against a report it has never seen, so a
misconfigured repository fails at run time rather than at build time. This is why so much of the
engine's error handling is about **naming the row, the file and the format** — those errors are the
only feedback a repository author gets.

It is also why an unknown key, an unknown column and a page class no step mentions are all **errors**
rather than things ignored. In a plugin system, silence is the enemy.

## Three levels of effort, merged by name

A format can be specified at three levels, differing in how much of the algorithm the author
supplies:

| Level | The algorithm | Its parameters | You write |
|---|---|---|---|
| **structured** | in the library, fixed | columns of a CSV | rows in a spreadsheet |
| **semistructured** | in the library *by name*, or your own module | YAML | a name and a configuration |
| **unstructured** | in the repository | in the code | Python |

They **add up** rather than exclude one another. Merging happens by summing the **same-named**
pipelines of the three levels, and that is the **only** place the three levels meet: no other part
of the engine knows they exist.

## Why the merge is the interesting part

Because irregularity is rarely uniform. A report whose investment table is perfectly ordinary may
still name its funds in a way nothing can parameterise. Being able to inherit two segments and write
the third is what keeps a hard format from costing three times a simple one — and it only works
because the segments are separable ({doc}`segments`).

The design would fail if the levels were **alternatives**: an author hitting one irregular detail
would have to drop to Python for the whole format, and the structured level would serve only the
formats that need nothing at all.

## Collisions are errors, not precedence

Defining the same semistructured algorithm name **both** natively and in your own module is an error
rather than a precedence rule to be remembered. The check runs over every native name of the segment
as soon as your module loads.

A collision is nearly always a typo, and letting one side win silently is what makes a typo
expensive. The same reasoning as unknown keys, applied to names.
