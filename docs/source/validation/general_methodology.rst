..
    WARNING:

    This file is the main file that describes the methodology for validating
    and checking the validation of the tests. This file is identified by its
    hash, so every change to it invalidates all the parts referring to that
    hash. Be careful with modifying this file: an incorrect update can result
    in the impossibility for the user to trust the output of the software and
    for the developers to grant its correct functioning. So before modifying
    it, it is appropriate to understand the validation mechanism.


===================
General methodology
===================

A repository has one directory dedicated to the tests, called ``tests``, and one dedicated to
accountability, called ``validation``. This page describes what the second one contains, what the
entries in it mean, and how a reader can reconstruct any claim made there.

We develop tests to establish that the program does what we say it does. Some of what we want to
say, though, is not a thing a test can establish — that a person looked at a result and found it
reasonable, that somebody read a statement and agrees with it — and those claims need a protocol
rather than an assertion. This page is the protocol for making such claims, and it is the text every
one of them is made under.

There are two kinds of thing we grant the content of:

1. **test results** — under the ``tests`` directory: files of type

   * ``.csv``
   * ``.yaml``
   * ``.json``
   * ``.png``
   * ``.pdf``

2. **assertions** — statements published as prose in the documentation, for the freeports engine
   under ``docs/source/validation/assertions``: files of type

   * ``.rst``
   * ``.md``
   * ``.png``
   * ``.svg``

A file is granted through a specific **methodology**, which is a published document describing the
protocol that was applied to it.

*****************************
Where methodologies come from
*****************************

A grant is a claim made **under a text**, so the text has to be something both the person granting
and the person verifying can obtain. Methodology pages are therefore **published documents,
resolved from a location each of them configures**, called a *source*.

A source is a pattern containing exactly one ``*``, which stands for the methodology's relative
name::

    https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
    https://github.com/tvp-freeports/analysis_finance_reports/blob/main/docs/source/validation/*.rst
    file:///home/me/my-methodologies/*.rst

The general methodology — this page — is looked for at ``general_methodology``; a methodology named
``basic check`` is looked for at ``methodologies/basic_check``, its name lowercased with spaces
turned into underscores. Several sources may be configured, and their order is priority: a name is
resolved from the first that offers it.

.. important::

   **A validation document records a methodology's name and its hash, and deliberately not the
   source they came from.**

   The source is a contract between the person who grants and the person who verifies, and each of
   them writes it in their own configuration. Recording the granter's source inside the document
   would make the document assert something about the verifier's setup that it has no standing to
   assert, and would turn a shared name into a URL that has to keep working for ever.

   The price of that decision is that a hash mismatch is ambiguous: the page may have been
   rewritten, or the two of you may be reading two different publications of the same methodology.
   ``freeports-validate sources`` prints what each name resolves to on the machine it is run on, so
   two people can compare one line each and see which of the two it was.

This also means the methodology pages do **not** travel inside the tool. Were they to, upgrading the
tool would silently change what every grant in every repository refers to, and the text a claim is
about must not be chosen by a package manager. The price of the arrangement is stated openly:
whoever publishes a methodology can invalidate the grants made under it, by editing the page.

********************
Validation documents
********************

The ``validation`` directory of a repository holds one ``.yaml`` document per contributor, named
after them — ``validation/jane_doe.yaml``. Each is a record of what that person vouches for, and it
is signed by them:

.. code-block:: yaml

    version: <hash_general_methodology>
    who:
      name: <complete_name>
      email: <email>
      pubkey_id: <id_public_key>
    methodologies:
      - name: <name_methodology>
        sha256: <methodology_hash>
      - name: <name_methodology>
        sha256: <methodology_hash>
    data:
      - methodology: <name_methodology>
        files:
          - path: <path_to_the_file>
            sha256: <file_hash>
          - path: <path_to_the_file>
            sha256: <file_hash>
      - methodology: <name_methodology>
        files:
          - path: <path_to_the_file>
            sha256: <file_hash>
    sign: <cryptographic signature of the document>

``version``
    Which version of the **general methodology** the document was written under — that is, of this
    page. It fixes how the entries below are to be interpreted. It is the ``SHA256`` of the source
    file of this page, resolved from the configured sources. If the content of this page changes,
    the hash changes with it and every document referring to the old one is invalidated.

``who``
    Who is accountable for the content of the document:

    * ``name`` — the complete name of the person who owns the document and is responsible for what
      it says; in particular, the owner of the cryptographic keys used to sign it;
    * ``email`` — their email address. It is the channel through which they can be notified of an
      inconsistency, and the link to a physical person: keys are published on the
      `OpenPGP key server <https://keys.openpgp.org/>`_, which requires email verification before a
      public key can be found by address;
    * ``pubkey_id`` — the fingerprint of the key pair used to sign the document.

``methodologies``
    Every methodology this document uses, each with:

    * ``name`` — the methodology's name, which is its published file name with ``_`` replaced by a
      space, lowercased, and without the ``.rst`` extension. The page published at
      ``methodologies/basic_check.rst`` is named ``basic check``.
    * ``sha256`` — the hash of the exact text the entry refers to. If the protocol described by that
      page is updated, the hash changes and every document referring to the old text is invalidated
      with it.

``data``
    What is actually vouched for: a list of methodologies, each with the files granted under it. The
    methodology named here must be one of the names in the ``methodologies`` section above.

    Each ``path`` is **relative to the root of the repository the document lives in** — for example
    ``tests/formats/ARCA-IT24/1/out/funds.csv``, or
    ``docs/source/validation/assertions/validation_algorithm_trustworthiness.rst``. The
    accompanying ``sha256`` is the hash of that file's content, which is what makes the grant a
    claim about *bytes* rather than about a name: if the file changes, the grant visibly no longer
    applies to what is on disk.

``sign``
    The signature, produced with the private half of the key pair identified in ``who`` over the
    rest of the document. What is signed is the document with the ``sign`` field removed and every
    mapping key sorted recursively, so that two people writing the same content produce the same
    bytes to sign.

.. tip::

    You can produce the exact bytes that are signed — normalised, and without the signature —
    with:

    .. code-block:: console

        $ yq -y -S 'del(.sign)' <yaml-document-path>

    ``-S`` is **jq's** own recursive key sort. This project uses the Python ``yq`` (kislyuk), which
    is a thin wrapper around ``jq``; the unrelated Go program of the same name has its own
    expression language and does not emit identical bytes.

*******************************************
What a methodology may declare about itself
*******************************************

Two things on a methodology page are read by the tool rather than only by people. Both are optional,
and both exist to make a grant more legible rather than to constrain it.

Which paths it covers
=====================

A methodology page may carry an ordinary, visible section whose title is ``Supported paths``,
declaring the repository paths that methodology applies to. Each entry is a single inline literal
with a prose gloss underneath:

.. code-block:: rst

    Supported paths
    ===============

    ``tests/formats/**/out/*.csv``
        The reference output of a format's test suite in a formats repository. A basic check here
        means the run completed, the columns are the expected ones, and a human has looked at the
        values for obvious nonsense.

The gloss is the substance. The same path can mean different things in two repositories — a
``tests/formats/`` directory exists both in a formats repository and in the extraction engine's own
— and that ambiguity is resolved by an author writing down which one they mean, never by the tool
guessing.

The pattern grammar is four tokens, and everything else is literal:

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Token
     - Matches
   * - ``*``
     - any part of one path segment; never crosses a ``/``
   * - ``**``
     - zero or more whole segments
   * - a trailing ``/``
     - the directory and everything under it — the same as appending ``**``
   * - anything else
     - itself

There are no character classes, braces, negation or ``?``. Patterns are always relative to the
repository root.

**A page with no such section covers any path.** That is the behaviour from before this convention
existed, and it remains the honest answer for a methodology whose scope cannot be written as a set
of paths.

Where a grant names a path outside the declared set, the consequence differs by moment, and the
asymmetry is deliberate:

.. list-table::
   :header-rows: 1
   :widths: 25 75

   * - Moment
     - What happens
   * - granting
     - refused, showing every declared pattern with its gloss; overridable by the person granting
   * - checking
     - a warning, never a failure
   * - the lookups
     - the grant is listed, and marked as outside the declared set

Refusing at grant time is cheap and catches the mistake while its author is present. Failing at
check time would break a repository because somebody else edited a page it adopts, and no
methodology author should have that power over other people's repositories.

What it pins
============

A methodology page is prose, and prose cites things — a diagram, a specification, another document.
A grant made under the page is in part a claim about what those things said, so the page may pin
each of them by hash, in an ordinary reStructuredText comment:

.. code-block:: rst

    .. image:: assets/pipeline.svg

    .. sha256: assets/pipeline.svg 3f2a1b...c9

``..`` followed by text that is not a directive is a comment in every reStructuredText parser: it
renders as nothing and breaks no build. The target is either an absolute URI or a path relative to
the page itself.

**These pins are not a second thing to trust.** They are lines of the page, so the page's own
``sha256`` — the one a validation document records — already commits to every one of them. Following
them is a *deepening* of a check, answering the transitive question of whether the things a page
relies on still say what its author read.

***********************
Utilities for the users
***********************

Three commands answer the same question from the three directions it can be asked:

* ``freeports-validate who-grants <file>`` — who vouches for this file
* ``freeports-validate granted-by <contributor>`` — what this person vouches for, named by their
  complete name, their email or their key fingerprint
* ``freeports-validate granted-with <methodology>`` — what has been vouched for under this
  methodology, named as the ``name`` entries in a document's ``methodologies`` section are

They output, respectively:

* the contributors that grant that file — grouped by contributor *(default)*, or by methodology
* the files granted by that contributor — grouped by methodology *(default)*, or by file
* the files covered by that methodology — grouped by file *(default)*, or by contributor

The grouping is chosen with ``-f`` by file, ``-c`` by contributor, ``-m`` by methodology.

**How they work.** Each reads every validation document in the repository's ``validation``
directory, and for each one:

1. validates the document against its schema, and its signature against your keyring;
2. resolves every adopted methodology from your configured sources and compares hashes;
3. compares the recorded hash of each granted file against the file as it is now;
4. reports only what survived all three, annotating any grant that lies outside the paths its
   methodology declares.

None of the three needs a signing key of your own. Reading what other people have vouched for is not
an act done in anybody's name.

.. code-block:: console

    $ freeports-validate who-grants tests/formats/ARCA-IT24/1/out/funds.csv
    $ freeports-validate granted-by "Jane Doe"
    $ freeports-validate granted-with "basic check" -c

****************************
Utilities for the developers
****************************

* ``create-document`` — first-time setup: creates your personal validation document
* ``grant <files> [with <methodology>]`` — vouch for files under a methodology
* ``ungrant <files> [with {any|<methodology>}]`` — withdraw
* ``check-grants [<document>]`` — verify: schema, signature, version, methodology hashes, file
  hashes
* ``update <subcommand>``

  * ``file`` — restate an existing grant after the file legitimately changed
  * ``version`` — after this page changed
  * ``methodology`` — after a methodology page changed

* ``sign-document`` — sign the document after changes
* ``sources`` — what your configuration resolves, and where from
* ``check-methodology <name>`` — one page in detail: where it came from, and what it pins
* ``report`` — what the repository as a whole has been vouched for

**Signing.** ``sign-document`` validates the document against its schema, refuses to overwrite an
existing signature without ``--update``, normalises the document as described above, produces a GPG
detached signature with your private key, and embeds it in the ``sign`` field.

**Verification.** Every command that reads a document verifies its signature against your keyring
before believing anything in it. A signature that fails to verify is far more often a public key
missing from your keyring than a document somebody tampered with — import it and try again before
concluding anything.

.. caution::

   ``update`` is not the way to silence a failing check. It restates an intention to vouch, and it
   should be run by the person who has confirmed that the change was expected. Running it because a
   check went red converts a real signal into a signature.

*******************************
What this mechanism does not do
*******************************

It does not establish that the extraction is correct. It establishes that a claim was made: by a
named person, under a published protocol, about an exact sequence of bytes, and that the claim can
still be checked. Where that is not enough for your purpose, the honest answer is that it is not
enough — and the methodology pages are written to let you determine that for yourself rather than to
reassure you.
