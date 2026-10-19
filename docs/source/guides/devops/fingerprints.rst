===================================
Fingerprints and version discipline
===================================

Three repositories declare a hash of their own contents. Until this work, **nothing computed it** —
and a field nobody verifies is not a fingerprint, it is a comment that looks like one.

The recipe, once
----------------

.. code-block:: console

    printf '%s\n' <paths> | LC_ALL=C sort | xargs sha256sum | sha256sum

The paths relative to the repository root, sorted in byte order; the sha256 of each, in that order,
as ``"<hash>  <path>"`` lines; and the sha256 of that stream.

**The path is inside the hashed lines on purpose**: moving a file changes the fingerprint as much
as editing it. A grant naming ``tests/a.json`` does not cover the same repository once that file is
called ``tests/b.json``. The sort is ``LC_ALL=C`` so the answer does not depend on the committer's
locale — the one thing a fingerprint must never do is differ between two people looking at the same
tree.

Non-ASCII file names would need ``sha256sum``'s escaping rules pinned down to stay reproducible.
Both hooks already refuse them, so this says so rather than pretending to handle a case nobody can
currently create.

A formats repository
--------------------

Four fingerprints in ``package.yaml``. Three **identify** what produces data; one **certifies**:

.. list-table::
   :header-rows: 1
   :widths: 26 30 44

   * - Field
     - Covers
     - Proposal when it moves
   * - ``sha256.content``
     - ``content/``
     - **patch**
   * - ``sha256.metadata``
     - ``metadata/``
     - **patch**
   * - ``sha256.tests``
     - ``tests/formats/`` — the expected outputs
     - only files added: **minor**; any file changed or removed: **major**
   * - ``info.validation_sha256``
     - every granted file
     - **patch**

Several moving together propose the strongest of them. The expected outputs are taken as the
behavioural specification: a change to ``content/`` that leaves them identical is a patch, a new
test (a new format, a new page fixture) adds coverage without changing any earlier output, and an
edited or deleted expected output means a run no longer produces what it did.

Telling an addition from an edit needs the earlier per-file hashes, not only their hash, so the
recipe's intermediate stream is **saved in the repository** as ``tests/formats.sha256``. Its own
``sha256sum`` is ``sha256.tests``, and it checks by hand:

.. code-block:: console

    sha256sum -c tests/formats.sha256      # every expected output still what was recorded
    sha256sum tests/formats.sha256         # = sha256.tests

It is written only when the manifest is, under the same rule below. With no listing at ``HEAD`` to
compare with, the change cannot be classified and the bump is left to the author.

``validation_sha256`` reads the granted files from the repository's own ``validation/*.yaml`` —
not from ``freeports-validate collect``, because collecting resolves methodology pages over the
network and a fingerprint that cannot be computed on a train is one the hook has to skip.

**Cache directories are not part of a repository**: ``__pycache__`` and any hidden directory
(``.ruff_cache``) are skipped, in every repository kind, because their contents differ from machine
to machine. Hidden *files* are kept — a format test's ``.log.csv`` is an expected output.

On ``dev`` the hook warns and **does not write the new fingerprint**. Writing it would leave the
manifest claiming that version X covers content Y, which is a false statement — and the entire
point of the field is that it is not one.

An input database
-----------------

``sha256.companies`` and ``sha256.lists`` in ``metadata.yaml``, over their two directories, same
recipe. Here the bump is **mechanical**, so the hook proposes it:

.. list-table::
   :header-rows: 1
   :widths: 32 22 46

   * - What moved
     - Proposal
     - Why
   * - ``lists`` only
     - **minor + 1**
     - the typical list change *adds* companies, which adds rows and changes none; a fingerprint
       cannot tell an addition from a removal, so whoever removed companies raises it to major
   * - ``companies`` (± ``lists``)
     - **major + 1**
     - ``companies/`` decides what *matches*, so it changes the meaning of every run

In an input database the patch component is never proposed: it is what a change moving neither
fingerprint gets — a description, a README — and only a person knows they made one.

Interactive means interactive: the hook reopens the terminal and asks, showing both hashes and the
proposed version. **Declined, or unanswerable, is no.** A rebase, a script, an editor's commit
button and a CI runner all reach it with no terminal, and in each of those the person who would
have said yes is not there. A prompt nobody can answer must never be read as a yes.

By hand, in any of the three repository kinds:

.. code-block:: console

    freeports-dev fingerprint             # compare and say
    freeports-dev fingerprint --update    # rewrite the manifest when it is entitled to
    freeports-dev fingerprint --propose   # offer the mechanical bump
    freeports-dev fingerprint --format json


The targets
-----------

The quality surface is laid out on two axes: **``<what>`` alone is everything, ``<what>-<which>`` is
one slice.** You compose a name and it exists.

.. list-table::
   :header-rows: 1
   :widths: 20 20 20 20 20

   * - ``<what>`` ↓ / ``<which>`` →
     - (all)
     - ``-rust``
     - ``-python``
     - ``-formats``
   * - ``test``
     - the fast suites (``test-all`` for every one)
     - the crate
     - the tooling packages
     - a formats repository
   * - ``lint``
     - both linters
     - clippy
     - ruff
     - ruff over ``content/``
   * - ``fmt``
     - both
     - ``cargo fmt``
     - ``ruff format``
     - —
   * - ``coverage``
     - both **(slow)**
     - ``cargo llvm-cov`` **(slow)**
     - ``pytest --cov`` **(slow)**
     - the document inventory
   * - ``doc-coverage``
     - both
     - rustdoc
     - the docstring walker
     - —

And the aggregates:

.. code-block:: console

    make ci-fast     # the commit gate on a dev branch — seconds
    make ci-full     # the commit gate on a prod branch — everything, measured here
    make ci          # ci-full plus the documentation build — what a pipeline would run
    make ci-rust     # the whole Rust column
    make ci-python   # the whole Python column
    make ci-check    # the verdict over whatever is already in reports/
    make dev-ci      # install the measurement tools
    make doctor      # which of them are missing, and which target supplies each

And the suites, on the same two axes:

.. code-block:: console

    make test-fast   # what runs at every commit
    make test-slow   # the rest, run deliberately
    make test-all    # both — before you call a piece of work finished
    make validation  # the grants report, the integrity check and the key lookup

.. note::

   The ``test-*`` targets were renamed when this was built, and **no aliases were kept**:
   ``test-full`` is now ``test-rust``, ``test-tools`` is ``test-python``, and their variants
   follow. ``docs-coverage`` is ``docs-site-coverage``, because that name claimed something it does
   not measure.
