=============
Writing tests
=============

There are two test surfaces in this project, with different owners and different purposes.

The engine's own tests
======================

The bulk of the coverage is in the Rust crate, as unit tests inside the module they exercise.

.. code-block:: console

    make check            # all of it — what the commit hook runs
    make test-unit        # unit tests
    make test-integration # integration tests
    make test-doc         # the examples in the doc-comments

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
   it. See :doc:`../whitepaper/validation`.

The whole-document tests are slower than the per-page ones by a wide margin. Run the per-page ones
constantly, the document ones before you claim to be finished.
