# Tutorials

The rest of this area is reference: it tells you what exists and what it means. These two pages are
the other thing — someone walking you through the work, on a real report, with the commands and
their actual output.

```{toctree}
:maxdepth: 1
:hidden:

first-format
diagnosing-a-format
```

| Page | For when |
|---|---|
| {doc}`first-format` | a report is not supported and you want to support it |
| {doc}`diagnosing-a-format` | a format already runs, and something in the output is wrong |

## Read this before you decide it is not for you

Adding a format looks like it should require understanding the engine. It does not, and that is a
design decision rather than a happy accident: formats live outside the engine precisely so that the
people who know what a fund report *means* do not have to know how a PDF is taken apart
({doc}`../../design/formats-as-plugins`).

Concretely, what a simple format costs is **one row in a spreadsheet**. Here is a real one, the
whole extraction configuration for a Luxembourg fund report:

```text
ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency
ASTERIA-EN24,CenturyGothic-Bold(:87),"CenturyGothic-Bold[8.9802] ""(expressed in""",CenturyGothic (:810),4,1,5,3,2
```

Those trailing numbers are column positions in the report's table, and there is a command that
prints the table with its columns numbered so that you can read them off. That is the level of
difficulty we are talking about. {doc}`first-format` does exactly this row, from scratch.

Some reports are genuinely awkward, and for those you can drop to writing Python — but you drop
only the one segment that needs it, and you keep inheriting the rest ({doc}`../writing-a-format`).
Nobody starts there.

## The three habits that save the most time

**Get classification right before anything else.** Which pages the engine even looks at is decided
first, and everything downstream is meaningless until it is right. Debugging the extraction of a
page that was never selected is the classic way to lose an afternoon.

**Let the tool tell you which segment is wrong.** A page goes through three stages and you can print
the output of each one. *"The page is wrong"* has no handle on it; *"the second stage dropped these
rows"* has one.

**Look at what you are about to freeze.** `make-tests` records what the code does *right now*. Read
its output before confirming: a wrong result you confirm becomes the specification, and from then on
the test defends the bug.
