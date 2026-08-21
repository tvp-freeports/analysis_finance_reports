"""This module provides functions and classes that help in matching object against
other in a consistent and standard way.
"""

import freeports_engine


class MatchFund:
    """Represents a fund with normalized name for consistent matching.

    This is a **bridge class**, not a full Rust port: its matching logic (name
    normalization, hashing, equality) is delegated to ``freeports_engine.core.MatchFund``
    (see ``packages/freeports_engine/src/core/match_fund.rs``), but it stays a plain Python
    class because it is used as a mixin base for Pydantic models (see
    ``output/classes_schema.py::Fund``, which does ``class Fund(BaseModel, MatchFund,
    PromisableDict)`` and calls ``MatchFund.__init__``/``__hash__``/``__eq__`` as unbound
    methods on ``self``) — a PyO3 pyclass cannot fill that role. Delete this bridge, and
    inherit from the Rust class directly, once ``Fund`` no longer needs a Python-side
    mixin (i.e. once it is itself ported off ``BaseModel``/``PromisableDict``).

    Attributes
    ----------
    name : str
        Original (un-normalized) fund name.
    _n_name : str
        Deeply normalized fund name used for hashing and equality.
    """

    name: str
    _n_name: str

    def __str__(self) -> str:
        """Return the normalized fund name as string representation.

        Returns
        -------
        str
            The deeply normalized fund name.
        """
        return str(self._core)

    def __init__(self, name: str) -> None:
        """Initialize MatchFund with a raw fund name.

        Parameters
        ----------
        name : str
            The raw fund name to store and normalize.
        """
        self._core = freeports_engine.core.MatchFund(name)
        self.name = self._core.name
        self._n_name = self._core.n_name

    def __hash__(self) -> int:
        """Hash based on the normalized fund name.

        Returns
        -------
        int
            Hash computed from the deeply normalized name.
        """
        return hash(self._core)

    def __eq__(self, other: object) -> bool:
        """Compare two MatchFund objects by normalized name hash.

        Parameters
        ----------
        other : object
            Another MatchFund instance to compare against.

        Returns
        -------
        bool
            True if both have the same normalized name hash.
        """
        return isinstance(self, other.__class__) and hash(self) == hash(other)

    def __repr__(self) -> str:
        """Return a code-like representation showing the original name.

        Returns
        -------
        str
            A string like ``MatchFund("Original Name")``.
        """
        return f'{self.__class__.__name__}("{self.name}")'
