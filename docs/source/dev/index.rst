=======================
Developer documentation
=======================

This section is about contributing to **this** repository — the engine, its tooling and its
documentation. Writing a *format* is a different activity, in a different repository, and it is
covered by :doc:`the whitepaper <../whitepaper/formats/index>`.

:doc:`How to contribute <../contribute>` is the page above this one: it maps the project's
repositories, describes all five development cycles — engine, format, input database,
documentation, validation — and says which one you are in. Come here once you know it is this
repository.

.. toctree::
   :maxdepth: 1
   :caption: Contents:

   build
   implementation-notes
   tests
   docs
   i18n

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
:doc:`../whitepaper/formats/repository`.

Working on the crate
====================

From a fresh clone, the whole setup is one command:

.. code-block:: console

    make init                      # venv, git hooks, everything installed
    source venv/freeports-dev/bin/activate

Then, for the day-to-day loop:

.. code-block:: console

    make develop                   # rebuild the extension in place
    make build                     # the command-line binary
    make test-unit                 # unit tests: the bulk of the coverage
    make check                     # the full suite: unit, integration, doctest
    make lint                      # clippy on the crate, ruff on the Python sources

``make develop`` builds the extension module; ``make build`` builds the binary. They are two build
products of one crate and neither implies the other, so after changing Rust that the Python side
uses, rebuild the extension — a stale ``.so`` is the usual explanation for a change that "had no
effect".

Every target is a recipe of commands you could type by hand, and ``make help`` lists them all.
:doc:`build` explains the arrangement: one command per role, where things get installed, and what
``make doctor`` tells you when an environment is in a state nobody expected.

Conventions
===========

* **Tests are grouped by topic** into nested modules inside ``mod tests``, never a flat list of
  ``#[test]`` functions. The bulk of the coverage is in the crate's unit tests, and it is meant to
  exhaust branches rather than sample them.
* **Doc-comments describe what is there**, in English: what a module does, what it guarantees, why
  it is built that way where the choice is not obvious, and its known limits. Not contracts for a
  future implementer, not references to plans or milestones. Where a type is non-trivial, add a
  runnable example — it becomes a doc-test, so it cannot go stale in silence.
* **Errors are typed**, one enum per module, and a user path does not panic.
* **Public surface is the** ``api`` **module.** The rest of the tree is internal and may be
  reorganised; moving a type between internal modules is not a breaking change, and it should stay
  that way.
* **The output of a formats repository's tests is a specification.** Regenerating a reference file
  is a deliberate act with a reason, never a way to turn a red test green.

Continuous integration
======================

.. note::

   **No CI currently builds this repository.** The ``Jenkinsfile`` describes a pipeline that is
   stopped, not obsolete: it is kept, and kept current, because it will be turned back on. Until
   then, the real gate is the local one — ``make pre-commit``, which is what the commit hook runs.

Each of its stages invokes a make target rather than a command sequence of its own. That is
deliberate: the previous version of that file linted ``src/`` with pylint and ran ``pytest
tests/``, paths that vanished with the Python engine, and nobody noticed because the pipeline
described the build on its own terms. A target, by contrast, is exercised daily by whoever is
working here.
