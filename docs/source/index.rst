=========
freeports
=========

**Structured data out of financial reports published as PDF.**

Funds disclose what they hold, what they are worth and how they classify themselves — in annual
reports, in PDF, laid out however each issuer chose. ``freeports`` reads those documents and writes
tables you can compute on. The engine knows nothing about any particular report: support for a
layout lives in a separately maintained **formats repository**, which is what lets coverage grow
without touching the engine.

.. important::

   Before relying on the output, read :doc:`what the project does and does not claim
   <overview/trust>`, and the :doc:`validation section <validation/index>` it summarises.

How this documentation is arranged
==================================

These pages are organised on **three axes at once**, because "where is that written down" has three
different answers depending on why you are asking.

.. list-table::
   :header-rows: 1
   :widths: 20 34 46

   * - Axis
     - Where it lives
     - What it is for
   * - **Who you are**
     - :doc:`guides/index`
     - one section per figure — user, engine developer, format author, input-database
       maintainer, granter, documentation writer, translator, DevOps, and the institutional
       reader who wants to know whether to believe any of it
   * - **What you must do or know**
     - inside each guide
     - a guide opens with its loop and keeps the deep material in its own ``advanced`` pages, so
       the first read is not the hardest one
   * - **What it is about**
     - :doc:`reference/index`
     - audience-independent: every option, every setting, the algorithm chapter by chapter. This
       is where the cross-references point, so nothing is written down twice

Above those sit two short sections everyone reads, and below them one that no human writes.

Start here
==========

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - If you want
     - Read
   * - to install it and see it run
     - :doc:`start/index` — installation, one real run, and the developer tools
   * - to understand what it is and why it exists
     - :doc:`overview/index` — the problem, how the engine works, the repositories, and what
       being trusted with the numbers means here
   * - to do a particular job
     - :doc:`guides/index` — pick the figure that is you
   * - to look one thing up
     - :doc:`reference/index` — options, settings, and the design of the algorithm
   * - to see what the machine says about this repository
     - :doc:`generated/index` — the two API references, the coverage of the grants, and the last
       gate run

.. toctree::
   :maxdepth: 2
   :caption: Getting started
   :hidden:

   start/index
   overview/index

.. toctree::
   :maxdepth: 3
   :caption: Guides
   :hidden:

   guides/index

.. toctree::
   :maxdepth: 2
   :caption: Reference
   :hidden:

   reference/index

.. toctree::
   :maxdepth: 2
   :caption: Generated
   :hidden:

   generated/index

.. note::

   This project is under active development.
