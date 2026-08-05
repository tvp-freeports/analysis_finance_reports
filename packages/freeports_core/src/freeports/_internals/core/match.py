"""This module provides functions and classes that help in matching object against
other in a consistent and standard way.
"""

from .normalization import deep_normalize_string


class MatchFund:
    """Represents a fund with normalized name for consistent matching.

    Stores both the original name and a deeply normalized version
    to support fuzzy or case-insensitive comparisons across sources.

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
        return self._n_name

    def __init__(self, name: str) -> None:
        """Initialize MatchFund with a raw fund name.

        Parameters
        ----------
        name : str
            The raw fund name to store and normalize.
        """
        self.name = name
        self._n_name = deep_normalize_string(self.name)

    def __hash__(self) -> int:
        """Hash based on the normalized fund name.

        Returns
        -------
        int
            Hash computed from the deeply normalized name.
        """
        return hash(self._n_name)

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
