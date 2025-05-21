from .consts import PDF_Formats as fmt
from typing import List, Tuple, Optional
from rapidfuzz import fuzz
from pdf_parts import PDF_part
from text_parts import Text_part
import text_parts


def EURIZON(pdf_part_list: List[PDF_part], targets: List[str]) -> List[Text_part]:
    default_search_min_ratio = 90
    text_part_list = []
    for pdf_p in pdf_part_list:
        row = pdf_p.content
        for target in targets:
            if (
                fuzz.partial_ratio(target.lower(), row.lower())
                >= default_search_min_ratio
            ):
                text_part_list.append(Text_part(text_parts.EURIZON, {}))
    return text_part_list


def _DEFAULT(pdf_part_list: List[PDF_part], targets: List[str]) -> List[Text_part]:
    pass


def get_text_bits(
    pdf_part_list: List[PDF_part], text_extractor, targets
) -> List[Text_part]:
    return text_extractor(pdf_part_list, targets)
