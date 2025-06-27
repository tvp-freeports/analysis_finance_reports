from enum import Enum, auto
from lxml import etree
import re
from .. import PdfBlock, TextBlock
from ..utils_pdf_filter import one_pdf_blk, standard_header_font_filter
import logging as log
import datetime as dt
from typing import List

logger = log.getLogger(__name__)


@standard_header_font_filter("PORTFOLIO AS AT", "Frutiger-Black", "Frutiger-Light")
def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
    pass


def text_extract(pdf_blocks: List[PdfBlock], targets: List[str]) -> List[TextBlock]:
    text_part_list = []
    for i, row_block in enumerate(pdf_blocks):
        row = row_block.content
        row = row.lower()
        row = " ".join(row.split())
        for target in targets:
            target_n = target.lower()
            target_n = " ".join(target_n.split())
            if target_n != "" and target_n in row:
                if "%" in row:
                    date_regex = r".*(\d{2}/\d{2}/\d{4})"
                    interess_regex = r".*(\d+\.\d+%)"
                    date_match = re.match(date_regex, pdf_blocks[i + 1].content)
                    if date_match:
                        interess_match_l1 = re.match(interess_regex, row)
                        interess_match_l2 = re.match(
                            interess_regex, pdf_blocks[i + 1].content
                        )
                        txt_blk = TextBlock(
                            TextBlockType.TARGET_BOND_2LINES,
                            {
                                "match": target,
                                "nominal value": pdf_blocks[i - 1].content,
                                "date": date_match[1],
                                "interest rate": interess_match_l1[1]
                                if interess_match_l1
                                else interess_match_l2[1],
                                "currency": pdf_blocks[i + 2].content,
                                "acquisition cost": pdf_blocks[i + 3].content,
                                "carrying value": pdf_blocks[i + 4].content,
                                "% net assets": pdf_blocks[i + 5].content,
                            },
                            row_block,
                        )
                    else:
                        data_match = re.match(date_regex, row)
                        interess_match = re.match(interess_regex, row)
                        txt_blk = TextBlock(
                            TextBlockType.TARGET_BOND_LINE,
                            {
                                "match": target,
                                "date": data_match[1],
                                "interest rate": interess_match[1],
                                "nominal value": pdf_blocks[i - 1].content,
                                "currency": pdf_blocks[i + 1].content,
                                "acquisition cost": pdf_blocks[i + 2].content,
                                "carrying value": pdf_blocks[i + 3].content,
                                "% net assets": pdf_blocks[i + 4].content,
                            },
                            row_block,
                        )

                else:
                    txt_blk = TextBlock(
                        TextBlockType.TARGET_EQUITY_LINE,
                        {
                            "match": target,
                            "nominal value": pdf_blocks[i - 1].content,
                            "currency": pdf_blocks[i + 1].content,
                            "acquisition cost": pdf_blocks[i + 2].content,
                            "carrying value": pdf_blocks[i + 3].content,
                            "% net assets": pdf_blocks[i + 4].content,
                        },
                        row_block,
                    )

                text_part_list.append(txt_blk)
    return text_part_list


def tabularize(TextBlock: TextBlock) -> dict:
    m = TextBlock.metadata

    date = None
    interest_rate = None
    if (
        TextBlock.type_block == TextBlockType.TARGET_BOND_2LINES
        or TextBlockType.TARGET_BOND_LINE == TextBlock.type_block
    ):
        date = dt.datetime.strptime("".join(m["date"].split()), "%d/%m/%Y")
        interest_rate = (
            float("".join(m["interest rate"].split()).replace("%", "")) / 100
        )

    row = {
        "company": m["match"],
        "date": date,
        "interest rate": interest_rate,
        "nominal value": int("".join(m["nominal value"].replace(",", "").split())),
        "currency": "".join(m["currency"].split()),
        "acquisition cost": int(
            "".join(m["acquisition cost"].replace(",", "").split())
        ),
        "carrying value": int("".join(m["carrying value"].replace(",", "").split())),
        "% net asset": float("".join(m["% net assets"].split()).replace(",", ".")),
    }
    return row


@one_pdf_blk
class PdfBlockType(Enum):
    pass


class TextBlockType(Enum):
    TARGET_EQUITY_LINE = auto()
    TARGET_BOND_LINE = auto()
    TARGET_BOND_2LINES = auto()
