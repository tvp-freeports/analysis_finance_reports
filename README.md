[![Freeports logo](https://www.freeports.org/assets/logo/complete-dark.svg)](https://www.freeports.org)
# Finance reports analysis ([Freeports](https://www.freeports.org))
This project is intended parse finance pdf reports and create `CSV` dataset.
The purpose of the project and related infos can be found at the [*official website*](https://www.freeports.org).

## Installation
Two things are installed together and are easy to confuse: the `freeports` **command** and the
`freeports` **Python module**. They are two build products of one Rust crate, and neither implies
the other.

### From PyPI
```bash
pip install freeports
```

### From source
The repository's `Makefile` does the whole setup, virtual environment included:

```bash
git clone https://github.com/tvp-freeports/analysis_finance_reports.git
cd analysis_finance_reports
make install                  # the engine: extension and command
source venv/freeports-dev/bin/activate
make installcheck             # says nothing if both work
```

Building from source needs a Rust toolchain (`rustup`, stable channel) as well as Python ≥ 3.10.

One target per role, and `make help` lists the rest:

| You are | Run |
|---|---|
| using the engine | `make install` |
| working on the engine | `make dev-engine` |
| writing a PDF format | `make dev-formats` |
| writing documentation | `make dev-docs` |
| maintaining all of it | `make dev-all` |

`make doctor` reports what is installed, what is missing, and which target supplies it.

## Quickstart
```bash
freeports -h
```
shows the options; all of them can also be given as environment variables or in a configuration
file. To drive the engine as a library instead:

```python
import freeports
```

A working installation is not yet a working run: two inputs must exist first, and neither ships
with the engine — an **input database** of target companies and a **formats repository** with the
parsing definitions for the documents you have. See the
[full documentation](https://docs.freeports.org).

## Contributing
See [CONTRIBUTING.md](CONTRIBUTING.md), and the developer section of the
[documentation](https://docs.freeports.org) for the build system, the test conventions and the
translation workflow.
