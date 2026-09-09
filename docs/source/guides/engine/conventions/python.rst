====================
Python: house style
====================

Two packages are Python: ``freeports_dev`` (the format author's tooling and a pytest plugin) and
``freeports_validate`` (grants, which is mostly shell with a Python front end). The engine is
**not** — ``import freeports`` loads the compiled crate, and there is no Python source underneath.

``ruff`` both lints and formats: ``make lint-python`` and ``make fmt-python``. As with Rust, this
page is only what a formatter cannot decide.

Layout
======

``src/`` layout, not flat — ``packages/<name>/src/<name>/`` — so that what the tests import is the
working tree and not the copy ``pip install`` left in the environment. ``pythonpath = ["src"]`` in
each ``pyproject.toml`` is what enforces it, and the comment there says why: without it a change to
a module would be checked against the previous release of that module, which is the one way a suite
can pass while the code it names is broken.

A subpackage per area once an area has more than one file — ``freeports_dev/ci/`` holds
``config``, ``metrics``, ``gate``, ``report``, ``render``, ``suites``, ``fingerprint``. The rule is
the same as the crate's: the split is for readers, and it may change.

Docstrings
==========

Numpy style (``napoleon`` is configured for it), and a **module docstring on every module** that
does the same job as the Rust ``//!``: what this module is for, and which decision it encodes.

The house voice again matters more than the format. ``freeports_dev/ci/metrics.py`` is the model:
it opens by saying what a metric *is*, then spends three paragraphs on why cost is a property of
the metric rather than of the caller — which is the fact a reader needs and no signature carries.

Write the docstring that stops the next person reopening the argument. Do not write the one that
repeats the parameter names.

Errors
======

Module-level exception classes, one base per area, named for it — ``ConfigError`` and friends.
Raised with a message that names the value and the expectation, exactly as on the Rust side.

**Two things are errors rather than resolutions**, and this is a project-wide instinct worth
absorbing: an ambiguity that could be silently resolved usually should not be. A branch matching two
class lists in ``ci.yaml`` is an error, not a first-match-wins; an unknown key in a configuration
file is an error, not an ignored line; an unreachable methodology page is an error, not a pass. A
setting that is quietly ignored configures nothing and reports nothing, and the user is left
believing it took effect.

The shell half
==============

``freeports_validate`` is a set of shell scripts with a Python entry point, and that is deliberate:
it composes ``gpg``, ``jq``, ``curl`` and ``sha256sum``, and a signature has to be computed the same
way on everyone's machine or it cannot be verified on anyone else's.

Consequences worth knowing before editing them: the filters are plain ``jq`` rather than any
higher-level expression language, because the signature is computed over ``yq -y -S 'del(.sign)'``
and ``-S`` is jq's own recursive key sort. An "equivalent" rewrite that sorts differently silently
invalidates every signature. :doc:`../../../reference/cli/freeports-validate` has the detail.

Configuration
=============

Every setting in these tools is reachable through **all three tiers** — configuration file,
environment variable, command line — because that is what the engine does and a tool that supports
two of the three is a tool people work around. The precedence and the per-field merge are
:doc:`../../../reference/configuration/dev-and-validate`.
