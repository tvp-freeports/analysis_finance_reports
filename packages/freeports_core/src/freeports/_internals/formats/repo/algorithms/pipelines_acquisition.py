"""It contains functions to acquire all the pipelines associated with a format."""

from typing import Dict, List, Tuple, Callable

from freeports.i18n import _
from .pipelines_definition import Pipeline
from .semistructured.acquisition import (
    get_pipelines as get_semistructured,
)
from .structured.acquisition import (
    get_pipelines as get_structured,
)
from .unstructured.acquisition import (
    get_pipelines as get_unstructured,
)


def get_pipelines(
    formats_repo_dir, format_name: str, allow_partial_pipelines: bool = False
) -> Dict[str, Tuple[List[Callable], List[Callable], List[Callable]]]:
    """Get processing pipelines for a specific format.

    Combines structured, semi-structured, and unstructured pipelines for the given format.

    Parameters
    ----------
    format_name : str
        Name of the format to get pipelines for
    allow_partial_pipelines : bool
        Whether to allow pipelines with missing components

    Returns
    -------
    Dict[str, Tuple[List[Callable], List[Callable], List[Callable]]]
        Dictionary mapping pipeline names to (pdf_extracts, text_filter, deserialize) tuples

    Raises
    ------
    ValueError
        If required pipeline components are missing and allow_partial_pipelines is False

    Notes
    -----
    Each pipeline consists of three components:
    - pdf_extracts: Functions that extract relevant blocks from PDF XML
    - text_filter: Functions that convert PDF blocks to text blocks with company matching
    - deserialize: Functions that convert text blocks to structured financial data

    The function combines pipelines from structured, semi-structured, and unstructured
    processing approaches to provide comprehensive format support.
    """
    struct = get_structured(format_name, formats_repo_dir)
    semistruct = get_semistructured(format_name, formats_repo_dir)
    unstruct = get_unstructured(format_name, formats_repo_dir)

    pipelines_names = set(struct) | set(semistruct) | set(unstruct)

    pipelines = {
        name: struct.get(name, Pipeline())
        + semistruct.get(name, Pipeline())
        + unstruct.get(name, Pipeline())
        for name in pipelines_names
    }
    if not allow_partial_pipelines:
        for p in pipelines.values():
            if not p.complete():
                raise ValueError(_("Pipeline is incomplete: \n{}").format(p))
    return pipelines
