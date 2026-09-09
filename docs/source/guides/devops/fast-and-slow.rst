.. _fast-and-slow:

Fast and slow: the two categories
=================================

Everything this page describes rests on one split, so it comes first.

Why a gate that costs a minute gates nothing
--------------------------------------------

The more often the tests, the linters and the coverage run, the more solid the tree. That is the
entire argument for checking at every commit rather than at every release, and it is a good one.

It is also an argument that turns on itself the moment the gate costs real time. A person who waits
a minute to commit commits less often. Fewer, larger commits are harder to review and harder to
bisect; then somebody discovers ``--no-verify``; then the gate is a decoration. **A cheap check
that runs a hundred times a week is worth more than a thorough one that runs on Fridays** — not
because thoroughness does not matter, but because a check that is skipped has no value at all.

So the checks are sorted by **cost, not by importance**. Nothing here is optional; the question is
only when it is paid for.

What makes something slow
-------------------------

Two different expenses, and this workspace treats them as one category because the gate has to make
the same decision about both.

.. list-table::
   :header-rows: 1
   :widths: 22 40 38

   * -
     - It costs time
     - It needs the network
   * - Why it is out
     - minutes, or seconds multiplied by every commit of a working day
     - it depends on a host nobody here controls, so it can fail for a reason that has nothing to
       do with the commit
   * - Examples
     - the instrumented Rust coverage build, the crate's integration tests and doctests, the
       process-per-test half of the tooling suite, a formats repository's whole-document runs
     - resolving methodology pages, looking a granter's key up on a key server
   * - Told apart by
     - ``cost`` in the registry
     - ``online`` beside it, which only changes the *message*

They are kept apart in the tables so that a message can say *which* it is — "run it" and "check
your network" are different things to tell somebody — and gated alike, because "can this be paid
for at every commit" has the same answer for both.

A check is **fast** when it runs locally, needs no network, no second toolchain and no instrumented
rebuild, and costs a second or so. Everything else is slow.

What the split is *not*
-----------------------

It is not "the checks" and "the optional checks". It is **what this commit was checked against**
and **what it was not** — and the second half is written down, at every commit, in the same table
as the first.

That is what the rest of this page is about: every suite records its outcome *and the commit it ran
at*, every slow metric records the commit it was measured at, and ``ci-check`` — which measures
nothing and costs a fifth of a second — prints a suite nobody ran here as ``NOT RUN`` and a figure
from an earlier commit as ``STALE``. Neither can be read as a pass, for the same reason
``unmeasured`` cannot: **something nobody established is not something that holds.**

On ``dev`` that is a notification and the work continues. On ``prod`` every one of them refuses, and
the branch's gate is the slow one, so that nothing about a production commit is unknown.
