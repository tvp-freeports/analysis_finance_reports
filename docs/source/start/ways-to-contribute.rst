=================
How to contribute
=================

Contributions to this project tend to take a few recognisable shapes, in different repositories,
with different tools and different loops. The first useful thing this page can do is help you work
out which one is closest to what you are about to do — because the setup, the commands and the
review that follow are not the same.

.. important::

   **A project is shaped by the people who turn up, not the other way round.** The shapes below are
   how this documentation happens to be arranged today — a record of the work people have already
   done here, not a list of the work that counts. They are soft at the edges and certainly
   incomplete.

   So do not read the table looking for permission. Take a piece of one row and a piece of another,
   bring an expertise none of them anticipated, or arrive with a background this project has never
   had — that is the case in which everyone gains most. Reporting a report the engine reads wrongly,
   arguing with a design decision, rewriting one unclear paragraph and translating one page are all
   contributions, and none of them needs a role first. A contribution these pages have no section
   for is a good sign about the project and a gap in the documentation; the section gets written
   afterwards, which is how most of the rows below came to exist.

Which of these is closest?
==========================

.. list-table::
   :header-rows: 1
   :widths: 26 40 34

   * - You want to
     - Which usually means working in
     - A good place to start
   * - get results out of it, not change it
     - nothing — you are a user
     - :doc:`../guides/user/index`
   * - fix or extend the extraction machinery
     - ``analysis_finance_reports``, in Rust
     - :doc:`../guides/engine/index`
   * - add support for a report layout
     - a formats repository, with ``freeports-dev``
     - :doc:`../guides/formats/index`
   * - change *which* companies a run looks for
     - an input database — CSV files, no build
     - :doc:`../guides/input-db/index`
   * - vouch for a file under a published methodology
     - a formats repository, with ``freeports-validate``
     - :doc:`../guides/grants/index`
   * - write or reorganise documentation
     - ``docs/`` in this repository
     - :doc:`../guides/docs/index`
   * - translate the documentation
     - ``docs/source/locales/``
     - :doc:`../guides/i18n/index`
   * - own the checks that refuse a commit
     - ``ci.yaml``, the hooks, the ``make`` targets
     - :doc:`../guides/devops/index`

None of these is a prerequisite for another, none of them is a commitment, and the middle four
happen in repositories this one does not contain. That is deliberate: coverage should be able to grow without anyone touching the
part that must not change.

The shape of each loop
======================

Each guide above documents its loop in full. What follows is the shape of each one, so you can
recognise yours before committing an afternoon to it.

**The engine cycle — edit, rebuild, test.** In ``packages/freeports``. The crate produces two
things, the ``freeports`` binary and the Python extension module, and **neither build implies the
other**, so a change to Rust that the Python side uses needs the extension rebuilt. Tests come
first and are meant to exhaust branches rather than sample them.

**The format cycle — inspect, freeze, test.** In a formats repository, and it needs no change to
the engine. Four commands: ``inspect-document`` to find out which page is what, ``inspect-page`` to
see what the engine sees stage by stage, ``make-tests`` to freeze that page's behaviour as
fixtures, ``test`` to ask whether it still holds.

**The input database cycle — edit, load, fix.** No compilation and no test suite: the database is a
handful of CSV files, and the loop is the engine's own validation, which runs **before any PDF is
opened** and therefore answers in seconds.

**The grant cycle — read, grant, sign.** A grant is a signed statement that a named methodology was
applied to a specific file, recorded against that file's hash — so it is invalidated by any change
to the file, deliberately.

**The documentation cycle — write, build, read.** New prose is Markdown with MyST; the Python API
is generated from the installed packages and the Rust API from ``cargo doc``. Build with
``make docs`` and read the result with ``make docs-serve``.

**The translation cycle — extract, merge, compile.** ``make i18n`` moves the catalogues forward;
``make docs-lang DOCLANG=it`` builds one language.

Before opening a pull request
=============================

.. code-block:: console

    make pre-commit

That is the same gate the commit hook fires — lint plus the fast test suite — so if your commits
went through, it has already passed. Run it once more before the pull request anyway: the hook sees
the working tree rather than the staged snapshot, and a rebase can produce a commit nobody tested.
:doc:`../guides/engine/conventions/index` is what the review will hold you to, and
:doc:`../guides/devops/index` is what the gate is doing.

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
