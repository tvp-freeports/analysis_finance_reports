from .consts import PDF_Formats as fmt
from typing import List, Tuple, Optional
from rapidfuzz import fuzz



def _DEFAULT_search(row: str, targets: List[str]) -> Tuple[bool,Optional[Tuple[str,str]]]:
    default_search_min_ratio=90
    for target in targets:
        if fuzz.partial_ratio(target.lower(),row.lower()) >= default_search_min_ratio:
            return (True, (target, row))
    return (False, None)


def relevant_lines(pdf_format: fmt, text: str, targets: List[str]) -> List[str]:
    search_func=None
    match pdf_format:
        case _:
            search_func=_DEFAULT_search
    relevant_rows=[]
    for row in text.split("\n"):
        matched, data= search_func(row,targets)
        if matched:
            relevant_rows.append(data)
    print(relevant_rows[:100])
    return relevant_rows