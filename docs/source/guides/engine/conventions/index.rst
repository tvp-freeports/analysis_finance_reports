============================
Conventions and house style
============================

.. important::

   **These are conventions, not rules, and their only purpose is consistency.** They are what this
   codebase happens to have settled on, written down so that a reader can predict the next file
   before opening it and a contributor does not have to guess. Almost every one of them is
   *arbitrary* — a coin-toss between defensible options that was worth deciding once so nobody has
   to decide it again.

   So: follow them where they help, and say so where they do not. A convention that makes your
   change worse is a convention with a missing exception, and pointing that out is more useful than
   complying. What genuinely matters is much shorter than what follows — that the tests are
   honest, that errors are not silently swallowed, and that the next person can read what you
   wrote. The rest is house style, and house style is negotiable.

.. toctree::
   :maxdepth: 1
   :hidden:

   rust
   python
   testing

.. list-table::
   :header-rows: 1
   :widths: 24 76

   * - Page
     - Covers
   * - :doc:`rust`
     - the crate: module layout, doc-comments, error enums, the public surface, dependencies
   * - :doc:`python`
     - ``freeports_dev`` and ``freeports_validate``: layout, docstrings, ruff, the shell half
   * - :doc:`testing`
     - test-driven development where it fits, how tests are organised, and what "fast" means

What a change to the engine must do
===================================

The short list, in rough order of how much anyone will mind:

* **Write the tests first**, and write them to exhaust the branches rather than to sample them.
  :doc:`testing` is what that means in practice.
* **Let the code document itself**, and use doc-comments for what the code cannot say: what a module
  guarantees, why it is built this way where the choice is not obvious, what its known limits are.
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

Only the third, fourth and fifth of those are about *correctness*. The others are consistency, and
they are here because deciding them once is cheaper than deciding them per pull request.

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
