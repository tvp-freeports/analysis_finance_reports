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
    "œ": "oe",
    "æ": "ae",
}

TO_SEP = ",/-"
TO_REMOVE = "!?{}[]()\"'/."
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


def dataframe_to_match(target_companies: pd.DataFrame) -> Tuple[List[Tuple], Dict]:
    """Prepare target company data for matching.

    Parameters
    ----------
    target_companies : pd.DataFrame
        DataFrame containing company matching data

    Returns
    -------
    Tuple[List[Tuple], Dict]
        Tuple containing:
        - matching_data: List of tuples with company matching information
        - regexs_table: Dictionary mapping company indices to compiled regex patterns

    Notes
    -----
    The returned data structure is optimized for efficient matching:
    - Companies are sorted by name length (longest first) for exact matching
    - Regex patterns are pre-compiled for performance
    - Symbol patterns are compiled with word boundary anchors
    """
    df = target_companies.copy()
    df["Regexs"] = df["Regexs"].apply(
        lambda rs: [re.compile(r, re.IGNORECASE | re.DOTALL) for r in rs]
    )
    df["Symbols"] = df["Symbols"].apply(
        lambda syms: [
            re.compile(rf"\b{sym}\b", re.IGNORECASE | re.DOTALL) for sym in syms
        ]
    )
    d = df.to_dict(orient="index")
    regexs_table = {idx: data["Regexs"] for idx, data in d.items()}
    matching_data = [
        (idx, (data["Name"], data["Buds"], data["Regexs"], data["Symbols"]))
        for idx, data in d.items()
    ]
    matching_data.sort(key=lambda row: len(row[1][0]), reverse=True)
    return matching_data, regexs_table


def match_company(
    text: str, target_companies: Tuple[List[Tuple], Dict]
) -> Optional[str]:
    """Match text against target companies using multiple matching strategies.

    This function implements a sophisticated multi-stage matching algorithm
    that balances accuracy and performance by trying different matching
    strategies in order of specificity. It's designed to handle real-world
    variations in company name representations in financial documents.

    Parameters
    ----------
    text : str
        Text to match against company names. This is typically extracted
        from PDF documents and may contain formatting artifacts.
    target_companies : Tuple[List[Tuple], Dict]
        Prepared target company data from dataframe_to_match. The tuple contains:
        - List[Tuple]: Company data sorted by name length (longest first)
        - Dict: Pre-compiled regex patterns for each company

    Returns
    -------
    Optional[str]
        Company identifier if a match is found, None otherwise.
        The identifier corresponds to the index in the original target dataframe.

    Raises
    ------
    ValueError
        If multiple companies match the text ambiguously, indicating
        the text could refer to more than one company in the target list.

    Notes
    -----
    The matching process uses multiple strategies in order of specificity:
    1. **Exact company name matches**: Fastest, most specific
    2. **BUD (Business Unit Designator) matches**: With regex validation
    3. **Regex pattern matches**: Flexible pattern-based matching
    4. **Stock symbol matches**: For ticker symbol identification

    This multi-stage approach ensures:
    - High accuracy through exact matches when possible
    - Good performance by trying faster strategies first
    - Flexibility through regex and symbol matching
    - Ambiguity detection to prevent incorrect matches

    Examples
    --------
    >>> # Assuming target_companies is prepared data
    >>> match = match_company("Microsoft Corporation", target_companies)
    >>> print(match)
    'MSFT'  # Company identifier

    >>> # With ambiguous text
    >>> match = match_company("ABC Inc", target_companies)
    >>> # Raises ValueError if multiple companies match
    """
    upper_text = text.upper()
    text = normalize_string(text)
    matching_data, regexs_dict = target_companies
    matching_buds: List[str] = []
    matching_regexs: List[str] = []

    # First pass: exact name matches
    for row in matching_data:
        idx, (name, buds, regexs, syms) = row
        if name in text:
            return idx
        if any(bud in text for bud in buds):
            matching_buds.append(idx)

    # Second pass: bud matches with regex validation
    if len(matching_buds) > 0:
        for bud_idx in matching_buds:
            if any(regex.search(text) for regex in regexs_dict[bud_idx]):
                matching_regexs.append(bud_idx)
        n_mregexs = len(matching_regexs)
        if n_mregexs == 1:
            return matching_regexs[0]
        if n_mregexs > 1:
            raise ValueError(
                _("Ambiguous match: multiple regex matches from different companies.")
            )

    # Third pass: regex and symbol matches
    for row in matching_data:
        idx, (name, buds, regexs, syms) = row
        if any(regex.search(text) for regex in regexs):
            matching_regexs.append(idx)
        if any(sym.search(upper_text) for sym in syms):
            return idx

    # Final resolution of regex matches
    n_mregexs = len(matching_regexs)
    if n_mregexs == 1:
        return matching_regexs[0]
    if n_mregexs > 1:
        raise ValueError(
            _("Ambiguous match: multiple regex matches from different companies.")
        )

    return None
