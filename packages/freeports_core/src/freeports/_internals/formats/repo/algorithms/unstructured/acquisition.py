"""Unstructured algorithm pipeline management.

This module handles the loading and configuration of unstructured
PDF processing algorithms for formats with complex or variable layouts
that require custom parsing logic.
"""

import logging
import importlib
from importlib.util import spec_from_file_location, module_from_spec
from typing import Dict, List, Tuple, Callable, Any, Optional
from pathlib import Path
import sys
from freeports._internals.formats.repo.algorithms.pipelines_definition import Pipeline

logger = logging.getLogger(__name__)


CONTENT_DIR = Path("content")
ALGORITHMS_DIR = CONTENT_DIR / "algorithms"
ORCHESTRATION_DIR = CONTENT_DIR / "orchestration"
TEMPLATES_DIR = CONTENT_DIR / "templates"

UNSTRUCTURED_DIR = ALGORITHMS_DIR / "unstructured"


def get_module(formats_repo_dir: Path, format_name: str) -> Optional[Any]:
    """Dynamically load an unstructured algorithm module for a given format.

    Parameters
    ----------
    formats_repo_dir : Path
        Root directory of the formats repository.
    format_name : str
        Name of the format whose algorithm module should be loaded.

    Returns
    -------
    module or None
        The loaded Python module, or None if no matching module file is found.
    """

    templates_dir = formats_repo_dir / TEMPLATES_DIR

    if str(templates_dir) not in sys.path:
        sys.path.insert(0, str(templates_dir))

    module_name = (
        format_name.lower().replace("-", "_").replace(".", "_").replace("@", "_")
    )

    module_file = formats_repo_dir / UNSTRUCTURED_DIR / f"{module_name}.py"

    package_init = formats_repo_dir / UNSTRUCTURED_DIR / module_name / "__init__.py"

    if module_file.is_file():
        module_path = module_file
        is_package = False

    elif package_init.is_file():
        module_path = package_init
        is_package = True

    else:
        return None

    runtime_name = f"_plugin_{module_name}"

    spec = spec_from_file_location(
        runtime_name,
        module_path,
        submodule_search_locations=([str(module_path.parent)] if is_package else None),
    )

    module = module_from_spec(spec)

    sys.modules[runtime_name] = module

    spec.loader.exec_module(module)

    return module


def get_pipelines(
    format_name: str, formats_repo_dir
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
        Tuple containing three dictionaries for pdf_extract, text_filter, and deserialize segments.
        Each dictionary maps pipeline names to lists of processing functions.

    Notes
    -----
    The function dynamically imports format-specific modules and extracts processing
    functions. Returns empty dictionaries if the format module is not found.
    """
    module = get_module(formats_repo_dir, format_name)
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


def standard_compute_page_class(page_classification: Any) -> Any:
    """Default page-classification passthrough (returns input unchanged).

    Parameters
    ----------
    page_classification : Any
        Classification data to pass through.

    Returns
    -------
    Any
        The same classification data unchanged.
    """
    return page_classification


def get_compute_page_class(
    formats_repo_dir: Path, format_name: str
) -> Callable[..., Any]:
    """Resolve the ``compute_page_class`` callable for a format.

    Attempts to load a format-specific implementation; falls back to
    :func:`standard_compute_page_class` if none is defined.

    Parameters
    ----------
    formats_repo_dir : Path
        Root directory of the formats repository.
    format_name : str
        Name of the format.

    Returns
    -------
    Callable
        The resolved ``compute_page_class`` function.
    """
    module = get_module(formats_repo_dir, format_name)
    if module is None:
        return standard_compute_page_class
    try:
        cpc = module.compute_page_class
        if not callable(cpc):
            raise Exception(f"Unstructured compute_page_class should be callable")
        return cpc
    except AttributeError:
        return standard_compute_page_class
