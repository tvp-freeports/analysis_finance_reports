from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractManagmentCompanyStandard,
)
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.text_filter import (
    ResultStandardFiltering,
    StandardManagmentCompanyTextBlock,
)
from freeports_analysis.formats.utils.text_filter import match
from freeports_analysis.formats.utils.deserialize import (
    DeserializerFundStandard,
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis import output
from enum import Enum, auto


class ParseState(Enum):
    FUNDS_PARSING = auto()
    POSSIBLE_INVESTMENT_MANAGER = auto()
    OTHER = auto()


class BlockType(Enum):
    INV_MAN = auto()
    MANCO = auto()


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    body = PdfLineSelection(
        font="arialnarrow", font_size=(10.9, 11.1), area=(315.0, 0.0, 1e6, 1e6)
    ).select(lines)
    body.sort(key=lambda x: x.bbox[1])
    trashold = 20
    group_id = 0
    groups = []
    y0 = body[0].bbox[1]
    for b in body:
        y1 = b.bbox[1]
        if abs(y1 - y0) >= trashold:
            group_id += 1
        y0 = y1
        groups.append(group_id)

    return [
        PdfBlock(BlockType.INV_MAN, {"group": g}, b.text) for g, b in zip(groups, body)
    ]


pdf_extract_manco = PdfExtractManagmentCompanyStandard(
    PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="arialnarrow-bold",
            font_size=(10.9, 11.1),
            text="Management Company and Global Distributor ",
        ),
        (-0.2, 1.2),
        width_mult=1.8,
        height_mult=1.3,
    )
)


def text_filter(pdf_blocks, filter_data):
    subfunds = set(
        map(
            lambda x: match.MatchFund(x.name),
            filter(lambda x: isinstance(x, output.Fund), filter_data),
        )
    )
    blocks = []
    manco = None
    for b in pdf_blocks:
        if b.type_block == BlockType.INV_MAN:
            blocks.append(b)
        else:
            manco = b
    current_funds = set()
    current_funds_text = ""
    current_inv_man = None
    current_group = 0
    invs_blks = {}
    state = ParseState.OTHER
    for rb in blocks:
        r = rb.content.strip()
        g = rb.metadata["group"]
        if g != current_group:
            invs_blks[current_inv_man] = current_funds
            state = ParseState.POSSIBLE_INVESTMENT_MANAGER
        if r.startswith("(Only in respect of") and r.endswith(")"):
            current_funds_text += (
                r.replace("(Only in respect of the Sub-Fund", "")
                .replace("(Only in respect of", "")
                .strip()
                .replace(")", "")
                .strip()
                .replace("and", ",")
            )
            current_funds = set(
                (MatchFund(name=s.strip()) for s in current_funds_text.split(","))
            )
            state = ParseState.OTHER
        elif r.startswith("(Only in respect of"):
            state = ParseState.FUNDS_PARSING
            current_funds_text += (
                r.replace("(Only in respect of the Sub-Fund", "")
                .replace("(Only in respect of", "")
                .strip()
                .replace("and", ",")
            )
        else:
            if state == ParseState.FUNDS_PARSING:
                if r.endswith(")"):
                    state = ParseState.POSSIBLE_INVESTMENT_MANAGER
                    current_funds_text += " " + r.replace(")", "").strip().replace(
                        "and", ","
                    )
                    current_funds = set(
                        (
                            match.MatchFund(name=s.strip())
                            for s in current_funds_text.split(",")
                        )
                    )
                    current_funds_text = ""
                else:
                    current_funds_text += " " + r.replace("and", ",")
            elif state == ParseState.POSSIBLE_INVESTMENT_MANAGER:
                current_inv_man = r
                state = ParseState.OTHER
        current_group = g
    invs_blks[current_inv_man] = current_funds
    new_funds = set()
    res = []
    for i, s in invs_blks.items():
        if not s.isdisjoint(subfunds):
            res.append(
                TextBlock.from_content(
                    BlockType.INV_MAN, {"funds": set([f.name for f in s])}, i
                )
            )
            for f in s:
                if f not in subfunds:
                    new_funds.add(f)
                    res.append(
                        TextBlock.from_content(ResultStandardFiltering.FUND, {}, f.name)
                    )

    manco = StandardManagmentCompanyTextBlock(manco, subfunds.union(new_funds))
    res.append(manco)
    return res


def deserialize(blk):
    if blk.type_block == BlockType.INV_MAN:
        return output.InvestmentsManager(
            name=blk.content, managed_funds=blk.metadata["funds"]
        )


deserialize_manco = DeserializerManagmentCompanyStandard()

deserialize_fund = DeserializerFundStandard()
