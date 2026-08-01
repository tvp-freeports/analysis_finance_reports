"""Target matching algorithms for company name extraction.

This module provides functions for matching text against target companies
using various matching strategies including exact matches, regex patterns,
and symbol-based matching.
"""

import re
from typing import Dict, List, Tuple, Optional
import pandas as pd
from freeports.i18n import _
from .normalization import deep_normalize_string


class MatchFund:
    name: str
    _n_name: str

    def __str__(self):
        return self._n_name

    def __init__(self, name):
        self.name = name
        self._n_name = deep_normalize_string(self.name)

    def __hash__(self):
        return hash(self._n_name)

    def __eq__(self, other):
        return isinstance(self, other.__class__) and hash(self) == hash(other)

    def __repr__(self):
        return f'{self.__class__.__name__}("{self.name}")'
