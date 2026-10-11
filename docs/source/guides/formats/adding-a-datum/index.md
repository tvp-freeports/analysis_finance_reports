# Adding a datum to a format

*I have a format, and a datum I want it to extract. What do I do?*

{doc}`../writing-a-format` answers the larger question — a report nothing supports, to a passing
suite. This area answers the smaller one that comes up far more often: the format exists, it reads
its holdings, and now the statement of net assets has to come out of it too.

The five steps are always the same, and they are in this order for a reason.

```{toctree}
:maxdepth: 1
:hidden:

finding-and-reading
where-each-logic-lives
choosing-a-level
recipes-and-tests
parameters
```

| Step | Page |
|---|---|
| 1. find the pages that carry the datum, and read one | {doc}`finding-and-reading` |
| 2. decide which segment each part of the reading belongs to | {doc}`where-each-logic-lives` |
| 3. write it at the most constrained level that holds | {doc}`choosing-a-level` |
| 4. freeze it, one test per **recipe** | {doc}`recipes-and-tests` |
| 5. when it holds, see whether the level can go up | {doc}`choosing-a-level` again |

## The one habit that matters more than the rest

**Every rule you write down says how many pages it was measured on, and the number travels with
it.**

This is not a style preference. A format is a claim about a layout, and a layout is only knowable
by looking: an anchor read off the page in front of you is a hypothesis, and the difference between
a hypothesis and a rule is a count. The project has paid for the difference more than once, and the
cheapest of those lessons is on the next page.

The commands that produce the count are two, and neither of them is the test suite:

```console
$ freeports-dev find-text -f CARNE-EN23 --regex 'Statement of Net Assets' --pages-only
$ freeports-dev inspect-document -f CARNE-EN23
```

The first says where a text occurs before any format exists; the second says how the format
classifies every page of the document once one does. Between them they turn *"it works on page 25"*
into *"it holds on 983 pages of 1 261, and the 80 it leaves out are these"*.

## What this area assumes you have read

{doc}`../selections`, because almost everything written in these steps is a selection, and
{doc}`../dev-loop`, because every measurement here is one of its four commands. The levels
themselves — what a row, a YAML block and a Python module can each say — are
{doc}`../advanced/index`.
