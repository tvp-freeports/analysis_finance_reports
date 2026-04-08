from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType
from freeports_analysis.formats.utils.text_filter import OneTextBlockType
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats import PdfBlock, TextBlock
from freeports_analysis.formats.utils.deserialize import deserialize_block_type
from freeports_analysis.output import FundRename, Fund, Promise
import datetime
import re
from enum import Enum, auto


class TypeBlock(Enum):
    LAST_DATE = auto()
    RENAME_ENTRY = auto()


def to_date(text):
    date_parts = text.split()
    months = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    date = datetime.date(
        int(date_parts[2]),
        months.index(date_parts[1].lower().strip()) + 1,
        int(date_parts[0]),
    )
    return date


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    opening_statements = PdfLineSelection.text(
        "The following Sub-Funds have been merged on"
    ).select(lines)

    res = []
    n_page = int(
        PdfLineSelection(
            font="frutiger-light", font_size=(7.5, 8.2), area=(0.0, 790, 1e6, 1e6)
        )
        .select(lines)[0]
        .text
    )
    previous_page = n_page - 1
    last_date_md = {"n_page": n_page}
    last_date_content = None
    for i in range(-1, len(opening_statements)):
        if i == -1:
            if PdfLineSelection.text("EVENTS OCCURRED DURING THE PERIOD").select(lines):
                continue
            top = 0.0
            last_date_content = Promise(f"date-merging-endpage-{previous_page}")
        else:
            top = opening_statements[i].bbox[3]
            last_date_content = opening_statements[i].text
        try:
            btm = opening_statements[i + 1].bbox[1]
        except IndexError:
            try:
                btm = (
                    PdfLineSelection.text("SUBSEQUENT EVENTS").select(lines)[0].bbox[1]
                )
            except IndexError:
                btm = 800

        table = PdfLineSelection(
            font="frutiger-light", area=(0.0, top, 1e6, btm)
        ).select(lines)
        _, cols = zip(*get_table_coordinates(table))
        old_names = [l for l, c in zip(table, cols) if c == 0]
        new_names = [l for l, c in zip(table, cols) if c == 4]
        (_, y0, _, y1) = old_names[0].bbox
        text_new_names = []
        text_old_names = []
        for o in old_names:
            text_old_names.append(o.text)
            (_, y0, _, y1) = o.bbox
            c = (y1 + y0) / 2.0
            new_first = new_names.pop(0)
            text_new_names.append(new_first.text)
            (_, new_y0_begin, _, new_y1_begin) = new_first.bbox
            top = new_y0_begin
            top_side = c - top
            bottom = top + 2.0 * top_side
            while len(new_names) > 0:
                (_, new_y0_end, _, new_y1_end) = new_names[0].bbox
                cc = (new_y0_end + new_y1_end) / 2.0
                if bottom > cc:
                    text_new_names[-1] += " " + new_names.pop(0).text
                else:
                    break
        res.append(
            PdfBlock(
                TypeBlock.RENAME_ENTRY,
                {"old_names": text_new_names, "new_names": text_new_names},
                last_date_content,
            )
        )
    res.append(PdfBlock(TypeBlock.LAST_DATE, last_date_md, last_date_content))
    return res


merging_regex = re.compile("The following Sub-Funds have been merged on (.+):")


def text_filter(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(name=x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    res = []
    for blk in pdf_blks:
        if blk.type_block == TypeBlock.RENAME_ENTRY:
            if isinstance(blk.content, Promise):
                date_text = blk.content
            else:
                date_text = merging_regex.search(blk.content).group(1)
            for o, n in zip(blk.metadata["old_names"], blk.metadata["new_names"]):
                current_name = MatchFund(name=n)
                if current_name in funds:
                    res.append(
                        TextBlock(
                            TypeBlock.RENAME_ENTRY,
                            {
                                "old_name": o,
                                "current_name": current_name.name,
                                "date": date_text,
                            },
                            blk,
                        )
                    )
    return res


def text_filter_last_date(pdf_blks, filter_data):
    blk = next(filter(lambda x: x.type_block == TypeBlock.LAST_DATE, pdf_blks))
    date_text = merging_regex.search(blk.content).group(1)
    return [
        TextBlock.from_content(
            TypeBlock.LAST_DATE, {"n_page": blk.metadata["n_page"]}, date_text
        )
    ]


def pdf_extract_renaming(page):
    lines = pdflines_from_pagedict(page)
    selected = (
        PdfLineSelection.area_from_bounds(
            x0=0.0,
            y0=PdfLineSelection.text("EVENTS OCCURRED DURING THE PERIOD"),
            x1=1e6,
            y1=PdfLineSelection(font="frutiger-black", text="Merging Sub-Fund"),
        )
        & PdfLineSelection.font("frutiger-light")
    ).select(lines)
    text = " ".join([x.text for x in selected])

    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {}, text)]


renaming_regex = re.compile(
    "The Sub-Fund (.+?) was renamed (.+?) on ([0-9]+[^,]+?[0-9]+)"
)


def text_filter_renaming(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(name=x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    search = renaming_regex.search(pdf_blks[0].content)
    old_name = search.group(1)
    current_name = MatchFund(name=search.group(2))
    date = search.group(3)
    if current_name in funds:
        return [
            TextBlock(
                OneTextBlockType.RELEVANT_BLOCK,
                {"old_name": old_name, "current_name": current_name.name, "date": date},
                pdf_blks[0],
            )
        ]
    return []


@deserialize_block_type(TypeBlock.RENAME_ENTRY)
def deserialize(txt_blk):
    md = txt_blk.metadata
    return FundRename(
        old_name=md["old_name"],
        current_name=md["current_name"],
        date=to_date(md["date"]) if not isinstance(md["date"], Promise) else md["date"],
    )


@deserialize_block_type(TypeBlock.LAST_DATE)
def deserialize_last_date(txt_blk):
    n_page = txt_blk.metadata["n_page"]
    return {f"date-merging-endpage-{n_page}": to_date(txt_blk.content)}
