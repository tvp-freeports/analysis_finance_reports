=================
How to contribute
=================

Contributions to this project take five quite different shapes, and they happen in different
repositories, with different tools and different loops. The first useful thing this page can do is
help you find out which one you are about to do — because the setup, the commands and the review
that follow are not the same.

The project's repositories
==========================

The project is **not** a monorepo. It is a set of repositories that depend on each other at run
time and are maintained independently, which is the same decision as keeping formats out of the
engine: coverage should be able to grow without anyone touching the part that must not change.

.. list-table::
   :header-rows: 1
   :widths: 30 40 30

   * - Repository
     - Holds
     - You change it to
   * - ``analysis_finance_reports``
     - the engine, ``freeports-dev``, ``freeports-validate``, this documentation
     - fix or extend the extraction machinery itself
   * - a **formats repository**
     - format definitions, their tests, their validation documents
     - add support for a report layout
   * - an **input database**
     - company lists, the evidence for recognising them, target lists
     - change *which* companies a run looks for
   * - the website
     - `freeports.org <https://www.freeports.org>`_
     - change the public site

The last three are not one repository each: anyone can create and maintain a formats repository or
an input database, and several exist. `analysis_finance_reports_formats
<https://github.com/tvp-freeports>`_ is the reference formats repository, and there is a public
input database beside it.

Only work on the first repository needs a Rust toolchain, and only it has a build to set up.

Setting up this repository
==========================

One command from a fresh clone, and then whichever role you are in:

.. code-block:: console

    git clone <url-of-your-fork>
    cd analysis_finance_reports
    make init                      # venv, git hooks, everything installed
    source venv/freeports-dev/bin/activate

``make init`` installs everything, which is the right default for a first look around. When you know
which part you are working on, the narrower targets are faster:

.. code-block:: console

    make dev-engine      # the crate: extension rebuilt in place, binary, tests, lint
    make dev-formats     # the engine plus freeports-dev and freeports-validate
    make dev-docs        # the above plus Sphinx and the translation tooling

You also need a Rust toolchain — ``rustup`` with the stable channel. The engine is a Rust crate, so
there is no way around it. ``make doctor`` tells you what is installed, what is missing and which
target supplies each gap; :doc:`dev/build` explains the whole arrangement, and
:doc:`whitepaper/usage/installation` gives the same steps as plain commands for an environment the
Makefile knows nothing about.

The five development cycles
===========================

Each cycle is documented in full elsewhere. What follows is the shape of each one and where to go.

The engine cycle: edit, rebuild, test
-------------------------------------

In this repository, in ``packages/freeports``. The engine is one Rust crate that produces two things
— the ``freeports`` binary and the Python extension module — and **neither build implies the
other**, so a change to Rust that the Python side uses needs the extension rebuilt.

.. code-block:: console

    make develop        # rebuild the extension in place
    make test-unit      # the bulk of the coverage; fast
    make check          # the full suite: unit, integration, doctests
    make lint           # clippy on the crate, ruff on the Python sources

Tests come first, and they are meant to exhaust branches rather than sample them. Read
:doc:`dev/tests` before writing any, and :doc:`dev/index` for the conventions the review will hold
you to. :doc:`dev/implementation-notes` covers the technology choices — as opposed to
:doc:`whitepaper/design/index`, which covers the algorithm.

The format cycle: inspect, freeze, test
---------------------------------------

In a formats repository, **not here**, and it needs no change to the engine. The loop is four
commands and it is the reason ``freeports-dev`` exists:

.. code-block:: console

    freeports-dev inspect-document   # which page is what
    freeports-dev inspect-page       # what the engine sees, stage by stage
    freeports-dev make-tests         # freeze this page's behaviour as fixtures
    freeports-dev test               # does it still hold?

:doc:`whitepaper/formats/dev-loop` is what the loop is *for*, and
:doc:`whitepaper/formats/writing-a-format` walks it end to end with a real report. Start there,
with :doc:`whitepaper/formats/tooling` open beside it as the option-by-option reference.
:doc:`whitepaper/formats/repository` describes the repository the work lands in.

From this repository you can run a formats repository's tests without leaving it, which is what you
want after changing anything the formats side depends on:

.. code-block:: console

    make test-formats REPO=../analysis_finance_reports_formats
    make test-formats REPO=../analysis_finance_reports_formats FORMAT=EURIZON-EN23

The input database cycle: edit, load, fix
-----------------------------------------

In an input database repository. There is no compilation and no test suite here — the database is a
handful of CSV files, and the loop is the engine's own validation, which runs **before any PDF is
opened** and therefore answers in seconds.

:doc:`whitepaper/input-db` describes the layout, what buds and regexes are for, what is checked and
the two properties that surprise everyone, and its final section is the working loop itself.

The documentation cycle: write, build, translate
------------------------------------------------

In this repository, under ``docs/``. New prose is written in Markdown with MyST; the Python API is
generated from the installed packages, and the Rust API from ``cargo doc``.

.. code-block:: console

    make docs           # the whole site, rustdoc included
    make docs-html      # Sphinx only — what you want while editing prose
    make docs-serve     # read the result in a browser
    make i18n           # extract, merge and compile the translation catalogues
    make docs-lang DOCLANG=it

:doc:`dev/docs` covers the site, including the two rules that keep the side panel usable, and
:doc:`dev/i18n` covers translation. Note the warning in both: the pages under ``validation/`` are
content-addressed and cannot be edited casually.

The validation cycle: grant, sign, verify
-----------------------------------------

Mostly in a formats repository, with ``freeports-validate``. A grant is a signed statement that a
named methodology was applied to a specific file, recorded against that file's hash — so it is
invalidated by any change to the file, deliberately.

:doc:`whitepaper/validation` explains what the system claims and refuses to claim;
:doc:`validation/index` holds the published methodologies themselves; and
:doc:`whitepaper/formats/tooling` documents the commands, from creating your validation document to
re-granting after a file legitimately changes.

Guidelines for a change to this repository
==========================================

* **Write the tests first**, and write them to exhaust the branches rather than to sample them.
  Group them by topic in nested modules inside ``mod tests``.
* **Let the code document itself**, and use doc-comments for what the code cannot say: what a module
  guarantees, why it is built this way where the choice is not obvious, what its known limits are.
  Add runnable examples for non-trivial types — they become doc-tests and cannot go stale in
  silence.
* **Errors are typed**, one enum per module. A user path does not panic.
* **Do not widen the public API by accident.** ``api`` is the promise; the rest of the tree is
  internal and free to move.
* **Do not change a formats repository to accommodate an engine change.** Propose it instead: those
  repositories have other maintainers, and their reference output is a specification.
* **Fix inherited bugs at the root, but ask first.** Where the old behaviour may be depended on, an
  opt-in parameter that defaults to the old behaviour is usually the right shape.
* **A build product does not go into version control**, and neither does a run's output. ``make
  clean`` sweeps what a build leaves behind; if something keeps reappearing in your working copy
  that neither you nor ``clean`` put there, that is a bug worth reporting.
* Meaningful commit messages, with the issue id when there is one.

Before opening a pull request
=============================

.. code-block:: console

    make pre-commit

That is the same gate the commit hook fires — lint plus the full test suite — so if your commits
went through, it has already passed. Run it once more before the pull request anyway: the hook sees
the working tree rather than the staged snapshot, and a rebase can produce a commit nobody tested.

If you touched anything the formats side depends on, add the tests of a real formats repository:

.. code-block:: console

    make test-formats REPO=../analysis_finance_reports_formats

And if you touched the documentation, build it — a broken cross-reference is a warning, not an
error, and it is easy to miss:

.. code-block:: console

    make docs

.. note::

   No CI currently builds this repository; the pipeline in ``Jenkinsfile`` is stopped and waiting to
   be turned back on. Until then ``make pre-commit`` is the only gate there is, which is a reason to
   run it rather than to skip it.

Contributing to the website
===========================

The project's `website <https://www.freeports.org>`_ has its own `repository
<https://github.com/tvp-freeports/analysis_finance_reports_website>`_ and its own toolchain, with
nothing in common with the build described here.

Resources
=========

* `How to Contribute to Open Source <https://opensource.guide/how-to-contribute/>`_
* `Using Pull Requests <https://docs.github.com/en/pull-requests>`_
