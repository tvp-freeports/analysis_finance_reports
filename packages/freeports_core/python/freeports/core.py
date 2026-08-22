"""Useful core classes used to construct format algorithms.
They are related to concept essential and depend from the program implementation.
"""
# pylint: disable=unused-import

from freeports import _native

PdfBlock = _native.core.PdfBlock
TextBlock = _native.core.TextBlock
PageParseFail = _native.core.PageParseFail
Promise = _native.core.Promise

from freeports._internals.formats.repo.algorithms.pipelines_definition import Pipeline
from freeports._internals.core.serialization import (
    to_serializable,
    from_serializable,
    dumps,
    loads,
    dump,
    load,
    SerializationError,
)
