========================
Writing the verdict down
========================

``ci-check`` prints a table on a terminal and nothing keeps it. ``freeports-dev ci-report`` renders
the *same run* into the shapes a reader meets outside a terminal, and ``make ci-report`` writes all
of them at once — which is what the commit hook runs, staging what it rewrote, so the published
figures and the commit that changed them arrive together.

.. list-table::
   :header-rows: 1
   :widths: 16 40 44

   * - ``--format``
     - What it is
     - Where this repository keeps it
   * - ``json``
     - The model. Every other rendering is a pure function of it, and a question none of them
       answers is a ``jq`` expression away.
     - Nowhere: it is written to standard output, or to a temporary file so one evaluation can be
       rendered several times.
   * - ``badges``
     - One shields-style SVG per metric, plus one for the run as a whole. Drawn locally, so
       generating them needs no network.
     - ``ci/report/badges/``
   * - ``markdown``
     - A block rewritten between two markers in a file somebody else wrote.
     - ``README.md``
   * - ``rst``
     - The same, for a documentation page. ``--table`` picks the arrangement.
     - :doc:`../../dev/ci-report/index`, and the two pages beside it
   * - ``html``
     - One self-contained page: the three arrangements as tabs, and a filter box.
     - ``docs/source/_extra/ci/report.html``, copied into the site root by ``html_extra_path``

All of it is published under :doc:`the continuous-integration report <../../dev/ci-report/index>`, beside the
validation coverage in *Trust and provenance* — because what the last run measured is a report about
this repository rather than a page about contributing to it. The single-page rendering is
`here <../ci/report.html>`_.

**This command never refuses anything.** Its exit status says whether it could write what it was
asked to write, and nothing about what it found — a hook that could be stopped by its own report is
a hook people remove. That is the rule the grants report already follows, and the reasons are the
same.

**A badge is coloured by the figure, not by the verdict** — with two exceptions. A figure under this
repository's own minimum is red whatever its value, because falling short of a rule the repository
wrote down is news; and one nobody could compute, or one taken at another commit, is grey, because
it is not a figure about this code at all. Everywhere else the colour is the absolute number:
painting 32 % green on the grounds that 32 is what this repository currently demands would flatter a
reader with the project's own floor, and a badge is read by people who have not opened ``ci.yaml``.

**The run as a whole gets one word**, and it is the vocabulary the grants report already uses:

.. list-table::
   :header-rows: 1
   :widths: 18 82

   * - Status
     - When
   * - ``passing``
     - every gated figure measured, current, and at or above its minimum
   * - ``failing``
     - a measured figure under its minimum, or a suite that did not pass
   * - ``inconclusive``
     - nothing below, but something unmeasured, something stale, or something a ``--skip-slow``
       run never looked at

``inconclusive`` is grey and is **not** a pass, for the reason ``unmeasured`` is not one: a figure
nobody could compute is not a figure above the threshold.

.. note::

   **No rendering carries a timestamp, and none names a commit unless a figure is stale.** These
   artefacts are committed — badges in a ``README.md``, tables in this tree — and a date, or the
   head recorded beside every figure, would put a diff in every commit saying nothing had changed.
   The commit is named in the one place where it is the news: beside a figure measured at another
   one.


What Jenkins will add
---------------------

Two things, and nothing else: publishing to PyPI on a tagged release (``make release``), and
building the documentation. Everything else it would run already has a name here, and ``make ci``
is that name.
