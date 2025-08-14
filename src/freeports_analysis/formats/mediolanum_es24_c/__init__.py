"""MEDIOLANUM_ES24_C format submodule"""

import re
from typing import TypeAlias
import logging
from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils import normalize_string
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import XRange, YRange
from freeports_analysis.formats_utils.text_extract import (
    EquityBondTextBlockType,
    TextBlock,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.consts import Currency


@standard_pdf_filtering(
    header_set=PdfLineSet(font="Helvetica-Bold", text="Cartera Exterior"),
    subfund_set=PdfLineSet(font="Helvetica-Bold", area=YRange(0, 80)),
    body_set=PdfLineSet(font="Helvetica", area=XRange(72, 675)),
    currency_set=Currency["EUR"],
    deselection_list=[
        PdfLineSet(text="Cartera de inversiones financieras a"),
        PdfLineSet(text="/ Plusval"),
    ],
)
def pdf_filter(xml_root):
    raise NotImplementedError


logger = logging.getLogger(__name__)

TextBlockType: TypeAlias = EquityBondTextBlockType


def text_extract(pdf_blocks, targets):
    rows_dict = {}
    company_blks = []
    for blk in pdf_blocks:
        r = blk.metadata["table-row"]
        if r not in rows_dict:
            company_blks.append(blk)
            rows_dict[r] = blk.content.strip()
        else:
            rows_dict[r] += "|" + blk.content.strip()
    rows = [rows_dict[r] for r in sorted(rows_dict.keys())]
    reg_comp = r"[a-zA-Z &]+"
    reg_interest = r"[0-9] ?, ?[0-9]{2}"
    reg_maturity = r"[0-9]{4} ?- ?[0-9]{2} ?- ?[0-9] ?[0-9]"
    reg_curr = r"[A-Z]{3}"
    reg_number = r"[0-9]+(?: [0-9]+)* ?, ?[0-9]{2}"
    reg_pnumber = f"\(?{reg_number}\)?"
    reg_isin = "[a-zA-Z][a-zA-Z0-9]+( [a-zA-Z0-9])*"
    reg_exp = re.compile(
        f"^(?P<company>{reg_comp}) (?P<interest>{reg_interest} )?(?P<maturity>{reg_maturity}) (?P<currency>{reg_curr})? (?P<cost>{reg_number}) ({reg_pnumber} )?(?P<value>{reg_number}) ({reg_pnumber}) ({reg_isin})?"
    )
    results = []
    for i, r in enumerate(rows):
        a = " ".join(" ".join(r.split()).split("|"))
        m = reg_exp.match(a)
        if m:
            for target in targets:
                target_n = normalize_string(target)
                if target_n != "" and target_n in normalize_string(m.group("company")):
                    matched = m.groupdict()
                    md = {
                        "page": company_blks[i].metadata["page"],
                        "currency": company_blks[i].metadata["currency"],
                        "subfund": company_blks[i].metadata["subfund"],
                        "company": target,
                        "company match": matched["company"],
                        "market value": matched["value"],
                        "acquisition cost": matched["cost"],
                    }
                    if matched["currency"] is not None:
                        md["acquisition currency"] = matched["currency"]
                    if matched["interest"] is not None:
                        md["interest rate"] = matched["interest"]

                        results.append(
                            TextBlock(TextBlockType.BOND_TARGET, md, company_blks[i])
                        )

        else:
            logger.error("Anomalous line -> %s", a)
            results.append(None)
    return results


@standard_deserialization(cost_and_value_interpret_int=False)
def deserialize(text_block, targets):
    raise NotImplementedError
