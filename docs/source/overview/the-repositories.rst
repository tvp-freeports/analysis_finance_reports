==========================
The project's repositories
==========================

The project is **not** a monorepo. It is a set of repositories that depend on each other at run
time and are maintained independently, which is the same decision as keeping formats out of the
engine: coverage should be able to grow without anyone touching the part that must not change.

.. list-table::
   :header-rows: 1
   :widths: 30 40 30

   * - Repository
     - Holds
     - You change it to
   * - ``analysis_finance_reports``
     - the engine, ``freeports-dev``, ``freeports-validate``, this documentation
     - fix or extend the extraction machinery itself
   * - a **formats repository**
     - format definitions, their tests, their validation documents
     - add support for a report layout
   * - an **input database**
     - company lists, the evidence for recognising them, target lists
     - change *which* companies a run looks for
   * - the website
     - `freeports.org <https://www.freeports.org>`_
     - change the public site

The last three are not one repository each: anyone can create and maintain a formats repository or
an input database, and several exist. `analysis_finance_reports_formats
<https://github.com/tvp-freeports>`_ is the reference formats repository, and there is a public
input database beside it.

Only work on the first repository needs a Rust toolchain, and only it has a build to set up.
