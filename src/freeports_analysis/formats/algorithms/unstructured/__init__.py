"""Unstructured algorithm pipeline management.

This module handles the loading and configuration of unstructured
PDF processing algorithms for formats with complex or variable layouts
that require custom parsing logic.
"""

import logging
import importlib
from typing import Dict, List, Tuple, Callable, Any
from ..data import get_pageclassify_pipelines
from ..commons import Pipeline

logger = logging.getLogger(__name__)


def get_module(format_name: str):
    module_name = (
        format_name.lower().replace("-", "_").replace(".", "_").replace("@", "_")
    )
    try:
        module = importlib.import_module(
            f"{__name__}.{module_name}",
            package=__package__,
        )
        return module
    except ModuleNotFoundError as e:
        if f"No module named '{__name__}.{module_name}'" in str(e):
            return None
        else:
            raise e


def get_pipelines(
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
    module = get_module(format_name)
    if module is None:
        return {}
    try:
        pp = module.pipelines
        for n, p in pp.items():
            if not isinstance(p, Pipeline):
                raise Exception(
                    f"Unstructured alghoritm with name `{n}` is not a Pipeline, but {type(p)}"
                )
        return pp
    except AttributeError:
        return {}


def standard_compute_page_class(page_classification):
    return page_classification


def get_compute_page_class(format_name: str):
    module = get_module(format_name)
    if module is None:
        return standard_compute_page_class
    try:
        cpc = module.compute_page_class
        if not callable(cpc):
            raise Exception(f"Unstructured compute_page_class should be callable")
        return cpc
    except AttributeError:
        return standard_compute_page_class
