"""Utilities of general interest common to all formats and that can be used
for creating `pdf_extract` or `text_filter` or `deserialize` functions
"""

from typing import Callable, TypeVar, ParamSpec

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


def deep_normalize_string(string: str) -> str:
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


def normalize_string(string: str, lower: bool = True) -> str:
    """Normalize a string by:
    1. Stripping leading/trailing whitespace
    2. Converting to lowercase if `lower`
    3. Collapsing multiple whitespaces into single spaces

    Parameters
    ----------
    string : str
        Input string to normalize
    lower : bool
        Determine if the string has to be lowered

    Returns
    -------
    str
        Normalized string
    """
    string = string.strip()
    if lower:
        string = string.lower()
    string = " ".join(string.split())
    return string


def normalize_word(word: str, lower: bool = False) -> str:
    """Normalize a word by:
    1. Stripping leading/trailing whitespace
    2. Removing all whitespace between characters
    3. Converting to lowercase if `lower`

    Parameters
    ----------
    word : str
        Input word to normalize
    lower : bool
        Determine if the string has to be lowered

    Returns
    -------
    str
        Normalized word with no whitespace
    """
    word = word.strip()
    word = "".join(word.split())
    if lower:
        word = word.lower()
    return word
