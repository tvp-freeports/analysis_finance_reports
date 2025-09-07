"""Functions for different target matching algorithms"""

from difflib import SequenceMatcher
import re

# from .. import normalize_string
from freeports_analysis.i18n import _

translation_table = {
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
to_sep = ".,/-"
to_remove = "!?{}[]()\"'/"
table = str.maketrans(translation_table)
for char in to_sep:
    table[ord(char)] = " "
for char in to_remove:
    table[ord(char)] = None


def normalize_string(string):
    return " ".join(string.lower().translate(table).split())


def match_full_name(text, target_companies):
    text = normalize_string(text)
    matched_names = []
    # Step 1: Check Name in text
    for idx, row in target_companies.iterrows():
        if row["Name"] in text:
            matched_names.append((idx, row["Name"]))

    if len(matched_names) == 1:
        return matched_names[0][0]  # Return the index (Name) of the match
    elif len(matched_names) > 1:
        # Return the longest match
        return max(matched_names, key=lambda x: len(x[1]))[0]
    return None


def match_regexs(text, target_companies):
    text = normalize_string(text)
    # Step 2: Check buds
    bud_matches = []
    for idx, row in target_companies.iterrows():
        buds = row.get("Buds", [])
        if any(bud.lower() in text for bud in buds):
            bud_matches.append(idx)
    if len(bud_matches) == 1:
        return bud_matches[0]
    elif len(bud_matches) > 1:
        # Step 2a: Check regexs on filtered rows
        regex_matched = []
        for idx in bud_matches:
            regexs = target_companies.loc[idx, "Regexs"]
            for pattern in regexs:
                if re.search(pattern, text):
                    regex_matched.append(idx)
                    break

        if len(set(regex_matched)) == 1:
            return regex_matched[0]
        elif len(set(regex_matched)) > 1:
            raise ValueError(
                _("Ambiguous match: multiple regex matches from different companies.")
            )
    else:
        # Step 2b: No buds matched, try all regexs
        regex_matched = []
        for idx, row in target_companies.iterrows():
            for pattern in row["Regexs"]:
                if re.search(pattern, text):
                    regex_matched.append(idx)
        if len(set(regex_matched)) == 1:
            return regex_matched[0]
        elif len(set(regex_matched)) > 1:
            raise ValueError(
                _("Ambiguous match: multiple regex matches from different companies.")
            )
    return None


def match_ticker(text, target_companies):
    text = text.upper()
    # Step 3: Check Symbols
    for idx, row in target_companies.iterrows():
        for sym in row["Symbols"]:
            if re.search(rf"\b{sym}\b", text):
                return idx
    return None


def match_company(text, target_companies, check_tickers=True, check_names=True):
    if check_names:
        m = match_full_name(text, target_companies)
        if m is not None:
            return m
        m = match_regexs(text, target_companies)
        if m is not None:
            return m

    if check_tickers:
        m = match_ticker(text, target_companies)
        if m is not None:
            return m

    return None


def target_match(text: str, target: str) -> bool:
    pass


def target_fuzzy_match(text: str, target: str, ratio: float) -> bool:
    pass


def target_prefix_match(text: str, target: str, ratio: float) -> bool:
    pass
