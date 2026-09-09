==================================
How this documentation is laid out
==================================

A reader arrives with one of three questions, and they are not the same question:

* *I am a person of a certain kind — where is my part?*
* *I have a job to do — what are the steps, and how deep do I need to go?*
* *I need one fact — where is it written down?*

A site organised on one of those serves the other two badly. So this one is organised on **all
three at once**, and a page's address is the answer to "which axis is this on".

The three axes
==============

.. list-table::
   :header-rows: 1
   :widths: 22 24 54

   * - Axis
     - Lives in
     - Rule
   * - **Audience** — who you are
     - ``guides/<figure>/``
     - one subtree per figure. A guide is written *to somebody*, in the second person, about the
       repository they are actually in
   * - **Task and level** — what you must do or know
     - inside each guide
     - the guide's own pages are the loop, in the order you meet it; anything that is depth rather
       than sequence moves down into ``advanced/``
   * - **Topic** — what it is about
     - ``reference/``
     - audience-independent and complete. Every option, every setting, the algorithm chapter by
       chapter. **This is where cross-references point**

Above the three sit two short sections everyone reads — ``start/`` (install it, run it once, find
out which kind of contribution is yours) and ``overview/`` (what the project is, how a run works,
what the repositories are, what being trusted with the numbers means here). Below them sits
``generated/``, which no human writes.

Why the third axis exists
=========================

The reference column is what stops the audience column from becoming nine copies of the same
manual. A format author and a DevOps engineer both need to know what ``FREEPORTS_ARCHIVE`` does;
neither guide explains it. Both link to the one page that does.

So the rule is blunt: **a fact is written down once, on the topic axis, and pointed at from
everywhere else.** When you find yourself explaining something in a guide that is not about the
reader's job — that is about the *thing* rather than about *doing* — it belongs in ``reference/``
and the guide should link to it in a sentence.

The inverse holds too. A reference page must not tell you what you want; it says what each option
does and stops. "Which of these do I want" is the guide's job, and a reference page that starts
advising has drifted onto the wrong axis.

Where does a new page go?
=========================

Ask, in this order:

#. **Would somebody read this because of who they are?** Then it is a guide. Which figure — and if
   the answer is "several", it is probably reference material that several guides should link to.
#. **Is it sequence, or is it depth?** Sequence stays in the guide. Depth goes to that guide's
   ``advanced/``. The test is whether a first-time reader needs it to finish the loop: if not, it
   is depth.
#. **Is it about a thing rather than about doing something?** Then it is ``reference/``, whoever
   asked.
#. **Is it written by a command?** Then it is ``generated/``, and it must say which command, so a
   reader can re-run it rather than believe it.
#. **None of the above?** ``reference/misc`` exists precisely so that nothing has to be forced into
   a section it does not belong to. A page there is a page waiting for its section to be written,
   which is a normal state and not a failure.

Splitting a page that has grown
===============================

Long pages get split two ways, and the choice matters:

**By theme**, when the page has become two subjects — as ``tooling.md`` did, and became one
reference page per command.

**By level**, when the page has become one subject at two depths — the first read and the argument
behind it. The first stays; the second goes to ``advanced/``.

A page that is long because it is *thorough about one thing at one level* should stay long. Length
is not the problem; mixing subjects or mixing depths is.

Two rules that keep the side panel usable
=========================================

The side panel is how the site is explored, so **every page of the site is reachable from it, from
anywhere**, without having to arrive at a page first to discover it exists. The section headings
*within* a page appear only while you are on that page — otherwise the panel is four hundred rows
and navigating it is worse than searching. Two things are needed for that to keep working, and both
are easy to break by accident.

**A page title must make sense on its own.** In the panel it sits between titles from seventy other
pages, with none of the surrounding prose. ``The minimum``, ``Rules of this source`` and ``What it
costs`` were real headings here and told a reader nothing — the last one appeared three times, on
three different pages, meaning three different things. Name the subject, not the rhetorical role.
No two titles in the site should read identically.

**A page's hidden toctree goes at the top of the page, above the first section**, not at the bottom
where it reads more naturally in the source. A directive belongs to whatever section encloses it, so
a toctree at the end of the file makes every child page a child of the *last section* — which
misstates the hierarchy. The toctrees are ``:hidden:``, so moving them changes the rendered page not
at all.

Cross-references, and not repeating yourself
============================================

Use ``:doc:`` and ``{doc}`` rather than prose like "see the configuration page": a real reference
breaks the build when it rots, and a sentence does not.

Every index page carries a short table of *what each page below answers*, phrased as the question
the reader has rather than as the title of the page. That table is the thing people actually read;
write it before writing the pages.

What is written where, and by whom
==================================

**Prose** — everything under ``start/``, ``overview/``, ``guides/`` and ``reference/`` — is written
by hand. New prose is Markdown with `MyST <https://myst-parser.readthedocs.io/>`_; older pages are
reStructuredText and stay that way, with no campaign to convert them.

**The Python API** is generated by ``autosummary`` from the installed packages, so it cannot drift.
**The Rust API** is generated by ``cargo doc`` and published beside the site. **The grants coverage
and the CI report** are written by ``freeports-validate report`` and ``freeports-dev ci-report``
into files with markers in them. Nothing under ``generated/`` is edited by hand, and every page
there names the command that produced it.

**The methodology pages** under ``docs/source/validation/`` are neither of those: they are hashed
artefacts. See the warning on :doc:`index`.
