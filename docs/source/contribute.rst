=================
How to contribute
=================

There are two quite different things you might want to contribute, and they happen in different
repositories.

**A new format** — support for a report layout nobody has covered yet. That work happens in a
formats repository, not here, and needs no change to the engine. Start with
:doc:`whitepaper/formats/index`.

**A change to the engine or its tooling** — this repository. The rest of this page is about that.

Setting up
==========

Fork the repository, clone your fork, and work in a virtual environment:

.. code-block:: console

    git clone <url-of-your-fork>
    cd analysis_finance_reports
    python3 -m venv venv/freeports
    source venv/freeports/bin/activate
    pip install --upgrade pip maturin
    pip install packages/freeports_dev packages/freeports_validate
    cd packages/freeports && maturin develop --release

You also need a Rust toolchain — ``rustup`` with the stable channel. The engine is a Rust crate, so
there is no way around it.

Guidelines
==========

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
* Meaningful commit messages, with the issue id when there is one.

Before opening a pull request, from ``packages/freeports``:

.. code-block:: console

    cargo test --lib && cargo test --test '*' && cargo test --doc
    cargo clippy --all-targets

and, if you touched anything the formats side depends on, the tests of a real formats repository.

Documentation and translation
=============================

See :doc:`dev/docs` for building the site and :doc:`dev/i18n` for the translation workflow. Note the
warning in both: the pages under ``validation/`` are content-addressed and cannot be edited casually.

Contributing to the website
===========================

The project's `website <https://www.freeports.org>`_ has its own `repository
<https://github.com/tvp-freeports/analysis_finance_reports_website>`_.

Resources
=========

* `How to Contribute to Open Source <https://opensource.guide/how-to-contribute/>`_
* `Using Pull Requests <https://docs.github.com/en/pull-requests>`_
