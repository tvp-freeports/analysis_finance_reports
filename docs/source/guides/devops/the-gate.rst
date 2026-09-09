.. _commit-gate:

===============
The commit gate
===============

What runs at every commit, and why that set
-------------------------------------------

On a ``dev`` branch the hook runs ``make pre-commit``, an alias of ``make ci-fast``:

.. code-block:: console

    make ci-fast    # lint, the fast suites, the docstring coverage,
                    # the report, and the verdict

On a ``prod`` branch it runs ``make ci-full`` instead — everything gated, measured at this commit.
Same three layers, same report, same verdict; only the set of measurements differs, and it differs
by exactly the line drawn above.

.. list-table::
   :header-rows: 1
   :widths: 26 37 37

   * -
     - ``make ci-fast`` (dev)
     - ``make ci-full`` (prod)
   * - Linters
     - clippy, ruff
     - clippy, ruff
   * - Test suites
     - ``rust.unit``, ``python.fast``
     - all of them, online included
   * - Coverage
     - —
     - ``cargo llvm-cov``, ``pytest --cov``
   * - Documentation
     - the docstring walk
     - the docstring walk, and rustdoc on nightly
   * - Grants
     - —
     - the report, ``check-grants``, ``check-keys``
   * - Cost here
     - **3.8 s** with ``make -j``, 5.5 s without
     - minutes
   * - What may be stale
     - anything slow, and it says which
     - **nothing**

The figures are from this repository on 2026-09-09, with warm caches. What keeps them there is what
the fast gate refuses to do: it does not run the whole crate suite, nor both Python suites twice over
— once as tests, once again under coverage — nor a second rustdoc toolchain, nor any command that
fetches a methodology page over the network. Every one of those is in ``ci-full``, where a wait is
something you chose.

The gate is a **name, not a list**. What ``ci-fast`` and ``ci-full`` consist of is decided in
``mk/ci.mk``, so either can grow without a hook being edited again.

.. note::

   **Both gates use** ``.WAIT``. Their measurements are independent of one another, the report may
   only be rendered once all of them have landed, and the verdict may only be read once the report
   is written — which is exactly what ``prerequisites .WAIT report .WAIT check`` says. Under ``make
   -j`` the first group runs at once, which is where the 5.5 seconds above becomes 3.8;
   without ``-j`` the same line still runs them in that order. ``MAKEFLAGS += -Otarget`` in the
   ``Makefile`` keeps each recipe's output whole rather than interleaved.


The same gate in a format repository
------------------------------------

**Every freeports repository has a** ``Makefile`` **and a hook that names a target in it.** The
arrangement above is not the engine's arrangement; it is the workspace's, and a format repository
runs the same three layers under the same names — measure, report, judge — with a smaller surface
because it has a smaller thing to measure.

.. list-table::
   :header-rows: 1
   :widths: 26 37 37

   * -
     - ``make ci-fast`` (dev)
     - ``make ci-full`` (prod)
   * - Linter
     - ruff over ``content/``
     - ruff over ``content/``
   * - Test suites
     - ``formats.single_page``
     - both, whole documents included
   * - Coverage
     - the document inventory
     - the document inventory
   * - Documentation
     - the docstring walk
     - the docstring walk
   * - Grants
     - —
     - the report, ``check-grants``, ``check-keys``
   * - Fingerprint
     - checked; warns
     - checked; **refuses**
   * - Cost
     - **6 s** in ``analysis_finance_reports_formats``
     - minutes
   * - What may be stale
     - anything slow, and it says which
     - **nothing**

The target names are the engine's names — ``test-fast``, ``test-slow``, ``lint``, ``coverage``,
``ci-fast``, ``ci-full``, ``ci-check`` — and each is one ``freeports-dev`` or ``freeports-validate``
invocation you could type by hand. What is *missing* is the second axis: the engine's surface is
laid out as ``test``/``lint``/``coverage`` × ``rust``/``python``, and a format repository has one
test command whose two halves are told apart by a marker rather than by a language. So there are
twenty-seven targets there against ninety-four here, in one file rather than eight.

.. note::

   **Two command surfaces, and the division is what each one acts on.** ``make`` changes the
   repository you are standing in — it writes the measurements, refreshes the committed reports, and
   rewrites the manifest when it is entitled to. ``freeports-dev`` answers questions *about* a
   repository, including one that is not yours, and writes only where you point it with ``--out``.
   ``make test-fast`` is ``freeports-dev test --fast`` with ``--repo`` pointed here; ``make
   ci-check`` is ``freeports-dev ci-check``. Someone who has learnt one can use the other.

**What may refuse there is narrower than here.** In a format repository only ``ci-check`` and the
fingerprint rule refuse — the suites and the grant checks record their outcome and leave the verdict
to the one step that can see which branch you are on. The Makefile expresses that with a
target-specific variable the two gates set and every prerequisite inherits: reached from ``ci-fast``
a failing suite is recorded, and ``make test-fast`` typed on its own still fails. Here the same
narrowing is done by the branch class rather than by the target, which is a difference of mechanism
between two repositories rather than of policy.

``GATE`` exists here too, and one recipe reads it: ``check-grants``, so that grant integrity cannot
refuse a commit through a failing ``make``. That is the note below, and it is the one thing on which
the target has to do the narrowing in both repositories — a branch class cannot, because the
narrowing is not "on this branch" but "never".

The three branch classes
------------------------

``ci.yaml`` at the root of each repository puts every branch into one of three classes:

.. list-table::
   :header-rows: 1
   :widths: 12 30 30 28

   * - Class
     - Test suites
     - Thresholds
     - Fingerprint vs version
   * - ``prod``
     - must pass — **refuses**
     - enforced — **refuses**
     - **refuses**
   * - ``dev``
     - run and reported
     - reported
     - reported
   * - ``off``
     - the hook does nothing at all
     - —
     - —

The class is derived at every run from the branch you have checked out, so switching branch sets
nothing and forgets nothing. **The default is** ``dev``: a branch nobody classified is not
production, and the quiet answer — ``off`` — is never the default one.

.. code-block:: console

    $ make branch-class
    branch dev -> dev
      because 'dev' in branches.dev
      everything is measured and reported; nothing refuses the commit

Run that first whenever a hook does something you did not expect. It names the rule that produced
the class, which is the whole answer; "prod" on its own is not.

.. note::

   **The branch class is what decides whether a hook refuses.** A failing ``make pre-commit``
   refuses the commit on ``prod`` and only there; on a development branch the same failure is
   reported and the commit stands.

   Grant integrity is not subject to that rule at all: a broken, stale or unverifiable grant is
   reported loudly and **never refuses**, on any branch, **in any repository — this one included**.
   A granted reference output regenerated by ``make-tests`` is ordinary work, and a contributor who
   does not hold the granting key could not clear such a gate at all. This repository used to be the
   exception by accident: ``ci-full`` depends on ``validation``, a failing recipe fails the make, and
   a failing make refuses the commit — so a merge here *was* refused over a grant. ``check-grants``
   now reports inside the gate and fails only when run on its own.

   What may refuse is a *threshold* and the *fingerprint rule*. For the grants that means two
   numbers and nothing else: ``grants.coverage``, so that enough of the repository is vouched for,
   and ``grants.keys_online``, so that a reader can actually fetch the keys those signatures name.
   Files being vouched for is not the gate; **enough** of them being vouched for, by keys somebody
   else can download, is — a signature nobody outside can check convinces nobody, which is the whole
   purpose of publishing one.
