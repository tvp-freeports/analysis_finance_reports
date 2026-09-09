# Contributing

Everyone interested in contributing is encouraged to do so. This file is the short version; the
full guide is
[How to contribute](https://docs.freeports.org/en/latest/start/ways-to-contribute.html), and the
documentation beyond it is arranged as one section per contributor figure.

**Those figures are an organising device, not a description of who may do what.** A project is
shaped by the people who turn up, not the other way round: the sections exist because somebody has
already done that kind of work, and they are expected to grow to fit whoever arrives next. The list
is not exhaustive and its boundaries are soft. Contribute wherever you feel expert, or simply
wherever you want to — take part of one row below and part of another, or do something none of them
mentions. A contribution the documentation has no section for is a good sign about the project and
a gap in the documentation, not a reason to hold back.

> **Please read the [code of conduct](CODE_OF_CONDUCT.md) before any contribution.**

## Find your repository first

The project is not a monorepo. What you want to change decides where you work, and only the first
row is this repository. The table is a map, not a menu:

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
make test-rust-unit      # the bulk of the coverage; fast
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

That repository has the same targets as this one, with the axes it has no use for taken out, so
`make ci-fast` inside it is the same gate under the same name. `make help` there lists all of it.

There is no CI building this repository at the moment, so this local gate is the only one there is.

## A few guidelines

These are conventions rather than rules, and their purpose is consistency: they are what this
codebase settled on, written down so nobody has to guess. Where one makes your change worse, say
so — a convention with a missing exception is more useful pointed out than complied with.

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

The full list, with the reasoning — and the house style for Rust, Python and tests — is in
[Conventions and house style](https://docs.freeports.org/en/latest/guides/engine/conventions/index.html).

All of this is our current workflow rather than a settled rulebook — feedback on what to improve or
change is itself a contribution.
