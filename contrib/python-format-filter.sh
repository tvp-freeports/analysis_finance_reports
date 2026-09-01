#!/bin/sh
#
# Git `clean` filter for the Python sources: formats what enters the index with `ruff format`, so
# that formatting is never something a review has to discuss. It is installed by `.gitconfig`,
# which `make githooks` links into the repository.
#
# Why a script and not the command line directly: a filter that fails makes *any* git operation
# touching that file fail — `git diff` and `git status` included — and `ruff` lives in the
# development environment, not on the system PATH. Without this script, a `git diff` with the venv
# deactivated ends in "external filter failed", which has nothing to do with what the user was
# asking for.
#
# The lookup has two stages, and the giving up is explicit:
#   1. `ruff` on the PATH (the environment is active: the commit case);
#   2. `ruff` in the repository's venv (the environment exists but is not active);
#   3. neither: the content is passed through untouched, with a warning on stderr.
#
# The third case is not a hole in the formatting: the `.githooks/pre-commit` hook refuses commits
# outside the `freeports-dev` environment, where ruff is present by construction. What the fallback
# saves are the read-only operations, for which formatting is irrelevant.

set -eu

filename="${1:?usage: python-format-filter.sh <file-name>}"

repo_root=$(git rev-parse --show-toplevel 2>/dev/null || echo .)
venv_ruff="${repo_root}/venv/freeports-dev/bin/ruff"

if command -v ruff >/dev/null 2>&1; then
    exec ruff format --force-exclude --stdin-filename "$filename" -
elif [ -x "$venv_ruff" ]; then
    exec "$venv_ruff" format --force-exclude --stdin-filename "$filename" -
else
    echo "python-format-filter: ruff not found, $filename passes through unformatted" >&2
    exec cat
fi
