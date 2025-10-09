"""Functions for different target matching algorithms"""

import re
import pandas as pd
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
TO_SEP = ",/-"
TO_REMOVE = "!?{}[]()\"'/."
table = str.maketrans(translation_table)
for char in TO_SEP:
    table[ord(char)] = " "
for char in TO_REMOVE:
    table[ord(char)] = None


def normalize_string(string: str) -> str:
    """normalizes a string by making it lowercase and removing accents

    Parameters
    ----------
    string : str
        original string

    Returns
    -------
    str
        normalized string
    """
    return " ".join(string.lower().translate(table).split()).strip()


# To be continued (hinting and docstringing)
def dataframe_to_match(target_companies: pd.DataFrame) -> tuple[list, dict]:
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


def match_company(text, target_companies):
    norm_text = normalize_string(text)
    upper_text = text.upper()
    matching_data, regexs_dict = target_companies
    matching_buds = []
    matching_regexs = []
    for row in matching_data:
        idx, (name, buds, regexs, syms) = row
        if name in norm_text:
            return idx
        if any(bud in norm_text for bud in buds):
            matching_buds.append(idx)
    n_mbuds = len(matching_buds)
    if n_mbuds > 0:
        for bud_idx in matching_buds:
            if any(regex.search(norm_text) for regex in regexs_dict[bud_idx]):
                matching_regexs.append(bud_idx)
        n_mregexs = len(matching_regexs)
        if n_mregexs == 1:
            return matching_regexs[0]
        if n_mregexs > 1:
            raise ValueError(
                _("Ambiguous match: multiple regex matches from different companies.")
            )
    for row in matching_data:
        idx, (name, buds, regexs, syms) = row
        if any(regex.search(norm_text) for regex in regexs):
            matching_regexs.append(idx)
        if any(sym.search(upper_text) for sym in syms):
            return idx
    n_mregexs = len(matching_regexs)
    if n_mregexs == 1:
        return matching_regexs[0]
    if n_mregexs > 1:
        raise ValueError(
            _("Ambiguous match: multiple regex matches from different companies.")
        )
    return None
