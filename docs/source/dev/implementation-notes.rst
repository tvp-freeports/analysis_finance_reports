====================
Implementation notes
====================

The technology choices behind the engine, each with the alternative that was rejected and the
reason. These are decisions about *how the thing is built*, useful to someone about to change it.

They are deliberately kept out of the whitepaper, whose design section is about the **algorithm** —
pages, classification, segments, promises — and not about which crates it is assembled from. See
:doc:`../whitepaper/design/index` for that.

The engine is Rust, and the parts that are not are named
========================================================

The engine was originally Python. It is now a Rust crate that Python imports, and the whole of the
logic — the data model, the pipelines, loading a formats repository, promise resolution, writing
the output — is native.

**Rejected:** keeping the engine in Python and optimising the hot spots. The hot spots were not
localised; the work is per page and per block and it is *all* of it.

**Two exceptions, and only two.** Python is reached in exactly two places, and both are recorded as
boundaries rather than left to grow:

#. **Loading the PDF**, through PyMuPDF. There is no mature Rust equivalent that reads a page's
   text with its layout, and writing one is not this project. PyMuPDF is called once per document,
   and its output becomes a native page immediately.
#. **Running an author's pipes**, because an unstructured format *is* Python by definition.

The original page dictionary is kept next to the native page, so an author's pipe receives what it
expects while native pipes work on native data — one conversion, not a round trip per block.
Nothing else in the crate mentions PyO3, and a Python error from an author's pipe is logged with its
traceback and converted to a typed error at the boundary; no Python exception travels further.

Oniguruma, not the ``regex`` crate
==================================

**Rejected:** the ``regex`` crate, which is faster and is what a Rust programmer reaches for by
reflex.

Formats repositories are full of patterns already written by their authors in Python/PCRE syntax,
with backreferences and lookaround. The ``regex`` crate does not support them, by design. Choosing
it would have meant rewriting every existing pattern and telling every future author that their
regex knowledge does not apply here. The engine uses ``onig`` so that a pattern that worked
yesterday works today.

The public API is a facade, not the module tree
===============================================

Everything reachable from ``api`` is a promise to library users; everything else is an internal tree
that may be reorganised at any time.

**Rejected:** exporting the module tree directly, which is free and is what most crates do. It makes
every file move a breaking change, and it publishes as commitments the fifty types that exist only
because the code is split into files.

The facade is deliberately narrower than the tree, with one recurring exception: a facade type drags
in the types of its own public fields and signatures. Exposing a block without its value type would
hand callers a struct they cannot read; exposing a fallible function without its error type would
hand them a ``Result`` they cannot match on. Those are not surface creep, they are what makes a
facade usable.

Typed errors everywhere, and no panics on a user path
=====================================================

**Rejected:** exceptions carried across the boundary, and ``unwrap`` on values that came from a
file.

One error enum per module, built with ``thiserror``. Panics remain only where the input genuinely
cannot come from outside, and where they do, the module says so. Two inherited panics are documented
limits rather than pending fixes — a deliberate choice to record a known edge instead of pretending
it was handled.

No pandas, no Pydantic, no Pandera
==================================

**Rejected:** reading the formats repository's CSVs with pandas and validating with Pandera, as the
Python original did.

The joins involved are lookups on a derived key — a hash map, ten lines. The validations are per-row
checks that need to report **the row number**, because these files are edited in a spreadsheet by
people who will never see a stack trace. Heavy dataframe dependencies bought a concise expression of
something that was not hard, and made the error messages worse.

Dates are hand-written for the same reason: parse, format, compare, no calendar arithmetic. A date
library would have been a dependency serving three functions.

Serialisation is derived, so fixtures are readable
==================================================

Everything crossing a segment derives serde's traits. The consequence that matters is not internal:
the per-page test fixtures are **JSON**, produced and consumed by the same derivation, instead of
Python pickles.

**Rejected:** keeping pickle compatibility with the existing fixtures. It would have frozen the data
model to Python's object layout, and pickles are opaque in a diff — a regression in a fixture should
be something you can *see*.

One logging system, three destinations
======================================

**Rejected:** one log stream with levels, which is the default answer everywhere.

What a person watches while a run happens and what a tool parses afterwards are different artefacts,
and serving both from one stream makes each worse. So: stderr for watching; a JSON-lines file for
tools; ``.log.csv`` as the extraction's own audit trail, anchored to pages and coordinates. See
:doc:`../whitepaper/usage/logging`.

A fourth destination existed and was **retired**: a YAML digest of the warnings and errors at
maximum verbosity. It duplicated, in a second format and a second file, records the JSON-lines file
already carries in full — and only at the one verbosity where that file is at its most complete.
What was valuable about it survives: the structured error record, with its ``Debug`` form, its
message and its whole ``source()`` chain, is what fills the ``error`` key of a JSON line.

This whole area was tuned against a real complaint and measured. The first instrumentation pass made
a 1,140-page job take **19 minutes** and produced a **2.8 GB** log file. After tuning: **13 seconds**
and **609 rows**, doing the same work. Instrumentation nobody can afford to leave on is
instrumentation that does not exist.

Processes and threads
=====================

The reasoning, the measurements and the two levels that the measurements cancelled are part of the
algorithm's design and live in :doc:`../whitepaper/design/parallelism`. Two notes belong here
instead, because they are about the implementation rather than about the design:

* the page-level thread pool is **owned by the crate**, in a ``OnceLock``, and is not rayon's
  global pool. A library must not seize the thread pool of the program embedding it — and this
  library is also a Python extension module, so the embedding program is a real thing with its own
  plans;
* job-level parallelism is child **processes** of the same binary, driven through a hidden
  ``--internal-worker`` flag and a two-file JSON protocol in a temporary directory. The flag is
  hidden because it is not a user interface but the channel between two copies of one program.

Documentation: one Sphinx site, rustdoc beside it
=================================================

**Rejected:** mdbook for the prose, which reads better and is pleasanter to write.

Three reasons survived scrutiny: the validation section already exists and is live content that
cannot be rewritten for free; publishing is already configured; and enabling MyST means the prose is
written in Markdown *inside* Sphinx, which is most of what mdbook was wanted for, without a second
toolchain.

A fourth reason was offered and turned out to be **false** on measurement — that four translations
already existed. There is one partial translation. The decision stands on the other three, and the
false one is recorded here because a reason that does not hold should stop being repeated.

rustdoc is generated and published as a sub-path, not transcribed into ``.rst``. A hand-written copy
of the Rust API goes stale in a week.

Some things were deliberately not redesigned
============================================

The PDF line model, the line selections and the tabularizer were carried over essentially as they
were, and improvements noticed during the port were **reported rather than applied**.

This is a decision, not an omission. That code has been used for a long time, its behaviour is what
the existing formats depend on, and a rewrite would have been risk without a user asking for it. The
place to record an improvement you cannot justify is a note, not a commit.
