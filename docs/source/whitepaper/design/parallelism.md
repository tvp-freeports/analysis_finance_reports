# Parallelism as a consequence

*Implemented.* Nothing in {doc}`pages`, {doc}`classification`, {doc}`schedule` or {doc}`segments`
mentions threads. That is the point: parallelism was **added without redesigning anything**, because
the page had been the unit of work all along.

This page is why the levels are what they are. How to drive them is {doc}`../usage/parallelism`.

## What the profiler found

Four real reports, 29 to 1,824 pages, profiled through the spans the engine already carries:

| Finding | Measure |
|---|---|
| loading the PDF through PyMuPDF | **35–75%** of a job |
| page classification | between 1:8.5 and 1:157 of the extraction steps |
| `text_filter` | 85–96% of the engine's own work |
| one pipe inside it — the standard investments text filter, pure Rust | **30–54%** of a whole job |
| `deserialize` | under **0.2%** everywhere |

Two of those numbers changed the plan, and one of them reversed it.

## Jobs get processes

Because the dominant cost is **behind the GIL**. PyMuPDF is Python; threads would have re-serialised
precisely the part worth parallelising.

The children send their results back over IPC and the **parent writes the output once**, so the
observable behaviour does not change.

**Rejected: letting each child write its own output.** It is simpler and it would have changed what
batch mode produces — ten directories instead of one set of tables with a `Report` column. A
parallelism strategy that changes the product is not a parallelism strategy.

## Pages get threads

Inside a job, the pages of a step run on a thread pool **the crate owns**, not rayon's global one.

A library must not seize the thread pool of the program embedding it. This one is a Python extension
module as well as a binary, so "the embedding program" is a real thing with its own plans.

Pipes **declare whether they scale with threads**, defaulting to yes, and the three Python pipe kinds
declare no. A format whose work is an author's Python pipe therefore runs sequentially rather than
contending for the GIL — measured, 182 ms → 189 ms between one thread and twenty, against 7.2 s →
1.0 s for an entirely structured format on the same machine.

## Two planned levels were cancelled by the measurements

This is the part worth reading twice, because it is the opposite of what a plan usually produces.

**Page classes and pipelines within a step** — cancelled. It would sit on top of a loop that already
saturates the machine: two nested units of work in rayon with no free threads to give them. The
pathological case it was meant to serve — very few pages, many heavy pipelines — **does not exist in
the corpus**: the smallest of 21 real reports has 29 pages, more than the machine has threads.

**Deserialization blocks above a threshold** — cancelled. It would have addressed 0.2% of runtime.

Both were **removed from the design** rather than shipped as options that default to off. An option
that is off by default never runs, is therefore never exercised, and still has to be maintained at
every change to the code it wraps. A feature that exists only to honour an earlier plan is a
maintenance cost with no user.

## What it costs

| | |
|---|---|
| gain | 39.4 s → 16.7 s (**2.36×**) on two large reports, defaults, output byte-identical |
| price | peak memory 783 MB → **~1.2 GB**, growing with concurrent jobs |

The memory is the real price, and it is why the job level is the one worth turning down on a small
machine: `--jobs 1` keeps page-level threading and drops the concurrent copies of the document,
giving up the smaller of the two gains.

## An optimisation not taken

The single hottest pipe — 30–54% of a job, pure Rust, single-threaded and deterministic — has never
been optimised internally. Indexing the target companies instead of scanning them, and reducing
regex compilation per call, could plausibly be worth as much as the whole parallelism effort, with
**no** risk to determinism.

It is recorded here rather than done, because it is a separate piece of work with its own
measurements to take. {doc}`limits`.
