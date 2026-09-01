# Determinism and observable order

*Implemented.* What the engine guarantees about order, what it does not, and why the guarantee was
deliberately weakened in exactly one place.

## Ordered maps where order is observable

Promise maps, schedule steps and pipe order all use **insertion or key order**, never hash order.

**Rejected: hash maps**, which are faster and were what the Python original used for some of these.

The cost of hash order is not correctness but **reproducibility**, in two specific ways:

- the order pages are processed in determines the order of the output rows;
- the order ids are visited in determines **which cycle** a promise failure reports.

A test that passes four times out of five is worse than a slower map. An error message that names a
different cycle on each run cannot be asserted, so it stops being tested, so it stops being
maintained.

## What is guaranteed

| | Guaranteed |
|---|---|
| output tables | **byte-identical** regardless of the parallelism settings |
| row order within a table | deterministic for the same inputs |
| the cycle a promise error reports | the same cycle every time |
| `.log.csv` rows | sorted, and identical between one worker and many |

The first is the strong one, and it is tested rather than asserted: the same run at one worker and
at N produces the same files, checked in the integration suite and by checksum against the formats
repository's reference outputs.

## The one place it was relaxed

Byte-for-byte determinism was the original requirement for the parallel work. It was relaxed, on
purpose, to **semantic equivalence**: the parallel output must contain the same data, not
necessarily in the same order.

The relaxation was needed because a strict reading would have excluded the cheapest collection
strategies. What happened next is worth recording:

> **The licence to diverge was taken and then not used.** Results from jobs are collected in
> **indexed slots**, which costs nothing, so the aggregated tables stay byte-identical anyway. The
> margin was spent only on **merging log files**, where the children's lines are poured into the
> parent's in job order rather than in the instant they happened.

That is the ideal outcome for that kind of permission, and it is why reference outputs remain
comparable by checksum even though the constraint that protected them was formally dropped.

## What is not guaranteed

**The interleaving of log lines across concurrent jobs.** Two jobs running at once produce lines in
whatever order they happen; the merge restores job order at the end, not the original instants.

**Wall-clock timing in the logs**, obviously.

**That a page is processed on any particular thread or process**, which nothing downstream may
depend on — that is the whole content of {doc}`pages`.
