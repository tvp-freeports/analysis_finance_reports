# Parallelism

Two independent levels, both automatic by default. This page is how to drive them; *why* they are
shaped this way, and the measurements that cancelled two other levels, are in
{doc}`../design/parallelism`.

| Option | Sets | Default |
|---|---|---|
| `--workers` / `-j` | the default for **both** levels | `auto` |
| `--jobs` | the job level alone | inherits `--workers` |
| `--pages` | the page level alone | inherits `--workers` |

Each accepts a positive integer **or** `auto`. `auto` is a value you can write, not merely the
absence of one: it is how a level is put back to automatic after a configuration file or an
environment variable has pinned it to a number.

Zero and negatives are a typed error naming the option you mistyped. From a configuration file the
same two levels are `n_workers` and the `parallelism` section:

```yaml
n_workers: auto
parallelism:
  jobs: 2
  pages: auto
```

## The two parallelism levels
**Jobs run in separate processes.** Loading a PDF goes through PyMuPDF, which means through Python,
which means through the GIL — profiling put that load at 35–75% of a job's wall time. Threads would
have re-serialised exactly the part worth parallelising. The children report their results back to
the parent, which writes the output once, so process-level parallelism does not change what is
produced.

**Pages of one document run in threads**, on a pool the crate owns rather than rayon's global one,
so that a program embedding the crate keeps its own.

`-j 1` therefore has a universal meaning: one job at a time *and* one page at a time.

## What parallelism buys, and what it costs
Measured on a 20-thread machine, two large reports in one run:

| | Time | Note |
|---|---|---|
| sequential | 39.4 s | |
| defaults | 16.7 s | **2.36×**, output byte-identical to sequential |

Page-level threading alone gave 4.4–7.0× *inside the engine*; the end-to-end figure is lower because
the PDF loading that only job-level parallelism touches remains.

The price is **memory**: peak went from 783 MB to about 1.2 GB for two concurrent large jobs, and it
grows with the number of jobs running at once. On a memory-constrained machine, `--jobs 1` keeps
page-level threading and drops the concurrent copies of the document — usually the right trade,
since it gives up the smaller of the two gains.

## When a format does not speed up

A format whose work is done by an author's Python pipe does not scale with threads, and **says so**:
those pipes declare that they do not, and the engine runs them sequentially rather than contending
for the GIL.

A format that gains nothing from `--pages` is usually one of these. That is the design working, not
failing — the alternative is threads that queue on the GIL and cost more than they save. Measured:
one small format with an author's `deserialize` went 182 ms → 189 ms between one thread and twenty,
while an entirely structured format went 7.2 s → 1.0 s on the same machine.

## Reproducibility

Parallelism does not change the tables. Results from the jobs are collected in **indexed slots**, so
the aggregated output is byte-identical whether you run at one worker or twenty — verified in the
test suite, and by checksum against the reference outputs of the formats repository.

The one place the ordering guarantee is spent is the **merging of log files**, where the children's
lines are poured into the parent's in job order rather than in the instant they happened. See
{doc}`../design/determinism`.
