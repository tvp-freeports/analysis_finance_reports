"""Functions for different target matching algorithms"""

from difflib import SequenceMatcher
from .. import normalize_string


def prefix_similarity(a: str, b: str) -> float:
    """Compute a similarity ratio from the beginning of the two strings.
    Only matching prefixes are considered.

    Parameters
    ----------
    a : str
        first string to compare
    b : str
        second string to compare

    Returns
    -------
    float
        similarity ratio between 0.0 and 1.0
    """
    min_len = min(len(a), len(b))
    i = 0
    while i < min_len and a[i] == b[i]:
        i += 1
    return i / len(a) if len(a) else 1.0


def target_match(text: str, target: str) -> bool:
    """Check if normalized target string is contained within normalized text.

    Parameters
    ----------
    text : str
        The input text to search within
    target : str
        The target string to search for

    Returns
    -------
    bool
        True if target is found in text after normalization, False otherwise
    """
    target = normalize_string(target)
    text = normalize_string(text)
    return target in text


def target_fuzzy_match(text: str, target: str, ratio: float) -> bool:
    """Perform fuzzy string matching between normalized text and target.

    Parameters
    ----------
    text : str
        The input text to compare
    target : str
        The target string to compare against
    ratio : float
        The minimum similarity ratio threshold (0.0 to 1.0)

    Returns
    -------
    bool
        True if similarity ratio meets or exceeds threshold, False otherwise
    """
    text = normalize_string(text)
    target = normalize_string(target)
    return SequenceMatcher(None, target, text).ratio() >= ratio


def target_prefix_match(text: str, target: str, ratio: float) -> bool:
    """Check if the normalized prefix of the target string
    matches the normalized text with a given similarity ratio.

    Parameters
    ----------
    text : str
        The input text to compare against the target prefix
    target : str
        The target string whose prefix is being matched
    ratio : float
        The minimum similarity ratio threshold (0.0 to 1.0) for the prefix match

    Returns
    -------
    bool
        True if the normalized prefix similarity meets or exceeds the threshold, False otherwise
    """
    text = normalize_string(text)
    target = normalize_string(target)
    return prefix_similarity(target, text) >= ratio
