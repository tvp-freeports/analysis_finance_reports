"""Utilities of general interest common to all formats and that can be used
for creating `pdf_extract` or `text_filter` or `deserialize` functions
"""

from typing import Callable, TypeVar, ParamSpec


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
