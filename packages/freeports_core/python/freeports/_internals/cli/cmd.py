"""Contains all functions related to command line use of the `freeport` script.

``cmd()`` (the `[project.scripts]` entry point `pyproject.toml` installs as the `freeports`
command) now execs the native Rust binary (Fase E, punto 6 — see
``analysis_finance_reports/agent-memory/rust-native-binary-plan.md``) instead of parsing argv and
orchestrating in Python — the whole CLI/config-resolution/execution chain this module used to
drive (`conf_parse.py`, `main.py`) is now `packages/freeports_engine/src/cli/` (config parsing,
plus the binary's own job/batch/output modules) and its `src/main.rs` entry point.
The original Python body (renamed ``_legacy_cmd``) was moved to
``reference_legacy/_internals/cli/cmd.py`` during the maturin-idiomatic restructure (see
`agent-memory/maturin-idiomatic-restructure-plan.md`, §6b) — reference-only, never packaged.

**Not retired**: `main.py`'s `main(config: dict)` — unlike `cmd()`, it has a real, live Python
caller (`freeports_dev.pytest_plugin`, which calls it directly with a config dict to power every
fixture in `freeports-dev test`, entirely independent of argv/the CLI). Confirmed via a
workspace-wide grep before touching anything here — only `cmd()` had no caller besides the
console-script entry point itself.
"""

import os
import sys
from pathlib import Path


def _native_binary_path() -> Path:
    """Locates the compiled `freeports` binary next to the `freeports._native` extension module
    it was built alongside (the `freeports` binary, `packages/freeports_engine/src/main.rs`,
    sharing that crate's `cargo build` output with the `.so`/`.abi3.so`).

    This lookup is layout-agnostic by construction (it always resolves relative to wherever
    `freeports._native` actually got installed), so it needed **no code change** when the
    freeports_core -> freeports_engine consolidation switched the build backend from
    setuptools-rust to maturin (see
    `analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md`) — only this
    docstring/error message did, both then and again for the maturin-idiomatic restructure that
    nested the extension inside `freeports` as `freeports._native` (see
    `agent-memory/maturin-idiomatic-restructure-plan.md`). Since `freeports._native` is now a
    private submodule of the one real `freeports` package rather than a dedicated top-level
    directory, `freeports._native.__file__`'s parent is `.../site-packages/freeports/` (the
    `freeports` package directory itself), not a separate `.../site-packages/freeports_engine/`
    as it briefly was — that's exactly where `freeports-native` now needs to be hand-copied to.
    What hasn't changed: maturin (confirmed empirically, not assumed — see the consolidation
    plan's final report) cannot ship the `[[bin]] freeports` binary in the same wheel as the
    `freeports._native` pyo3 extension (`--bindings pyo3`, the default here, silently drops
    `[[bin]]` targets; `--bindings bin` builds a *separate*, non-abi3 wheel containing only the
    binary and is incompatible with `[project.scripts]`). So the binary still has to be built and
    copied into place by hand: `cargo build --release` in `packages/freeports_engine`, then copy
    the resulting `target/release/freeports` binary to sit next to the installed
    `freeports._native` module (i.e. into the same directory as `freeports._native.__file__`)
    under the name `freeports-native`.
    """
    from freeports import _native

    return Path(_native.__file__).parent / "freeports-native"


def cmd() -> None:
    """Command line entry point for the freeports script.

    Execs the native Rust binary with this process's own argv, replacing the current process
    (matching what running a real native binary directly would do — no lingering Python process
    wrapping it). Raises a clear error rather than silently falling back to the Python
    implementation if the binary isn't where it's expected — a missing binary is a packaging bug
    to fix, not something to paper over.
    """
    binary = _native_binary_path()
    if not binary.is_file():
        raise FileNotFoundError(
            f"Native freeports binary not found at {binary}. Build it with "
            "`cargo build --release` in packages/freeports_engine and copy the resulting "
            "`target/release/freeports` binary to that path (maturin cannot ship the binary "
            "and the freeports._native extension in the same wheel — see this function's own "
            "docstring)."
        )
    # argv[0] stays "freeports" (what the user actually typed), not the resolved binary path —
    # matches what running the real installed console script looked like before this change.
    os.execv(str(binary), ["freeports", *sys.argv[1:]])
