==================
Rust: house style
==================

The engine is one crate, ``packages/freeports``, on edition 2024. ``cargo fmt`` and ``clippy``
settle everything mechanical — ``make fmt`` and ``make lint`` — so this page is only about the
choices a formatter cannot make for you.

.. note::

   Everything below is *a* consistent answer, not *the* answer. Where a module has a reason to
   differ, differing is fine; what is not fine is differing by accident, because then a reader
   cannot tell the deviation from the pattern.

How the tree is arranged
========================

``src/`` is split by area rather than by kind — ``core``, ``input``, ``output``, ``cli``,
``formats_repo``, ``formats_utils``, ``commons``, ``python`` — and a module that grows a family of
children becomes a directory with a file of the same name beside it.

**A parent module is the vocabulary of its area.** It re-exports the types a consumer of that area
needs, so callers write ``use crate::core::pipeline::{Extracted, PipeError}`` and never learn how
the area happens to be split across files. ``core/pipeline.rs`` is the example worth copying: three
``pub mod`` declarations, then ``pub use`` of everything a caller should see.

That is what makes the internal layout free to move. Splitting a file, or merging two, is not a
breaking change as long as the parent's re-exports stay put.

**The public surface is** ``api``. Everything else is internal, whatever its visibility says. A
``pub`` item deep in the tree is public to the crate's own modules, not a promise to the outside.

Doc-comments
============

``//!`` at the top of every module, ``///`` on every public item. What goes in them is what the code
cannot say for itself:

* what the module or type **guarantees**;
* **why** it is built this way, where the choice is not obvious;
* its **known limits**.

Not: a restatement of the signature, a plan for a future implementer, or a reference to a milestone
or a ticket. Those go stale and read as instructions to somebody who is not there.

Where a type is non-trivial, add a runnable example. It becomes a doc-test, which means it cannot go
stale in silence — the one form of documentation the build can check.

The house voice is worth naming, because it is unusual and deliberate: module docs explain the
*decision*, often in a sentence that begins with what would have gone wrong otherwise. Read
``core/pipeline.rs`` or ``ci/metrics.py`` before writing a long one.

Errors
======

**One error enum per module**, named after the module — ``DateError``, ``ScheduleError``,
``DocumentError``, ``FreeportsConfigError``. Derived with ``thiserror``, and always
``#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]`` where the payload allows it, so that
tests can compare an error to an expected value rather than to a string.

**The message names the value that was wrong, and what was expected**::

    #[error("year must be between 0 and 9999, found {0}")]
    YearOutOfRange(i32),
    #[error("day must be between 1 and {max} for {year:04}-{month:02}, found {day}")]
    InvalidDay { year: i32, month: u8, day: u8, max: u8 },

Tuple variants for one value, struct variants as soon as there are two — an error nobody can read
at the call site is an error somebody will convert to a string and lose.

**A user path does not panic.** ``unwrap`` and ``expect`` are for invariants the type system cannot
express and the code has just established, not for "this should not happen". Where a panic really is
right, ``expect`` with a sentence saying what was assumed.

Naming
======

Ordinary Rust naming, with two habits worth knowing:

* **A newtype rather than a bare** ``String`` where the value has a meaning — ``PipelineName``,
  ``FormatName``. It costs three lines and removes a class of argument-swap bug.
* **Builders return** ``Result``, and their error enum is named for the builder:
  ``RectangleBuildError``, ``LimitsBuildError``. A type that can be constructed wrongly says so in
  its constructor's signature.

Dependencies
============

Adding one is a decision, and ``Cargo.toml`` is commented like prose for that reason — every
non-obvious dependency carries the sentence explaining why *that* one. Two standing choices you will
trip over if nobody says them:

* **Regular expressions use** ``onig`` **(Oniguruma), not the** ``regex`` **crate.** The syntax has
  to match what a format author writes in a formats repository, which is Python's, and the ``regex``
  crate deliberately does not support parts of it.
* **No PyO3** ``extension-module`` **feature.** The crate produces both a Python extension and a
  binary that *embeds* Python; that feature would break the second. ``Cargo.toml`` explains it at
  the ``[lib]`` section.

Prefer the standard library, then a crate this project already depends on, then a new one. "It is
one function" is a reason to write the function.
