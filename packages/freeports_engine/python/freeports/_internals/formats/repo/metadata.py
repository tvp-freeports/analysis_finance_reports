"""Data management for PDF format definitions and URL mappings.

``get_formats``/``url_to_format`` are natively ported to Rust — see
``packages/freeports_engine/src/formats_repo/metadata.rs`` and
``analysis_finance_reports/agent-memory/detect-format-metadata-rust-port-implementation-plan.md``
(Milestone 1). This module now only keeps what still has real (even if thin) Python-side logic:
``get_formats`` reshapes the native ``List[str]`` into the ``pd.DataFrame`` (indexed by
``Format name``) shape every real caller here still expects, and ``FORMAT_NAME_REGEXP`` is a
plain Python constant with live consumers of its own
(``formats/repo/algorithms/pipelines_definition.py``'s ID-format pandera checks). ``url_to_format``
is a pure passthrough kept alongside them for cohesion (same module, same "format metadata"
concern) rather than split out on its own.

Per Decision 3 of the freeports_core -> freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``), the dead
``_legacy_*`` bodies this module used to keep for reference (the original pandas/pandera CSV
readers, plus ``get_url_mapping``/``_get_url_mapping``, which had zero live callers anywhere in
the workspace even before this consolidation) were removed rather than carried over — they added
no value once `freeports_core` itself stopped existing as a separate, still-evolving migration
target.
"""

from pathlib import Path
from typing import Optional, List
import pandas as pd
import freeports_engine


METADATA_DIR = "metadata"


FORMAT_NAME_REGEXP = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.\/]+)?"


def get_formats(formats_repo_dir: Path) -> pd.DataFrame:
    """Load and validate the list of formats from formats.csv.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.

    Returns
    -------
    pd.DataFrame
        DataFrame of format names, indexed by ``Format name`` (the only shape every real caller
        actually reads).

    Notes
    -----
    Thin wrapper over the native Rust port (``freeports_engine.core.get_formats``).
    """
    format_names = freeports_engine.core.get_formats(formats_repo_dir)
    return pd.DataFrame(index=pd.Index(format_names, name="Format name"))


def url_to_format(
    formats_repo_dir: Path, format_names: List[str], url: str
) -> Optional[str]:
    """Associate a URL with a format name.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.
    url : str
        URL to match against known format URLs

    Returns
    -------
    Optional[str]
        Format name if a match is found, None otherwise

    Notes
    -----
    Thin wrapper over the native Rust port (``freeports_engine.core.url_to_format``).
    """
    return freeports_engine.core.url_to_format(formats_repo_dir, format_names, url)
