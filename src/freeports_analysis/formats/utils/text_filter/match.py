"""Target matching algorithms for company name extraction.

This module provides functions for matching text against target companies
using various matching strategies including exact matches, regex patterns,
and symbol-based matching.
"""

import re
from typing import Dict, List, Tuple, Optional
import pandas as pd
from freeports_analysis.i18n import _

# Character translation table for string normalization
translation_table: Dict[str, str] = {
    "é": "e",
    "è": "e",
    "ê": "e",
    "ë": "e",
    "á": "a",
    "à": "a",
    "â": "a",
    "ä": "a",
    "í": "i",
    "ì": "i",
    "î": "i",
    "ï": "i",
    "ó": "o",
    "ò": "o",
    "ô": "o",
    "ö": "o",
    "ú": "u",
    "ù": "u",
    "û": "u",
    "ü": "u",
    "ñ": "n",
    "ç": "c",
    "ß": "ss",
    "å": "a",
    "ø": "o",
    "œ": "oe",
    "æ": "ae",
    "&": "and",
}

TO_SEP = ",-–+"
TO_REMOVE = "!?{}[]()\"'’/."
table = str.maketrans(translation_table)
for char in TO_SEP:
    table[ord(char)] = " "
for char in TO_REMOVE:
    table[ord(char)] = None


def normalize_string(string: str) -> str:
    """Normalize a string by making it lowercase and removing accents.

    Parameters
    ----------
    string : str
        Original string to normalize

    Returns
    -------
    str
        Normalized string with accents removed and whitespace collapsed

    Notes
    -----
    This function performs the following transformations:
    - Converts to lowercase
    - Removes diacritical marks (accents)
    - Replaces separator characters with spaces
    - Removes punctuation characters
    - Collapses multiple whitespace characters into single spaces
    - Strips leading and trailing whitespace
    """
    return " ".join(string.lower().translate(table).split()).strip()
