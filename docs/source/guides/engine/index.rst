=======================
Developing the engine
=======================

This section is about changing **this** repository — the Rust crate that is the engine, and the two
Python packages beside it. Writing a *format* is a different activity in a different repository and
is :doc:`../formats/index`; so is changing an :doc:`input database <../input-db/index>`.

.. toctree::
   :maxdepth: 1
   :hidden:

   build
   tests
   conventions/index
   advanced/index

What each page here answers
===========================

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Page
     - Answers
   * - :doc:`build`
     - one command per role, where things get installed, and what ``make doctor`` tells you
   * - :doc:`tests`
     - what a test in this repository is expected to do, and how the suites are divided
   * - :doc:`conventions/index`
     - the house style for Rust, Python and tests — and how much of it is arbitrary
   * - :doc:`advanced/index`
     - the technology choices, as opposed to the algorithm

What is in this repository
==========================

Three packages under ``packages/``:

``freeports``
   The engine. One Rust crate that produces two things: the ``freeports`` command-line binary, and
   the Python extension module that ``import freeports`` loads. There is no Python source tree
   underneath — the package *is* the compiled extension.

``freeports_dev``
   The ``freeports-dev`` command and a pytest plugin: inspecting pages, generating per-page
   fixtures, running a formats repository's tests.

``freeports_validate``
   The ``freeports-validate`` command: methodology grants and their verification.

Formats are **not** here. They live in formats repositories, maintained separately — see
:doc:`../formats/repository`.

The day-to-day loop
===================

From a fresh clone, the whole setup is one command:

.. code-block:: console

    make init                      # venv, git hooks, everything installed
    source venv/freeports-dev/bin/activate

Then:

.. code-block:: console

    make develop                   # rebuild the extension in place
    make build                     # the command-line binary
    make test-rust-unit            # unit tests: the bulk of the coverage
    make check                     # the full suite: unit, integration, doctest
    make lint                      # clippy on the crate, ruff on the Python sources

``make develop`` builds the extension module; ``make build`` builds the binary. They are two build
products of one crate and neither implies the other, so after changing Rust that the Python side
uses, rebuild the extension — a stale ``.so`` is the usual explanation for a change that "had no
effect".

Every target is a recipe of commands you could type by hand, and ``make help`` lists them all.
:doc:`build` explains the arrangement.

What the gate will do to your commit
====================================

.. note::

   **No CI currently builds this repository.** The ``Jenkinsfile`` describes a pipeline that is
   stopped, not obsolete: it is kept, and kept current, because it will be turned back on. Until
   then, the real gate is the local one — ``make pre-commit``, which is what the commit hook runs.

What that gate measures, what refuses and why, is a section of its own:
:doc:`../devops/index`. What it found on the last run is :doc:`../../dev/ci-report/index`.
