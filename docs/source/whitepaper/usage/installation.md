# Installation

## What you are installing

Three distributions, with distinct jobs. Only the first is needed to extract data.

| Distribution | Gives you | Who needs it |
|---|---|---|
| `freeports` | the extraction engine: the `freeports` command and the Python module of the same name | everyone |
| `freeports-dev` | the `freeports-dev` command and a pytest plugin: inspecting pages, generating fixtures, running a formats repository's tests | format authors |
| `freeports-validate` | the `freeports-validate` command: methodology grants and their verification | format authors, and anyone auditing a repository |

The engine is a **Rust crate that is also a Python extension module**. `import freeports` from
Python imports the compiled extension itself — there is no Python source tree underneath it — and
the `freeports` command is a native binary built from the same crate. This matters for installation
only in that building from source needs a Rust toolchain as well as Python.

## Prerequisites

| | Needed by | Note |
|---|---|---|
| Python ≥ 3.10 | everything | the two tooling packages accept 3.8, but the engine sets the floor |
| a Rust toolchain | building from source | `rustup` with the stable channel is enough |
| [PyMuPDF](https://pypi.org/project/PyMuPDF/) | the engine | pulled in as a dependency; the one external library the engine cannot do without |
| GnuPG and `jq` | `freeports-validate` only | its Python dependencies come with it; these two must come from your system — see {doc}`../formats/tooling` |

## From source

Work in a virtual environment. The engine is a compiled extension, and mixing it with a system
Python is the fastest way to an import error nobody can reproduce.

```console
$ git clone https://github.com/tvp-freeports/analysis_finance_reports.git
$ cd analysis_finance_reports
$ python3 -m venv venv/freeports
$ source venv/freeports/bin/activate
$ pip install --upgrade pip maturin
```

Then the engine, built by [maturin](https://www.maturin.rs/) because of the Rust extension:

```console
$ pip install packages/freeports
```

For an editable install while working on the crate itself, use maturin directly. `maturin develop`
rebuilds the extension in place, and `--release` is worth the extra compile time for anything but
the smallest report:

```console
$ cd packages/freeports
$ maturin develop --release
```

The two tooling packages are plain setuptools projects:

```console
$ pip install packages/freeports_dev packages/freeports_validate
```

### The `freeports` binary

`maturin develop` builds the *extension module*, not the command-line binary: the two are separate
build products of one crate. Build the binary with cargo and put it on your `PATH`:

```console
$ cd packages/freeports
$ cargo build --release
$ install -m 755 target/release/freeports ~/.local/bin/freeports
```

Everything the binary does is also reachable from Python through the same crate, so this step is
optional if you drive the engine as a library.

## Checking the installation

```console
$ freeports --help
$ python -c "import freeports; print(freeports.__doc__.splitlines()[0])"
```

Both should answer. If the second fails while the first works, the binary is on your `PATH` but the
extension is not in *this* interpreter's environment — almost always a virtual environment that is
not the one the install went into.

A working installation is not yet a working run: two inputs must exist first, and neither ships
with the engine. See {doc}`inputs`.
