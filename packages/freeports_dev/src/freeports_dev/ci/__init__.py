"""The measurements, the thresholds and the gate -- what a commit to this repository must clear.

Three layers, and nothing in one layer knows the next:

* **measurement** -- each command answers one question and writes one small normalised JSON file
  into ``reports/``. None of them decides anything;
* **the gate** -- :mod:`freeports_dev.ci.gate` reads those files, ``ci.yaml`` and the class of the
  current branch, prints one table and chooses an exit status;
* **the wiring** -- ``make`` targets in the engine repository and a thin hook in each of the others.

It is the split ``freeports-validate`` already follows -- ``collect`` establishes facts, ``report``
renders them, ``check-grants`` judges them -- and it is what keeps every figure reproducible by
hand, which is the only way a threshold survives its first failure.
"""
