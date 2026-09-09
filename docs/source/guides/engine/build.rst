================
The build system
================

This repository holds three distributions written in two languages — a Rust crate that is *also*
the ``freeports`` Python extension, and two pure-Python tooling packages — plus a Sphinx site.
Each of those has its own tool already: cargo, maturin and pip, ``sphinx-build``. The ``Makefile``
at the repository root does not replace any of them. It orchestrates them, and it exists so that
each role has **one command to learn** rather than a page of instructions to follow in order.

``make help`` lists everything. It is generated from the comments on the targets themselves, so a
target that is added documents itself and a target that is removed disappears from the help —
which is the only arrangement under which the help stays true.

One command per role
====================

.. list-table::
   :header-rows: 1
   :widths: 25 45 30

   * - You are
     - and you need
     - so you run
   * - using the engine
     - the ``freeports`` command and the Python module
     - ``make install``
   * - working on the engine
     - the above, plus the extension rebuilt in place, tests, clippy
     - ``make dev-engine``
   * - writing a format
     - the engine plus ``freeports-dev`` and ``freeports-validate``
     - ``make dev-formats``
   * - writing or translating documentation
     - Sphinx, the i18n tooling, and the three packages installed
     - ``make dev-docs``
   * - maintaining all of it
     - everything, plus the distributions
     - ``make dev-all``, ``make dist``

On a fresh clone, ``make init`` is the whole first-time setup: it creates the virtual environment,
wires up the git hooks, and installs everything. There is no bootstrap script to find and run
first.

The extension and the binary are two products
=============================================

One crate, two build products, and **neither implies the other**: ``pip install`` and ``maturin
develop`` build the *extension module*, cargo builds the *binary*. This is the single most common
way an installation ends up half-done — the module imports fine and the ``freeports`` command is
nowhere — and it is why ``make install`` covers both:

.. code-block:: console

    make install          # install-engine + install-binary
    make install-engine   # only the Python extension
    make install-binary   # only the command, compiled and placed in $(bindir)

While working on the crate, ``make develop`` rebuilds the extension in place. A stale ``.so`` is
the usual explanation for a Rust change that "had no effect" on the Python side.

Choosing where things go
========================

Everything is addressed through ``ENV_PREFIX``, the Python environment being worked on. If you
already have one active — a virtualenv or a conda environment — that is the one used; otherwise it
is the repository's own ``venv/freeports-dev``, which ``make venv`` creates. Neither case needs a
flag, and a third environment is one variable away:

.. code-block:: console

    make install ENV_PREFIX=/opt/pythons/3.12

The binary follows the same rule: by default it lands in the active environment's ``bin/``, so
that ``make uninstall`` is complete and nothing is left behind in your home directory. The cost is
that it is not on your ``PATH`` with the environment deactivated. For a system-wide install, the
GNU variables are honoured:

.. code-block:: console

    make install-binary PREFIX=/usr/local
    make install-binary PREFIX=/usr DESTDIR=/tmp/staging   # for a packager

When something is wrong
=======================

.. code-block:: console

    make doctor

It reports rather than repairs: which environment is in use and its Python version, whether cargo
is present, which of the three distributions are installed, which commands are actually on disk,
and whether the external programs ``freeports-validate`` shells out to — ``gpg``, ``jq``,
``sha256sum``, ``realpath`` — exist. For each thing missing it names the target that supplies it.
It also recognises the specific inconsistencies this repository has produced before: a module
installed without its binary, and packages left behind by an earlier layout.

``make installcheck`` is the shorter question — it runs the command and imports the module, and
says nothing if both work.

The commit gate
===============

``.githooks/pre-commit`` runs ``make pre-commit`` — an alias of ``make ci-fast`` — on a ``dev``
branch, and ``make ci-full`` on a ``prod`` one. The gate is a *name*, not a list: what each consists
of is decided in the ``Makefile``, so either can grow without the hook being edited.

**The fast gate costs 3.8 seconds** with ``make -j``, which is what the hook passes, and that number
is a budget rather than an outcome. It holds lint, the two fast suites, the docstring walk, the
report and the verdict. Everything else — the instrumented coverage, the crate's integration tests
and doctests, the process-per-test half of the tooling suite, and everything that touches the
network — is run deliberately, and what has not been run at this commit is *listed as not run* at
every commit. :ref:`fast-and-slow` is the reasoning, in one page: the value of a check is how often
it runs, so the gate is sorted by cost and the rest is made visible rather than optional.

**Whether a failure refuses the commit depends on the branch.** ``ci.yaml`` at the repository root
puts every branch in one of three classes, and only ``prod`` refuses; ``dev`` — the default —
measures and reports. ``prod`` also inverts the gate: it runs everything first, so that nothing
about a production commit can be stale. ``make branch-class`` says which class you are on and the
rule that put you there. :doc:`../devops/index` is the whole of it: every metric and every suite, what each
measures and what it does not, why an unmeasured figure refuses on a production branch, and the
fingerprint rule.

Formatting is not part of it, deliberately. The git ``clean`` filter configured in ``.gitconfig``
runs ``ruff format`` on Python sources as they enter the index, so an unformatted working tree is
a normal condition rather than an error. ``make fmt`` formats both languages on demand, and
``make fmt-check`` verifies without rewriting.

Neither is the validation report, and it is no longer in the fast gate at all. ``make validation``
runs the three of them — ``validation-report`` to regenerate the badges, the block in ``README.md``,
the seven pages under ``docs/source/validation/report/`` and the coverage page; ``check-grants`` to
say whether the claims in ``validation/`` still hold; ``check-keys`` to say whether the granters'
keys are published. All three resolve pages over the network, which is nine seconds with it up and a
failure with it down, so they belong to ``make ci-full`` and to a production commit rather than to
every commit of a working day.

None of them is a gate. Grant integrity **never refuses, on any branch** — a granted reference
output regenerated by ``make-tests`` is ordinary work, and a contributor who does not hold the
granting key could not clear such a gate at all. What the grant check says is still worth reading:
an exit status of 3 means it could not check everything, and the grants it could not compare are not
vouched for by that run.

What stands in for it at every commit costs nothing. ``grants.coverage`` carries the commit it was
measured at, so the CI report — which *is* refreshed at every commit — paints it grey and names that
commit the moment it stops describing ``HEAD``. The report is allowed to be one run out of date;
what it is not allowed to be is out of date and silent about it.

Why not the Autotools
=====================

The question is a fair one — ``./configure && make && make install`` is the sequence everyone
recognises, and the Autotools are the portable build system par excellence. They are not used here
for a reason worth writing down.

What ``autoconf`` is *for* is discovering the platform's capabilities at compile time: which
functions exist, what the maths library is called, which flags shared libraries want. Those are
questions you ask while compiling C. Nothing here compiles C. The crate's answers come from
``rustc`` and its ``cfg`` machinery and from cargo's dependency resolution; the Python
distributions' answers come from PEP 517 and wheel tags. ``automake``, for its part, is built
around turning lists of C sources into programs and libraries with libtool — none of which applies
to ``cargo build``, which would appear only inside custom rules. The ceremony would arrive without
the leverage, and with it a dependency on autoconf, automake and m4 for anyone touching the build.

What the Autotools would genuinely add, we take without the machinery. The **conventions** are
adopted: the canonical target names (``all``, ``install``, ``uninstall``, ``check``,
``installcheck``, ``clean``, ``distclean``, ``dist``) mean what the GNU standard says they mean,
and ``install`` honours ``PREFIX`` and ``DESTDIR`` so that a future packager finds the handles it
expects. The one genuinely configure-shaped job here — checking for the external programs no
``pyproject.toml`` can declare — is ``make doctor``, at a fraction of the cost and with better
messages.

Cleaning
========

.. code-block:: console

    make clean        # build products and debris; the cargo cache is left alone
    make clean-docs   # only docs/build, _generated and _extra
    make clean-rust   # cargo clean — the next build will be a full one
    make distclean    # all of the above, and the repository's venv

``clean`` deliberately does not call ``cargo clean``: deleting ``target/`` costs a full
recompilation, which is not what should happen when a target is run out of habit. It does know about
the debris a working session leaves around the tree — ``build/`` trees, caches, and the
``freeports.log.jsonl`` a run at ``-vvv`` drops wherever it was started — so that a run that writes
where it should not is at least easy to sweep up.
