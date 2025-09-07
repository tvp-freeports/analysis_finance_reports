"""Definition of types for identify carateristic related with typographic aspect of the line"""

from typing import TypeAlias
from pydantic import PositiveFloat
from .generic import Range

Font: TypeAlias = str
FontSize: TypeAlias = PositiveFloat
FontSizeRange: TypeAlias = Range
