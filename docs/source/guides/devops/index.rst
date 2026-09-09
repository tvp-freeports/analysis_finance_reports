===========================
Running the checks (DevOps)
===========================

Every commit to a freeports repository is measured, and some commits are refused. This section is
for whoever owns that machinery: what is measured, what refuses, and how to reproduce every figure
by hand — because a threshold whose number you cannot check yourself is a threshold you will switch
off the first time it is wrong.

There is no Jenkins at the moment. Everything a pipeline would do runs here, as ``make`` targets,
and ``make ci`` is the answer to "what did the pipeline run".

.. toctree::
   :maxdepth: 1
   :hidden:

   fast-and-slow
   the-gate
   ci-yaml
   suites-and-verdicts
   fingerprints
   reporting

What each page here answers
===========================

.. list-table::
   :header-rows: 1
   :widths: 32 68

   * - Page
     - Answers
   * - :doc:`fast-and-slow`
     - the one split everything else rests on, and why a gate that costs a minute gates nothing
   * - :doc:`the-gate`
     - what runs at every commit, and how the three branch classes differ
   * - :doc:`ci-yaml`
     - the thresholds file: the three tiers, the metrics, what each one measures and what it does not
   * - :doc:`suites-and-verdicts`
     - suites as opposed to metrics, and why ``unmeasured`` and ``stale`` refuse
   * - :doc:`fingerprints`
     - the hash a repository declares of itself, and which version component each one moves
   * - :doc:`reporting`
     - writing the verdict down, and what Jenkins will add when it is turned back on

The figures from the last run are not here: they are a *report about this repository* rather than a
page about operating it, and they live with the other machine-written pages under
:doc:`../../dev/ci-report/index`.

Three layers, and nothing in one knows the next
===============================================

**Measurement.** Each command answers one question and writes one small JSON file into
``reports/``. None of them decides anything.

**The gate.** ``freeports-dev ci-check`` reads those files, ``ci.yaml``, and the class of the
branch you are on. It prints one table and chooses an exit status. It measures nothing.

**The wiring.** ``make`` targets, and a thin ``pre-commit`` hook that names one of them. **Every
freeports repository has both** — this one, a formats repository, an input database — with the same
target names and the same two gates, ``ci-fast`` and ``ci-full``. What differs is how much there is
to measure: see :doc:`the-gate` for a format repository's smaller surface.

This is the split ``freeports-validate`` already follows — ``collect`` establishes facts, ``report``
renders them, ``check-grants`` judges them. It is what keeps every figure reproducible: when a
commit is refused, the number that refused it is sitting in a file, written by a command you can
re-run.
