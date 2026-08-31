========
Rust API
========

The engine is a Rust crate; its API is documented by **rustdoc**, generated from the crate's own
doc-comments. It is published alongside this site rather than transcribed into ``.rst``: a
hand-written copy of the Rust API would go stale within a week.

`Open the rustdoc for the freeports crate <rustdoc/freeports/index.html>`_

.. note::

   The link resolves only in a published build, where ``cargo doc`` output has been copied into
   the site. To produce it locally, run ``make rustdoc`` in ``docs/`` before ``make html``, or
   read it directly with ``cargo doc --open`` from ``packages/freeports``.

Who needs which
===============

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - If you are
     - Read
   * - developing the crate
     - rustdoc, module by module
   * - writing a format
     - the Python API in :doc:`API`, plus the format guides
   * - evaluating the project
     - the prose sections: usage, design choices, and :doc:`validation <validation/index>`
