============================
Tests, and TDD where it fits
============================

Tests come first **where the shape of the answer is known before the code is** — which is most
things here, and not all of them. Where you are exploring how a PDF actually behaves, write the
exploration, learn the answer, then write the test that pins it and delete the exploration. Calling
that "not TDD" and doing it anyway is more honest than pretending the test came first.

What is not negotiable is the end state: **the behaviour is pinned by a test that would fail if
somebody removed it.**

.. note::

   As everywhere in this section, the organisation below is a convention. It exists so that a
   reader can find the test for a behaviour without grepping, and so that two people adding tests
   to the same file do not produce two incompatible arrangements.

Rust tests
==========

**Grouped by topic in nested modules inside** ``mod tests``, never a flat list of ``#[test]``
functions::

    #[cfg(test)]
    mod tests {
        use super::*;

        mod construction {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;
            use DateError::*;

            #[test]
            fn accepts_an_ordinary_date() { … }

            #[test]
            fn refuses_a_thirteenth_month() { … }
        }

        mod ordering { … }
    }

The submodule names the behaviour under test; the function names the case, as a sentence, in the
present tense. ``accepts_an_ordinary_date`` and ``refuses_a_thirteenth_month`` tell a failing run
what broke. ``test_date_1`` does not.

``pretty_assertions`` for readable diffs and ``test_case`` for table-driven cases are already
dependencies; use them rather than hand-rolling either.

**Exhaust branches rather than sample them.** For a function with four error variants, four tests
that each provoke one, not one test that provokes whichever came to mind. The unit tests carry the
bulk of the coverage; integration tests exist for the interactions units cannot show.

Python tests
============

One ``tests/test_<module>.py`` per module, and a **module docstring that states the properties the
file holds** — not "tests for config.py". ``tests/test_ci_config.py`` opens by naming the two
properties that make ``ci.yaml`` trustworthy, and every test below is recognisably one of them.

Fixtures in ``conftest.py``, and an ``autouse`` fixture to scrub the ambient environment wherever a
test could otherwise be decided by a variable the developer happened to export. That is a real bug
this project has had.

Fast and slow
=============

**Only fast tests run at every commit.** A test that starts processes, bootstraps a repository or
touches the network is marked ``slow`` and deselected by default; ``addopts = "-m 'not slow'"``
does it, and ``make test-python-slow`` / ``make test-tools-slow`` run them deliberately and
**record that they did**, so a commit that has not seen them says so rather than looking as though
it has.

Note the classification: **anything needing the network is slow**, regardless of wall-clock cost.
A figure that depends on somebody else's host being up is not a figure about the commit being made,
and a gate you cannot clear on a train is a gate people will switch off.

Where the marker is derived from a fixture rather than written by hand, that is on purpose too —
``conftest.py`` says which fixtures and why. A marker somebody has to remember to add is a marker
that will be forgotten.

:doc:`../../devops/index` is the whole arrangement: which suites exist, what they cost, and what
happens to a commit that has not run one.

Reference output is a specification
===================================

A formats repository's ``tests/formats/<FORMAT>/out/**`` is not a snapshot to be refreshed when it
disagrees with a run. When a run diverges from it, the assumption is that the **engine** changed.

Regenerating one of those files is a deliberate act with a reason written down — never a way to
turn a red test green — and the same holds for the integration reference outputs in this
repository. If an engine change makes a formats repository fail, the change is proposed to that
repository's maintainers, not made on their behalf.
