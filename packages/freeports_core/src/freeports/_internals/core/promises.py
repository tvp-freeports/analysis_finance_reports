"""Module containing the classes and routines to implement the `promises` system.

This concept exists in order to loose the assuption that all pages contains informations
that are self contained and indepent from other pages. It permits to reference value
in classes definition that get resolved after all document get parsed.
"""

from datetime import date
from typing import Dict, Any, Union, Annotated, TypeAlias

from pydantic import PositiveFloat, BeforeValidator, confloat, ConfigDict

from freeports.consts import Currency, SfdrArticle
from freeports.i18n import _


PromisesResolutionMap: TypeAlias = Dict[str, Any]
"""Type alias for promise resolution mapping.

A dictionary mapping promise IDs to their resolved values.
"""


class Promise:
    """Base class for deferred value resolution in financial data processing.

    Implements a promise pattern where values can be resolved later from a mapping.

    Attributes
    ----------
    _id : str
        The unique identifier for this promise

    Methods
    -------
    fulfill_with(mapping: PromisesResolutionMap) -> Any
        Resolves the promised value from the given mapping.
    """

    def __init__(self, promise_id: str):
        """Initialize a Promise with a unique identifier.

        Parameters
        ----------
        promise_id : str
            Unique identifier for this promise
        """
        self._id = str(promise_id)

    def fulfill_with(self, mapping: PromisesResolutionMap) -> Any:
        """Resolve this promise's value from the given mapping.

        Parameters
        ----------
        mapping : PromisesResolutionMap
            Dictionary containing values to resolve promises from

        Returns
        -------
        Any
            The resolved value from the mapping
        """
        return mapping[str(self)]

    def __str__(self) -> str:
        """Get string representation of the promise.

        Returns
        -------
        str
            The promise's unique identifier
        """
        return self._id

    def __repr__(self) -> str:
        """Get detailed string representation showing promise class and ID.

        Returns
        -------
        str
            String representation showing promise class and ID
        """
        return f'{self.__class__.__name__}("{str(self)}")'

    def __hash__(self):
        return hash(self._id)

    def __eq__(self, other: object) -> bool:
        """Check equality with another promise.

        Parameters
        ----------
        other : object
            The object to compare with

        Returns
        -------
        bool
            True if promises have the same ID
        """
        if not isinstance(other, Promise):
            return False
        return hash(self) == hash(other)

    def __format__(self, fmt: str) -> str:
        """Format the promise for string formatting.

        Parameters
        ----------
        fmt : str
            Format specification

        Returns
        -------
        str
            Formatted string representation
        """
        return repr(self)


PromisesResolutionContext: TypeAlias = Dict[str, Union[Promise, Any]]
"""Type alias for promise resolution context.

A dictionary containing promises and their dependencies during resolution.
"""


class CircularPromisesChain(Exception):
    """Exception raised when a circular dependency is detected in promise resolution.

    This occurs when a promise chain references itself either directly or indirectly,
    creating an infinite loop that cannot be resolved.
    """


def flatten_promise_map(mapping: PromisesResolutionMap) -> PromisesResolutionMap:
    """Flatten a mapping containing Promise objects by resolving all references.

    Processes a dictionary that may contain Promise objects, resolving each promise
    by looking up its value in the mapping until all values are concrete (non-Promise).
    Detects and prevents circular references that would cause infinite resolution loops.

    Parameters
    ----------
    mapping : PromisesResolutionMap
        Dictionary containing both direct values and Promise objects to be resolved

    Returns
    -------
    PromisesResolutionMap
        A new dictionary with all Promise objects resolved to their final values

    Raises
    ------
    CircularPromisesChain
        If a circular reference is detected in the promise resolution chain

    Notes
    -----
    This function implements a depth-first resolution algorithm that follows
    promise chains until concrete values are found. It maintains a resolution
    history to detect and prevent infinite loops from circular dependencies.
    """
    flattened = {}
    resolve_history = {}
    promises = []

    # Initial pass: separate promises from concrete values
    for key, value in mapping.items():
        if isinstance(value, Promise):
            promises.append(key)
            resolve_history[key] = []
        else:
            flattened[key] = value
    if len(promises) == 0:
        return flattened

    # Process promises until all are resolved
    while True:
        i = 0
        while True:
            p = promises[i]
            value = mapping[p]
            if not isinstance(value, Promise):
                # Found concrete value - add to flattened and remove from processing
                flattened[p] = value
                promises.pop(i)
            else:
                # Check for circular reference
                if value._id in resolve_history[p]:
                    _debug_str = f"{resolve_history[p]} -> {value._id}"
                    raise CircularPromisesChain(
                        _("Circular reference detected in promise resolution chain: ")
                        + _debug_str
                    )

                # Track resolution path and follow the reference
                resolve_history[p].append(value._id)
                mapping[p] = mapping[value._id]
                i += 1
            if i >= len(promises):
                break

        if len(promises) == 0:
            break

    return flattened


def try_convert_to_currency(value: Union[str, Promise]) -> Union[Currency, Promise]:
    """Attempt to convert a string to Currency, preserving Promise objects.

    Parameters
    ----------
    value : Union[str, Promise]
        The value to convert - either a currency string or Promise object

    Returns
    -------
    Union[Currency, Promise]
        Currency enum if conversion successful, otherwise original Promise

    Raises
    ------
    KeyError
        If the currency string is not a valid Currency enum member

    Notes
    -----
    This function is used as a Pydantic validator to handle both concrete
    currency values and Promise objects that will be resolved later.
    """
    if isinstance(value, Promise):
        return value
    return Currency(value)


# Type aliases for financial data with promise support
# Company = Annotated[str, AfterValidator(validate_company)]
PromisedMarketValue = Union[Promise, PositiveFloat]
PromisedCurrency = Annotated[
    Union[Promise, Currency],
    BeforeValidator(try_convert_to_currency),
]
PromisedFundName = Union[Promise, str]
PromisedPercNetAsstes = Union[Promise, confloat(ge=0.0, lt=1.0)]
PromisedAcquisitionCost = Union[Promise, PositiveFloat]
PromisedAcquisitionCurrency = Annotated[
    Union[Promise, Currency],
    BeforeValidator(try_convert_to_currency),
]
PromisedInterestRate = Union[Promise, confloat(ge=0.0, lt=1.0)]
PromisedDate = Union[Promise, date]
PromisedSfdrArticle = Union[Promise, SfdrArticle]


class PromisableDict:
    """Mixin providing promise fulfillment via Pydantic model_config."""

    model_config = ConfigDict(
        validate_assignment=True,
        arbitrary_types_allowed=True,
    )

    def fulfill_promises(self, mapping: PromisesResolutionMap) -> None:
        """Resolve all promise objects in this financial data instance.

        Processes each attribute that may contain a Promise object, resolving it
        using the provided mapping and performing validation where required.

        Parameters
        ----------
        mapping : PromisesResolutionMap
            Dictionary containing values to resolve promises from.

        Notes
        -----
        For attributes that require validation (perc_net_assets, company),
        the resolved values will be validated before assignment. This method
        iterates through all model attributes and resolves any Promise objects
        found, updating the instance in place.
        """
        for k, v in self.__dict__.items():
            if isinstance(v, Promise):
                setattr(self, k, v.fulfill_with(mapping))
