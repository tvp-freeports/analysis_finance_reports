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
    make i18n-prune     # drop catalogues whose page no longer exists
    make i18n-update    # merge the extracted strings into the .po files
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

It compares the catalogues against ``docs/build/gettext``, so run it after ``i18n-extract`` and not
before: only an extraction from the current sources can say which pages exist.

What is translated so far
-------------------------

``it`` covers the pages a reader meets first — the front page, the whole of
:doc:`../../overview/index` and :doc:`../../start/index`, :doc:`../../guides/user/index`,
:doc:`../../guides/input-db/index`, :doc:`../../guides/institutional/index`, and the two section
indexes — which is around 780 strings. The contributor-facing material is not translated: the
format, engine, DevOps, grants and documentation guides, the CLI and configuration reference, the
design chapters and the validation section are all still English.

``fr`` and ``pt`` are scaffolding with nothing translated in them yet. Which languages are carried
forward has not been decided; the scaffolding is maintained so that the decision stays available.

A partly translated language is the normal state of a translated site, not a broken one: gettext
falls back **per string**, so an untranslated paragraph appears in English inside an otherwise
Italian page. ``make i18n-stat`` is how you see where a language actually stands.

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
