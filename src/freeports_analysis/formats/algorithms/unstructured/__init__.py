"""Unstructured algorithm pipeline management.

This module handles the loading and configuration of unstructured
PDF processing algorithms for formats with complex or variable layouts
that require custom parsing logic.
"""

import logging
import importlib
from typing import Dict, List, Tuple, Callable, Any

logger = logging.getLogger(__name__)


def _get_segment(
    segment_name: str, pipeline_modules: Dict[str, Any]
) -> Dict[str, List[Callable]]:
    """Extract processing functions from pipeline modules.

    Parameters
    ----------
    segment_name : str
        Name of the processing segment ('pdf_filter', 'text_extract', 'deserialize')
    pipeline_modules : Dict[str, Any]
        Dictionary mapping pipeline names to module objects

    Returns
    -------
    Dict[str, List[Callable]]
        Dictionary mapping pipeline names to lists of processing functions

    Notes
    -----
    If a module doesn't have the specified segment function, it is silently skipped.
    """
    segment: Dict[str, List[Callable]] = {}
    for pipeline, module in pipeline_modules.items():
        try:
            funcs = getattr(module, segment_name)
            segment[pipeline] = funcs if isinstance(funcs, list) else [funcs]
        except AttributeError:
            pass
    return segment


def get_pipes(
    format_name: str,
) -> Tuple[
    Dict[str, List[Callable]], Dict[str, List[Callable]], Dict[str, List[Callable]]
]:
    """Get processing pipelines for a specific unstructured format.

    Parameters
    ----------
    format_name : str
        Name of the format to get pipelines for

    Returns
    -------
    Tuple[Dict[str, List[Callable]], Dict[str, List[Callable]], Dict[str, List[Callable]]]
        Tuple containing three dictionaries for pdf_filter, text_extract, and deserialize segments.
        Each dictionary maps pipeline names to lists of processing functions.

    Notes
    -----
    The function dynamically imports format-specific modules and extracts processing
    functions. Returns empty dictionaries if the format module is not found.
    """
    module_name = (
        format_name.lower().replace("-", "_").replace(".", "_").replace("@", "_")
    )
    modules: Dict[str, Any] = {}
    try:
        module = importlib.import_module(
            f"{__name__}.{module_name}",
            package=__package__,
        )
        named_pipelines = []
        try:
            named_pipelines = module.pipelines
        except AttributeError:
            pass
        modules = {pipe.__name__: pipe for pipe in named_pipelines}
        modules |= {"": module}
        pdf_filter_segment = _get_segment("pdf_filter", modules)
        text_extract_segment = _get_segment("text_extract", modules)
        deserialize_segment = _get_segment("deserialize", modules)
        return pdf_filter_segment, text_extract_segment, deserialize_segment
    except ModuleNotFoundError:
        return {}, {}, {}
