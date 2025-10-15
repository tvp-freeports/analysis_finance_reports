"""Definition of types for identify characteristic related with geometrical aspects of the line."""

from typing import Optional
from pydantic import BaseModel, PositiveFloat, model_validator


class InputArea(BaseModel):
    """Validated Area initially input by the user

    Parameters
    ----------
    BaseModel
        pydantic BaseModel

    Raises
    ------
    ValueError
        improper x range
    ValueError
        improper y range
    """

    x_min: Optional[PositiveFloat] = None
    x_max: Optional[PositiveFloat] = None
    y_min: Optional[PositiveFloat] = None
    y_max: Optional[PositiveFloat] = None

    @model_validator(mode="after")
    def validate_bounds(self):
        if self.x_max is not None and self.x_min is not None:
            if self.x_max <= self.x_min:
                raise ValueError("x_max must be greater than x_min")
        if self.y_max is not None and self.y_min is not None:
            if self.y_max <= self.y_min:
                raise ValueError("y_max must be greater than y_min")
        return self
