# Contributing

Everyone interested in contributing is encouraged to do so. This file is the short version; the
full guide, with one section per kind of contribution, is
[How to contribute](https://docs.freeports.org/contribute.html) in the documentation.

> **Please read the [code of conduct](CODE_OF_CONDUCT.md) before any contribution.**

## Find your repository first

The project is not a monorepo. What you want to change decides where you work, and only the first
row is this repository:

| To change | Work in | Needs a Rust toolchain |
|---|---|---|
| the extraction engine or its tooling | this repository | yes |
| support for a report layout | a **formats repository** | no |
| which companies a run looks for | an **input database** | no |
| the documentation | this repository, under `docs/` | to build the Rust API, yes |
| the public site | the [website repository](https://github.com/tvp-freeports/analysis_finance_reports_website) | no |

Formats and input databases are plugins, maintained separately and by anyone: adding a format
requires no change to the engine, and it is not a pull request against this repository.

## Setting up this repository

```bash
git clone <url-of-your-fork>
cd analysis_finance_reports
make init                     # venv, git hooks, everything installed
source venv/freeports-dev/bin/activate
```

`make help` lists every target, and `make doctor` says what is installed, what is missing and which
target supplies each gap. The narrower setups are faster once you know what you are working on:

```bash
make dev-engine     # the crate: extension rebuilt in place, binary, tests, lint
make dev-formats    # the engine plus freeports-dev and freeports-validate
make dev-docs       # the above plus Sphinx and the translation tooling
```

You also need a Rust toolchain (`rustup`, stable channel). The engine is a Rust crate, so there is
no way around it.

## The day-to-day loop

```bash
make develop        # rebuild the extension in place after a Rust change
make test-unit      # the bulk of the coverage; fast
make check          # the full suite: unit, integration, doctests
make lint           # clippy on the crate, ruff on the Python sources
make docs           # the documentation site, rustdoc included
```

`make develop` builds the extension module and `make build` builds the binary: they are two build
products of one crate and neither implies the other. A stale `.so` is the usual explanation for a
Rust change that "had no effect" on the Python side.

## Before opening a pull request

```bash
make pre-commit
```

That is the same gate the commit hook fires — lint plus the full test suite. If you touched anything
the formats side depends on, add a real formats repository's tests:

```bash
make test-formats REPO=../analysis_finance_reports_formats
```

There is no CI building this repository at the moment, so this local gate is the only one there is.

## A few guidelines

- **Tests first**, written to exhaust branches rather than sample them, grouped by topic in nested
  modules inside `mod tests`.
- **Errors are typed**, one enum per module; a user path does not panic.
- **`api` is the public surface.** The rest of the tree is internal and free to move.
- **Do not change a formats repository to accommodate an engine change** — propose it. Those
  repositories have other maintainers, and their reference output is a specification.
- **Fix inherited bugs at the root, but ask first.** Where the old behaviour may be depended on, an
  opt-in parameter defaulting to it is usually the right shape.
- **The pages under `docs/source/validation/` are content-addressed.** Their hashes are recorded in
  signed documents; editing one invalidates every grant that cites it, so it is a deliberate
  operation, never tidying.

The full list, with the reasoning, is in
[How to contribute](https://docs.freeports.org/contribute.html).

All of this is our current workflow rather than a settled rulebook — feedback on what to improve or
change is itself a contribution.
