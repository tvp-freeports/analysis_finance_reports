===================
Suites and verdicts
===================

The suites
----------

A metric is a number with a minimum. A suite is a set of tests with an outcome. Both are facts about
one commit, both are recorded into ``reports/`` with the commit they were established at, and both
are judged by ``ci-check``.

.. list-table::
   :header-rows: 1
   :widths: 22 8 22 48

   * - Suite
     - Cost
     - Repositories
     - Run by
   * - ``rust.unit``
     - fast
     - engine
     - ``make test-rust-unit``
   * - ``python.fast``
     - fast
     - engine
     - ``make test-python``
   * - ``formats.single_page``
     - fast
     - formats
     - ``freeports-dev test -- -m 'not integration_tests'``
   * - ``rust.integration``
     - **slow**
     - engine
     - ``make test-rust-integration``
   * - ``rust.doc``
     - **slow**
     - engine
     - ``make test-rust-doc``
   * - ``python.slow``
     - **slow**
     - engine
     - ``make test-python-slow``
   * - ``python.online``
     - **slow**, online
     - engine
     - ``make test-python-online``
   * - ``formats.integration``
     - **slow**
     - formats
     - ``freeports-dev test -- -m integration_tests``

Every row carries the command that runs it, and every message about a suite quotes it. A verdict
that says something is stale without saying how to un-stale it is a verdict people route around.

Why a suite is recorded rather than announced
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The outcome used to be a string on a command line — ``ci-check --suite fast:passed``. That says what
has just happened, and it is **silent about everything that has not**. A gate that runs two of six
suites and reports "tests: ok" is worse than one that reports nothing, because a reader believes it.

So each suite writes ``reports/suite-<name>.json``:

.. code-block:: json

    {"suite": "rust.unit", "outcome": "passed", "head": "3f2a…", "detail": {}}

and ``ci-check`` reads the lot. The commit is the point of the file. Without it there is no telling
a suite that passed on this code from one that passed a fortnight ago, and the fast/slow split would
quietly become a way of never running the slow half.

.. list-table::
   :header-rows: 1
   :widths: 18 44 19 19

   * - Verdict
     - Meaning
     - ``dev``
     - ``prod``
   * - ``ran``
     - ran at this commit, everything passed
     - —
     - —
   * - ``FAILED``
     - ran at this commit, something did not pass
     - notify
     - **refuse**
   * - ``STALE``
     - last ran at another commit
     - notify
     - **refuse**
   * - ``NOT RUN``
     - never run in this repository
     - notify
     - **refuse**

**A suite nobody ran is not a suite that passed**, which is the same rule ``unmeasured`` follows
among the metrics, applied to the other half of what a commit is judged on.

What marks a test slow, and why it is derived
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Never by hand. In both tooling packages a test is marked ``slow`` by the *fixtures it asks for*:
request one that starts a process or a server, and the marking follows. A test written next year
that bootstraps a repository is marked by the act of asking for the fixture that bootstraps it, and
one that stops doing so stops being marked. Three hundred hand-written ``@pytest.mark.slow``
decorators would be three hundred chances to forget.

The seam is **"starts anything"**, and getting it wrong is expensive in a way that is easy to miss.
In ``freeports_validate`` the line used to be drawn at "starts *the command*", which left twenty
tests that start a threaded HTTP server and forty that start a ``bash`` or a Python interpreter on
the fast side of it — three and a half seconds, most of a commit gate's entire budget, spent on
``fork``. In ``freeports_dev`` the same seam catches the two fixtures that bootstrap a whole
repository, each of which is twenty-odd process starts.

.. tip::

   Before moving a test across the line, check that it is really the test that is slow. **The three
   worst offenders in this repository were not tests at all**, and between them they were most of
   the suite:

   * **A fixture rebuilt for every test.** The expensive fixtures in ``freeports_validate``'s suite
     raise their state by running the command three to five times — ``create-document``,
     ``sign-document``, a couple of ``grant``\ s — and were function-scoped, so a class of five
     tests paid for all of it five times. **327 of the slow suite's 470 seconds were** ``setup``,
     and not one of them was a test. What those 4.4 seconds produce is seven files and one
     kilobyte, which ``copytree`` copies in 0.42 ms — four orders of magnitude. See ``Prototypes``
     in ``packages/freeports_validate/tests/conftest.py``: the state is built once per session and
     each test gets a copy, so isolation is exactly what it was.
   * **A half-second poll interval, paid per test.** ``ThreadingHTTPServer.shutdown()`` waits for
     ``serve_forever``'s loop to look between ``select`` timeouts, and the default is 0.5 s — paid
     in full at every teardown. Twenty tests, ten seconds. ``poll_interval=0.01`` took it to the
     noise floor.
   * **A plugin importing the world.** ``freeports_dev``'s ``pytest11`` entry point imported pandas,
     PyMuPDF and the engine at module level, so *every pytest process on the machine* paid three
     quarters of a second for libraries it would never touch — including the two tooling suites,
     which collect no format tests at all. The imports now happen where they are used.

   Deferring a test is a decision with a cost: it is a test that stops running at every commit.
   Deleting a wasted second costs nothing, and the second is usually there. Between them these
   three took the slow suite from **470 s to 162 s** and the fast gate from 5.5 s to 1.8 s, without
   a single test being deleted, deferred or weakened.

   The tool for finding them is ``pytest --durations=0 --durations-min=0``, split by ``setup`` /
   ``call`` / ``teardown``: setup that dominates call is a fixture doing per-test work that is not
   per-test. Where the cost is outside Python — this command shells out to ``yq``, ``gpg`` and
   ``check-jsonschema`` — putting a timing wrapper for each of them first on ``PATH`` and summing
   by program says in one run where the seconds went.


Verdicts
--------

.. list-table::
   :header-rows: 1
   :widths: 20 44 18 18

   * - Verdict
     - Meaning
     - ``dev``
     - ``prod``
   * - ``pass``
     - measured, at or above the minimum
     - —
     - —
   * - ``below``
     - measured, under it
     - notify
     - **refuse**
   * - ``unmeasured``
     - the tool is missing, or the run failed
     - notify
     - **refuse**
   * - ``stale``
     - measured at a commit other than HEAD
     - notify
     - **refuse**
   * - ``no threshold``
     - nothing configured
     - listed
     - listed

Why ``unmeasured`` refuses
~~~~~~~~~~~~~~~~~~~~~~~~~~

**A figure nobody could compute is not a figure above the threshold.** Reporting it as a pass is
exactly the failure this project already refuses for a methodology page it could not reach:
unverifiable is not granted, and unmeasured is not passing. The message names the missing tool and
the command that installs it — ``make dev-ci`` supplies all four — because a refusal you cannot act
on is a refusal you will route around.

.. _staleness:

Why ``stale`` refuses
~~~~~~~~~~~~~~~~~~~~~

Staleness is what makes the fast/slow split honest. The hook measures the fast metrics now and
reads the slow ones from the last full run; without the commit recorded beside each number there
would be no way to tell a fresh figure from one taken three weeks ago, and the split would quietly
become a way of never measuring the slow half at all.

**It is the mechanism that lets the gate be cheap without being dishonest.** A gate may run a
subset — as long as it says, in the same table, exactly which subset. Take staleness away and the
fast gate stops being a smaller gate and becomes a smaller *claim about the same thing*, which is
the failure this whole arrangement exists to avoid.

On ``dev`` it is a notification. On ``prod`` it refuses — and the ``prod`` hook runs ``make
ci-full`` first, so by the time the verdict is read there is nothing left to be stale. That is the
inversion the two branch classes are for: on a working branch you keep the loop tight and accept
that some things are unknown, and on a production branch you take the time and nothing is.
``--skip-slow`` exists for the person who knows better; it is never the default, and it prints what
it skipped.

Exit statuses: ``0`` pass or dev, ``1`` refused, ``2`` the configuration itself cannot be obeyed.
