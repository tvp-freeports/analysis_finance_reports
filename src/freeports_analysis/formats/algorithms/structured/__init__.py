"""Structured algorithm pipeline management.

This module handles the loading and configuration of structured
PDF processing algorithms for formats with well-defined layouts
and consistent data structures.
"""

from pathlib import Path
from typing import Dict, List, Tuple, Any, Callable

from .definitions import investments as i
from .definitions import page_classify as p
from freeports_analysis.formats.algorithms.commons import Pipeline


def get_pipelines(format_name: str, formats_repo_dir):
    """Get processing pipelines for a specific structured format.

    Parameters
    ----------
    format_name : str
        Name of the format to get pipelines for

    Returns
    -------
    Tuple[Dict[str, List[Callable]], Dict[str, List[Callable]], Dict[str, List[Callable]]]
        Tuple containing three dictionaries for pdf_extract, text_filter, and deserialize segments.
        Each dictionary maps pipeline names to lists of processing functions.

    Notes
    -----
    Returns empty dictionaries if the format name is not found in the mapping.
    """
    i_pipelines = i.get_pipelines(format_name, formats_repo_dir)
    p_pipelines = p.get_pipelines(format_name, formats_repo_dir)
    all_keys = set(i_pipelines) | set(p_pipelines)
    return {
        key: i_pipelines.get(key, Pipeline()) + p_pipelines.get(key, Pipeline())
        for key in all_keys
    }
