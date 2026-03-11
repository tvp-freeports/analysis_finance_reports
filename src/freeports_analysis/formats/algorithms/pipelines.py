from .commons import Pipeline
from .semistructured import (
    get_pipelines as get_semistructured,
)
from .structured import (
    get_pipelines as get_structured,
)
from .unstructured import (
    get_pipelines as get_unstructured,
)


def get_pipelines(
    format_name: str, allow_partial_pipelines: bool = False
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
    struct = get_structured(format_name)
    semistruct = get_semistructured(format_name)
    unstruct = get_unstructured(format_name)

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
