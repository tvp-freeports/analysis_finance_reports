==============================
Installing the developer tools
==============================

One command from a fresh clone, and then whichever role you are in:

.. code-block:: console

    git clone <url-of-your-fork>
    cd analysis_finance_reports
    make init                      # venv, git hooks, everything installed
    source venv/freeports-dev/bin/activate

``make init`` installs everything, which is the right default for a first look around. When you know
which part you are working on, the narrower targets are faster:

.. code-block:: console

    make dev-engine      # the crate: extension rebuilt in place, binary, tests, lint
    make dev-formats     # the engine plus freeports-dev and freeports-validate
    make dev-docs        # the above plus Sphinx and the translation tooling

You also need a Rust toolchain — ``rustup`` with the stable channel. The engine is a Rust crate, so
there is no way around it. ``make doctor`` tells you what is installed, what is missing and which
target supplies each gap; :doc:`../guides/engine/build` explains the whole arrangement, and
:doc:`install` gives the same steps as plain commands for an environment the
Makefile knows nothing about.
