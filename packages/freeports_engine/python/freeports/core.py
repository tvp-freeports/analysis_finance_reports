"""Useful core classes used to construct format algorithms.
They are related to concept essential and depend from the program implementation.
"""
# pylint: disable=unused-import

import freeports_engine

PdfBlock = freeports_engine.core.PdfBlock
TextBlock = freeports_engine.core.TextBlock
PageParseFail = freeports_engine.core.PageParseFail
Promise = freeports_engine.core.Promise

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
