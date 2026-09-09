===========================
``ci.yaml``: the thresholds
===========================

One file, one name, one syntax, in all three kinds of repository — the engine, a formats
repository, an input database. It is committed, it is what the repository owner edits, and it is
the only file you have to open to answer "what does this repository demand of a commit".

.. code-block:: yaml

    branches:
      prod: [main, master, "release/*"]
      dev:  [dev, generalization]
      off:  [experimental, "wip/*", "spike/*"]
      default: dev
    keyserver: https://keys.openpgp.org
    thresholds:
      tests.rust.lines: 91
      tests.python.lines: 32
      lint.rust: 9.9

Patterns are shell globs, so ``release/*`` works and a plain name is an exact match.

``package.yaml`` and ``metadata.yaml`` say what a repository *is*. ``ci.yaml`` says how it is
*gated*, which is a different question with a different audience — and the engine repository, which
has no manifest, needs no manifest invented for the purpose.

Two things in the file are **errors**, and deliberately:

* a branch matching two class lists. Resolving that silently would make the class of a commit
  depend on the order of a mapping nobody reads;
* a threshold on a metric this repository cannot measure. A threshold that can never be evaluated
  is a threshold somebody only thinks they have.

Absent, everything falls to the defaults: class ``dev``, no thresholds, the default key server —
which is to say a repository nobody has configured reports and never refuses.

.. tip::

   ``off`` is one of the ten words YAML 1.1 spells a boolean with, so a bare ``off:`` key parses as
   ``False``. The reader maps it back, so both ``off:`` and ``"off":`` name the class. You do not
   have to quote it.

The three tiers
---------------

Resolved **per setting**, so raising one bar leaves every other where it was.

.. list-table::
   :header-rows: 1
   :widths: 14 32 27 27

   * -
     - Thresholds
     - Branch class
     - Key server
   * - command line
     - ``--min tests.rust.lines=95``
     - ``--branch-class prod``
     - ``--keyserver URL``
   * - environment
     - ``FREEPORTS_CI_MIN_TESTS_RUST_LINES``
     - ``FREEPORTS_CI_BRANCH_CLASS``
     - ``FREEPORTS_VALIDATE_KEYSERVER``
   * - file
     - ``thresholds:``
     - ``branches:``
     - ``keyserver:``
   * - default
     - none — reported, never gated
     - ``dev``
     - ``keys.openpgp.org``

``--min metric=value`` rather than a flag per metric: the set of metrics grows, and the per-package
minima make it combinatorial. The environment name is the metric upper-cased with dots as
underscores, prefixed ``FREEPORTS_CI_MIN_``.


The metrics
-----------

.. list-table::
   :header-rows: 1
   :widths: 30 10 8 25 27

   * - Metric
     - Unit
     - Cost
     - Repositories
     - Measured by
   * - ``tests.rust.lines``
     - percent
     - **slow**
     - engine
     - ``make coverage-rust``
   * - ``tests.python.lines``
     - percent
     - **slow**
     - engine
     - ``make coverage-python``
   * - ``tests.python.<pkg>.lines``
     - percent
     - **slow**
     - engine
     - ``make coverage-python``
   * - ``tests.formats.integration``
     - percent
     - fast
     - formats
     - ``freeports-dev coverage``
   * - ``tests.formats.single_page``
     - percent
     - fast
     - formats
     - ``freeports-dev coverage``
   * - ``docs.rust``
     - percent
     - **slow**
     - engine
     - ``make doc-coverage-rust``
   * - ``docs.python``
     - percent
     - fast
     - engine, formats
     - ``make doc-coverage-python``
   * - ``lint.rust``
     - score /10
     - fast
     - engine
     - ``make lint-rust``
   * - ``lint.python``
     - score /10
     - fast
     - engine, formats
     - ``make lint-python``
   * - ``grants.coverage``
     - percent
     - **slow**
     - all with ``validation/``
     - ``freeports-validate collect``
   * - ``grants.keys_online``
     - percent
     - **slow**
     - all with ``validation/``
     - ``freeports-validate check-keys``

**Cost is a property of the metric, not of the caller.** ``tests.rust.lines`` costs minutes whether
a hook, a person or a pipeline asks for it, which is what lets the hook run the fast set at every
commit and read the slow ones from the last full run.

Five of these are slow for two different reasons, and :ref:`fast-and-slow` is why both count as one
category. ``tests.python.lines`` and its per-package family re-run both suites under ``coverage.py``
— twenty-four seconds, for figures the suites themselves have already established the *outcome* of.
``docs.rust`` needs a nightly toolchain. ``grants.coverage`` and ``grants.keys_online`` are quick on
this machine and depend entirely on somebody else's being up.

Per-package minima
~~~~~~~~~~~~~~~~~~

``tests.python.lines`` and ``docs.python`` each have a per-package family. **The most specific
configured name wins for that package, and the aggregate applies to whatever no specific name
claims** — a package with its own minimum leaves the aggregate's denominator, so the two can never
contradict each other.

What each metric measures, and what it does not
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

**Test coverage, Rust.** ``cargo llvm-cov``, line percentage. It needs ``venv/freeports-dev``
active: six tests in the crate's ``python_boundary`` submodules import PyMuPDF through PyO3, and
without the environment they panic, the run exits non-zero, and llvm-cov writes *no report at all*.

**Test coverage, Python.** ``pytest --cov`` per package, over the same fast selection the commit
gate uses. *An honest limit:* ``freeports_validate`` is mostly shell scripts, so its figure covers
``cli.py`` and the three modules under ``lib/`` only. The shell half is exercised by the slow
process tests and **is not measured**. Do not read that number as coverage of the whole command.

**Test coverage, formats.** A directory walk, so it is fast. The unit is the **document** — a
format directory holding a ``report.pdf``, or each variant subdirectory of a multi-document format:

* ``tests.formats.integration`` — documents with an ``out/``, i.e. for which a whole-document test
  is collected at all;
* ``tests.formats.single_page`` — documents with at least one page carrying all three of
  ``<n>-pdf_blks.json``, ``<n>-txt_blks.json`` and ``<n>-results.json``.

The two are not averaged into one, because a repository can be strong in one and weak in the other
and one number would hide it. The walk is
:mod:`freeports_dev.format_inventory`, **shared with the pytest plugin**, so the report cannot claim
a coverage the suite does not collect.

**Documentation coverage, Rust.** ``cargo +nightly rustdoc -- --show-coverage``, aggregated from the
per-file item counts.

**Documentation coverage, Python — and why not sphinx's number.** ``sphinx.ext.coverage`` measures
whether an object *appears in the built site*, which is a fact about how ``autosummary`` is
configured rather than about whether anything is documented. Two things follow, and both make it
unusable as a threshold: every module of the compiled ``freeports`` extension reports 100.00 %
because ``inspect.getmembers`` attributes nothing to it — a vacuous 100, not a perfect one — and
its TOTAL line is a union over object names, which is why it reads 25.93 % while nearly every row
above it reads 100 %. A threshold on that would move whenever somebody edited a template.

So the gated metric is **docstring coverage**, an AST walk over public modules, classes, functions
and methods. ``make docs-site-coverage`` still reports on the site, which is a real question worth
asking — it is simply a different one.

**Lint.** A score out of ten, with pylint's own formula:

.. code-block:: text

    score = max(0, 10 - 10 * (5 * errors + warnings) / statements)

Pylint's formula because the number then means what it meant on the trend graph this project used
to plot, and because a raw count is not comparable between a 2 900-line repository and a
44 000-line one. ``statements`` is approximated by **non-blank, non-comment lines over the file set
the linter was given** — an approximation, stated as such, and stable enough to ratchet against.
Docstrings are counted in that denominator on purpose: excluding them would make the score rise as
documentation is deleted.

**The raw counts print beside the score, always.** A score is what a threshold compares; a count is
what a person fixes.

**Grants.** ``grants.coverage`` is read out of ``freeports-validate collect``'s model, so a commit
resolves each methodology page once rather than once per thing that wants a number from it.
``grants.keys_online`` is the fraction of granters whose key is published on the configured key
server.
