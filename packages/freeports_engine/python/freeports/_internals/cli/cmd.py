"""Contains all functions related to command line use of the `freeport` script.

``cmd()`` (the `[project.scripts]` entry point `pyproject.toml` installs as the `freeports`
command) now execs the native Rust binary (Fase E, punto 6 — see
``analysis_finance_reports/agent-memory/rust-native-binary-plan.md``) instead of parsing argv and
orchestrating in Python — the whole CLI/config-resolution/execution chain this module used to
drive (`conf_parse.py`, `main.py`) is now `packages/freeports_engine/src/cli/` (config parsing,
plus the binary's own job/batch/output modules) and its `src/main.rs` entry point.
The original Python body is kept below (renamed ``_legacy_cmd``) as dead code, per this migration's
usual strangler-fig convention, until the migration is far enough along to delete it.

**Not retired**: `main.py`'s `main(config: dict)` — unlike `cmd()`, it has a real, live Python
caller (`freeports_dev.pytest_plugin`, which calls it directly with a config dict to power every
fixture in `freeports-dev test`, entirely independent of argv/the CLI). Confirmed via a
workspace-wide grep before touching anything here — only `cmd()` had no caller besides the
console-script entry point itself.
"""

import logging
import os
import sys
from pathlib import Path

from freeports._internals.core.logging import (
    HANDLER_STDERR,
    LOG_CONTEXTUAL_INFOS,
    DevDebugFormatter,
    log_config,
)
from freeports._internals.cli.conf_parse import (
    DEFAULT_CONFIG_LOCATION,
    DEFAULT_CONFIG,
    FreeportsFileConfig,
    FreeportsEnvConfig,
    FreeportsCmdConfig,
)

from freeports._internals.cli.main import main as _legacy_main


def _native_binary_path() -> Path:
    """Locates the compiled `freeports` binary next to the `freeports_engine` extension module
    it was built alongside (the `freeports` binary, `packages/freeports_engine/src/main.rs`,
    sharing that crate's `cargo build` output with the `.so`/`.abi3.so`).

    This lookup is layout-agnostic by construction (it always resolves relative to wherever
    `freeports_engine` actually got installed), so it needed **no code change** when the
    freeports_core -> freeports_engine consolidation switched the build backend from
    setuptools-rust to maturin (see
    `analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md`) — only this
    docstring/error message did. What *did* change: maturin (confirmed empirically, not assumed —
    see the consolidation plan's final report) cannot ship the `[[bin]] freeports` binary in the
    same wheel as the `freeports_engine` pyo3 extension (`--bindings pyo3`, the default here,
    silently drops `[[bin]]` targets; `--bindings bin` builds a *separate*, non-abi3 wheel
    containing only the binary and is incompatible with `[project.scripts]`). So, exactly as
    before this consolidation, the binary still has to be built and copied into place by hand:
    `cargo build --release` in `packages/freeports_engine`, then copy the resulting
    `target/release/freeports` binary to sit next to the installed `freeports_engine` module
    (i.e. into the same directory as `freeports_engine.__file__`) under the name
    `freeports-native`.
    """
    import freeports_engine

    return Path(freeports_engine.__file__).parent / "freeports-native"


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
            "and the freeports_engine extension in the same wheel — see this function's own "
            "docstring)."
        )
    # argv[0] stays "freeports" (what the user actually typed), not the resolved binary path —
    # matches what running the real installed console script looked like before this change.
    os.execv(str(binary), ["freeports", *sys.argv[1:]])


def _legacy_cmd() -> None:
    """Dead code, superseded by the Rust implementation `cmd()` execs into. Kept until the
    migration is far enough along to delete it.

    Notes
    -----
    The configuration is loaded in the following order of precedence:
    1. Command line arguments (highest priority)
    2. Environment variables
    3. Configuration files
    4. Default values (lowest priority)
    """
    rootlogger = logging.getLogger()
    logger = logging.getLogger(__package__ + ".cmd")
    logger.addHandler(HANDLER_STDERR)
    logger.propagate = False

    config = DEFAULT_CONFIG
    config_location = DEFAULT_CONFIG_LOCATION
    log_level = (5 - config["VERBOSITY"]) * 10
    logger.setLevel(log_level)

    parser = FreeportsCmdConfig.create_parser()
    config_cmd = FreeportsCmdConfig(parser.parse_args(), DEFAULT_CONFIG["VERBOSITY"])
    config_env = FreeportsEnvConfig()
    tmp_config, tmp_config_location = config_env.overwrite_config(
        DEFAULT_CONFIG, DEFAULT_CONFIG_LOCATION
    )
    tmp_config, tmp_config_location = config_cmd.overwrite_config(
        tmp_config, tmp_config_location
    )
    config_file_path = tmp_config["CONFIG_FILE"]
    config_file = FreeportsFileConfig(config_file_path)
    config, config_location = config_file.overwrite_config(
        DEFAULT_CONFIG, DEFAULT_CONFIG_LOCATION
    )
    config, config_location = config_env.overwrite_config(config, config_location)
    config, config_location = config_cmd.overwrite_config(config, config_location)
    log_level = (5 - config["VERBOSITY"]) * 10
    if log_level <= logging.DEBUG:
        handler_devdebug = logging.FileHandler("freeports.log", "w")
        handler_devdebug.addFilter(LOG_CONTEXTUAL_INFOS)
        handler_devdebug.setFormatter(DevDebugFormatter())
        rootlogger.addHandler(handler_devdebug)
    rootlogger.setLevel(log_level)
    log_config(logger, config, config_location)

    logger.removeHandler(HANDLER_STDERR)
    logger.propagate = True
    _legacy_main(config)
