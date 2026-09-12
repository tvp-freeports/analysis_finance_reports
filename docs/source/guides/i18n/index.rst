============================
Contributing a translation
============================

This section is for translating the documentation and the engine's user-facing messages. It needs
no Rust and no Python: the unit of work is a ``.po`` file and a language you know well.

Both the documentation and the engine's user-facing messages use `GNU gettext
<https://en.wikipedia.org/wiki/Gettext>`_. Translators work on ``.po`` files, which are compiled
into the binary ``.mo`` catalogues that are actually read at run time.

Translating the documentation
=============================

The catalogues live in ``docs/source/locales/<lang>/LC_MESSAGES/``, one ``.po`` per source page,
where ``<lang>`` is an `ISO 639 <https://en.wikipedia.org/wiki/ISO_639>`_ code. Edit them with a
text editor or a dedicated tool such as `Poedit <https://poedit.net/>`_.

There is no ``en`` catalogue, and its absence is not an omission: English is the source language,
so every one of its entries would be a string translated into itself.

The loop, from the repository root:

.. code-block:: console

    make i18n-extract   # extract the translatable strings from the current sources
    make i18n-update    # merge the extracted strings into the .po files
    make i18n-prune     # drop the catalogues nobody should be translating
    make i18n-stat      # how far each language has got, page by page
    make i18n-build     # compile .po into .mo

``make i18n`` runs extract, update and build in a row — the three steps that are always right.
``i18n-prune`` and ``i18n-stat`` are deliberately outside it: one deletes files and the other only
reports, and neither belongs in a target you run without reading the output.

Then build and read one language:

.. code-block:: console

    make docs-lang DOCLANG=it
    make docs-serve DOCLANG=it

``docs-lang`` recompiles the catalogues first. Sphinx reads the ``.mo`` files and never the ``.po``
a translator edits, so building a language without recompiling produces the *English* page for every
string edited since the last compile — silently, with no error to notice.

.. warning::

   ``make i18n-update`` rewrites every catalogue against the *current* sources. After the prose has
   been rewritten it touches several hundred files at once — new ``.po`` for pages that never had
   one, and obsolete entries marked in the rest. That is the correct outcome, but it is a large,
   deliberate commit of its own, not something to let ride along with an unrelated change.

Why ``i18n-prune`` exists
-------------------------

``sphinx-intl update`` merges today's strings into the catalogues and marks what has gone stale
*inside* a file. What it cannot do is notice a ``.po`` belonging to a page that no longer exists:
such a file is not stale, it is orphaned. Nothing points at it, ``i18n-stat`` still counts it, and a
translator can spend an afternoon on a page nobody will ever build. Reorganising the site produces
these by the dozen, which is why removing them is a step of the loop rather than an occasional
chore.

It compares the catalogues against ``docs/build/gettext``, so it has to come after
``i18n-extract``: only an extraction from the current sources can say which pages exist.

It also has to come **after** ``i18n-update``, which is the half of the order that is easy to get
wrong. ``sphinx-intl update`` creates a ``.po`` for every ``.pot`` it finds, so a prune run before it
has everything below undone one step later. The orphan half survives either order — an orphan has no
``.pot``, so nothing recreates it — which is precisely why the wrong order looks like it worked.

It also removes the catalogues of the pages **a command writes** — ``generated/`` and
``dev/ci-report/``, named in ``GENERATED_PAGES`` in ``mk/docs.mk``. Those pages are rewritten at
every run, so a translation of one would be stale before the commit translating it landed; and the
CI report in particular repeats the same cell text down a column, which Sphinx extracts as
**duplicate message definitions**. ``sphinx-intl`` tolerates those, but GNU ``msgfmt`` refuses such
a file outright and so does Poedit — so leaving them in the tree hands a translator three files
their tools will not open, describing pages nobody wants translated. With them gone,
``make i18n-stat`` counts only what a person can actually translate.

What is translated so far
-------------------------

``it`` covers **all of the prose**: every page a person wrote, from the front page through the
guides and the reference to the validation section. What is deliberately left in English is the
material no person writes — the two generated API references under ``generated/`` and the CI report
under ``dev/ci-report/`` — because those pages are rewritten by a command at every run, and a
translation of them would be stale before it was committed. Technical names are left in English
throughout: an option is ``--target-list`` in every language, and a reader who translates it back to
type it has been misled.

``fr`` and ``pt`` are scaffolding with nothing translated in them yet. Which languages are carried
forward has not been decided; the scaffolding is maintained so that the decision stays available.

A partly translated language is the normal state of a translated site, not a broken one: gettext
falls back **per string**, so an untranslated paragraph appears in English inside an otherwise
translated page. ``make i18n-stat`` is how you see where a language actually stands.

Translating the engine's messages
=================================

The engine looks up its user-facing strings through a gettext catalogue at run time, falling back
to the message id itself when there is no translation — a missing translation is a cosmetic
problem, and it must never be able to stop a run.

.. warning::

   The catalogue tree the engine reads is **not currently in this repository**: what the crate holds
   is the lookup code and one ``.mo`` fixture used by its tests. Packaging and shipping catalogues
   for the engine is unfinished work, and this page will describe the workflow once there is one to
   describe.
