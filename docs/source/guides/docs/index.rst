=========================
Writing the documentation
=========================

This section is for whoever is adding to, correcting or reorganising these pages. It has two
halves, and they answer different questions: :doc:`organisation` says **where a page belongs and
why**, and :doc:`building-the-site` says **how to build and read the result**.

Read :doc:`organisation` first, even for a one-paragraph correction. The site is arranged on three
axes at once, and a page filed on the wrong one is worse than a page that does not exist: it makes
the reader believe they have found the answer.

.. toctree::
   :maxdepth: 1
   :hidden:

   organisation
   building-the-site

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Page
     - Answers
   * - :doc:`organisation`
     - the three axes, where a new page goes, and the rules that keep the side panel usable
   * - :doc:`building-the-site`
     - Sphinx, MyST, the two generated API references, and what ``make docs-serve`` is for

Translating what is here is a different job with a different toolchain, and it is
:doc:`../i18n/index`.

.. warning::

   The pages under ``docs/source/validation/`` are **content-addressed**: their SHA-256 hashes are
   recorded in signed validation documents, in this repository and in others, and their published
   URLs are the default source those documents resolve. Editing one — even fixing a typo — or moving
   it invalidates every grant that cites it. Changing them is a deliberate operation that includes
   re-granting and re-signing, not an act of tidying. See :doc:`../institutional/why-trust-a-grant`.
