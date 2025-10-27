"""Definition of types for identifying characteristics related to geometrical aspects of lines."""

from typing import Optional
from pydantic import BaseModel, PositiveFloat, model_validator


class InputArea(BaseModel):
    """Represents a validated rectangular area with optional boundaries.

    This class defines a rectangular area with optional minimum and maximum coordinates
    for both x and y axes. It includes validation to ensure proper coordinate ranges.

    Attributes
    ----------
    x_min : Optional[PositiveFloat]
        Minimum x-coordinate of the area. Must be positive if provided.
    x_max : Optional[PositiveFloat]
        Maximum x-coordinate of the area. Must be positive if provided.
    y_min : Optional[PositiveFloat]
        Minimum y-coordinate of the area. Must be positive if provided.
    y_max : Optional[PositiveFloat]
        Maximum y-coordinate of the area. Must be positive if provided.

    Raises
    ------
    ValueError
        If x_max is not greater than x_min when both are provided.
    ValueError
        If y_max is not greater than y_min when both are provided.

    Examples
    --------
    >>> area = InputArea(x_min=0.0, x_max=10.0, y_min=0.0, y_max=5.0)
    >>> area.x_min
    0.0
    """

    x_min: Optional[PositiveFloat] = None
    x_max: Optional[PositiveFloat] = None
    y_min: Optional[PositiveFloat] = None
    y_max: Optional[PositiveFloat] = None

    @model_validator(mode="after")
    def validate_bounds(self) -> "InputArea":
        """Validate that maximum bounds are greater than minimum bounds.

        This validator ensures that when both minimum and maximum values are provided
        for either axis, the maximum is strictly greater than the minimum.

        Returns
        -------
        InputArea
            The validated instance.

        Raises
        ------
        ValueError
            If x_max is not greater than x_min when both are provided.
        ValueError
            If y_max is not greater than y_min when both are provided.
        """
        # Validate x-axis bounds
        if self.x_max is not None and self.x_min is not None:
            if self.x_max <= self.x_min:
                raise ValueError("x_max must be greater than x_min")

        # Validate y-axis bounds
        if self.y_max is not None and self.y_min is not None:
            if self.y_max <= self.y_min:
                raise ValueError("y_max must be greater than y_min")

        return self
