=======================
Developer documentation
=======================

This section is about contributing to **this** repository — the engine, its tooling and its
documentation. Writing a *format* is a different activity, in a different repository, and it is
covered by :doc:`the whitepaper <../whitepaper/formats/index>`.

.. toctree::
   :maxdepth: 1
   :caption: Contents:

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

.. code-block:: console

    python3 -m venv venv/freeports && source venv/freeports/bin/activate
    pip install --upgrade pip maturin
    cd packages/freeports
    maturin develop --release      # rebuild the extension in place
    cargo build --release          # the command-line binary
    cargo test --lib               # unit tests: the bulk of the coverage
    cargo test --test '*'          # integration tests
    cargo test --doc               # the examples in the doc-comments
    cargo clippy --all-targets

``maturin develop`` builds the extension module; ``cargo build`` builds the binary. They are two
build products of one crate and neither implies the other, so after changing Rust that the Python
side uses, rebuild the extension — a stale ``.so`` is the usual explanation for a change that
"had no effect".

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

Commits are built by the project's CI, which lints, runs the tests, and builds this documentation.
A release is published from a version tag.

.. note::

   The pipeline configuration in ``Jenkinsfile`` predates the move of the engine to Rust: it lints
   ``src/`` with pylint and runs ``pytest tests/``, paths that no longer exist in this repository,
   and it runs no ``cargo`` step at all. Treat the local commands above as the real gate until it
   is updated.
