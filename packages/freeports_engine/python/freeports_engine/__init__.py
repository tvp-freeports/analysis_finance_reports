"""Placeholder package for maturin's mixed-layout `python-source` requirement.

maturin's mixed Rust/Python layout expects a Python package under `python/<module-name>/`
matching `[tool.maturin] module-name` to host the compiled extension submodule it drops in next
to this file — see `agent-memory/freeports-core-consolidation-plan.md` for how this was verified
empirically. Re-exporting everything from that submodule here keeps `import freeports_engine`
resolving exactly the way it always has (all top-level attributes, plus the `core` sub-namespace),
matching the pre-consolidation setuptools-rust layout where `freeports_engine` was the compiled
extension module directly.
"""

from .freeports_engine import *  # noqa: F401,F403
from .freeports_engine import core  # noqa: F401
