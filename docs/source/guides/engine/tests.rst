=============
Writing tests
=============

There are two test surfaces in this project, with different owners and different purposes.

Which of them run at every commit
=================================

Every suite here is worth running, and not every suite is worth running *every time*. The line is
drawn by cost, and :ref:`fast-and-slow` is the whole argument: a gate that costs a minute is a gate
people commit around, and a check that gets skipped has no value at all.

.. code-block:: console

    make test-fast    # rust.unit + python.fast — what the commit hook runs, ~4 s
    make test-slow    # the integration tests, the doctests, the process-per-test half
    make test-all     # both — before you call a piece of work finished

**Deferring a suite does not hide it.** Each one records its outcome and the commit it ran at, so
``make ci-check`` prints a suite nobody ran here as ``NOT RUN`` and one that last ran elsewhere as
``STALE`` — neither of which reads as a pass. On a production branch each of those refuses the
commit, and the hook runs ``make ci-full`` first so that nothing is left unknown. The table of
suites, and what each verdict means, is in :doc:`../devops/index`.

If you are about to move a test across the line, read the tip under *What marks a test slow* in
:doc:`../devops/index` first. Twice in this repository the slow thing turned out not to be the test.

The engine's own tests
======================

The bulk of the coverage is in the Rust crate, as unit tests inside the module they exercise.

.. code-block:: console

    make test-rust             # all three of the below
    make test-rust-unit        # unit tests — the third that runs at every commit
    make test-rust-integration # integration tests — run deliberately
    make test-rust-doc         # the examples in the doc-comments — run deliberately

The unit tests are in the gate because they are the ones that answer a keystroke: 0.7 s, no fixture
files, no whole-document flow. The integration tests are what would notice a regression across the
whole flow and cost four times as much between them, so they are run when you ask — which the
verdict table then reminds you to do.

How much of it is covered is a *gated* figure, not a matter of taste: ``make coverage-rust`` and
``make coverage-python`` measure it and ``make ci-check`` compares it against the minimum this
repository sets for itself. Both are slow — one recompiles the crate instrumented, the other re-runs
both Python suites under ``coverage.py`` — so neither is in the commit gate, and both are read from
the last full run. :doc:`../devops/index` says where those minima live and what happens when one is missed.

Two conventions are not optional:

* **group by topic.** Tests live in nested modules inside ``mod tests``, one module per behaviour
  under test, never a flat list of ``#[test]`` functions. A file with two hundred flat tests is a
  file nobody can navigate.
* **exhaust, do not sample.** Cover the branches, including the ones that only fail, and stress the
  edges — a parser is expected to survive any input without panicking, and that is a test, not a
  hope.

A handful of tests reach Python, because two modules genuinely do: loading a document, and running
an author's pipe. Everything else stays native, which is what keeps the suite fast and
deterministic.

A format's tests
================

Tests for a format live in its **formats repository**, not here, and are run with ``freeports-dev``:

.. code-block:: console

    make test-formats REPO=path/to/formats-repo
    make test-formats REPO=path/to/formats-repo FORMAT=CARNE-EN23

Each format under ``tests/formats/<FORMAT>/`` has three things:

``report.pdf``
   the document.

``pages/<page class>/``
   per-page fixtures — ``<page>-pdf_blks.json``, ``<page>-txt_blks.json``, ``<page>-results.json``,
   and a ``filter_data.json`` per page class. They pin one page at a time, which is what tells you
   *which segment* broke rather than that the document did. Generate them with:

   .. code-block:: console

       freeports-dev make-tests --repo . --format CARNE-EN23 --page 25 --page-type investments

   They are JSON on purpose: a regression should be visible in a diff.

``out/``
   the expected output of the whole document. This is the repository's specification, not a
   snapshot: if a run diverges from it, the engine changed, and the divergence is the finding.
   Regenerating one of these files is a deliberate act with a reason attached — and because grants
   are made against the content of exactly these files, regenerating one invalidates the grant on
   it. See :doc:`../../overview/trust`.

The whole-document tests are slower than the per-page ones by a wide margin — a minute and a half
against five seconds, in ``analysis_finance_reports_formats``, because each one is a full extraction
run. So they are two suites, gated the way the engine's are:

.. code-block:: console

    make test-fast     # formats.single_page, at every commit
    make test-slow     # formats.integration, deliberately
    make test-all      # both

in that repository, or from anywhere against a repository that is not yours:

.. code-block:: console

    freeports-dev test --repo <path> --fast
    freeports-dev test --repo <path> --slow
    freeports-dev test --repo <path> --all

The targets and the flags are the same three selections — a format repository's ``Makefile`` is the
engine's, with the axes it has no use for taken out. Run the per-page ones constantly: that is the
loop format development actually happens in, and it is the one that tells you *which segment* broke
rather than that the document did. That repository's commit hook runs them at every commit and the
whole-document ones on a production branch, and ``make ci-check`` says which of the two describes
the commit in front of you.
