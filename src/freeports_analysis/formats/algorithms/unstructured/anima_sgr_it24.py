"""Custom pipeline for ANIMA_SGR-IT24"""

from freeports_analysis.formats import PdfBlock, TextBlock
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractFundStandard,
    PdfExtractCurrencyStandard,
    PdfExtractPageClassifyStandard,
    OnePdfBlockType,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterPageClassifyStandard,
    OneTextBlockType,
)
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.deserialize import (
    DeserializerPageClassifyStandard,
    to_date_with_it_month,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.output import Fund, FundMerge
import re


h_font_selection = (
    PdfLineSelection.font("Lato,Bold")
    | PdfLineSelection.font("TrebuchetMS-Bold")
    | PdfLineSelection.font("Open Sans,Bold")
)
header_sets = [
    PdfLineSelection.text("Titoli") & h_font_selection,
    PdfLineSelection.text("Divisa") & h_font_selection,
]

s_font_selection = (
    PdfLineSelection.font("Lato")
    | PdfLineSelection.font("Open Sans")
    | PdfLineSelection.font("Lato-Regular")
)

manco_set = PdfLineSelection.text("di Gestione del Risparmio") & s_font_selection

subfund_set = (
    PdfLineSelection.area_from_bounds(x0=manco_set, y1=header_sets[0], x1=1e6, y0=0.0)
    & s_font_selection
)


currency_font = (
    PdfLineSelection.font("Lato,Bold")
    | PdfLineSelection.font("TrebuchetMS-Bold")
    | PdfLineSelection.font("Open Sans,Bold")
)

currency_set = (
    PdfLineSelection(text="Controvalore in ")
    & currency_font - PdfLineSelection(text="in $")
) | (
    PdfLineSelection.area_from_movewindow(
        PdfLineSelection(text="Controvalore in ") & currency_font,
        vec=(0.0, 1.0),
        width_mult=1.2,
        height_mult=1.2,
    )
    & currency_font
)

b_font_selection = (
    PdfLineSelection.font("Lato")
    | PdfLineSelection.font("TrebuchetMS")
    | PdfLineSelection.font("Open Sans")
)

body_set = (
    PdfLineSelection.area_from_bounds(
        x0=0.0,
        x1=1e6,
        y1=1e6,
        y0=PdfLineSelection(text="Elenco analitico", font_size=(11, 13))
        & b_font_selection,
    )
    & PdfLineSelection.font_size(6.8, 7.2)
    & b_font_selection
)


def pdf_extract_merges(page):
    lines = pdflines_from_pagedict(page)
    top = PdfLineSelection.area_from_bounds(
        x0=0.0, y0=PdfLineSelection.text("ONDO OGGETTO DI"), x1=1e6, y1=1e6
    )
    table = (
        PdfLineSelection.area_from_bounds(
            x0=0.0,
            y0=PdfLineSelection.text("ONDO OGGETTO DI"),
            x1=1e6,
            y1=top & PdfLineSelection.text("Il Consiglio di Amministrazione"),
        )
        & PdfLineSelection.font("latobolditalic")
    ).select(lines)
    _, cols = zip(*get_table_coordinates(table))
    lines_old_name = []
    lines_current_name = []
    for c, f in zip(cols, table):
        text = f.text.strip()
        if c == 0 and text not in lines_old_name:
            lines_old_name.append(text)
        elif c == 1 and text not in lines_current_name:
            lines_current_name.append(text)
    old_name = " ".join(lines_old_name)
    current_name = " ".join(lines_current_name)
    body = (
        PdfLineSelection.area_from_movewindow(
            PdfLineSelection.text("di fusione"), (-0.5, -1.5), 100.0, 4.0
        )
        & b_font_selection
    ).select(lines)
    return [
        PdfBlock(
            OnePdfBlockType.RELEVANT_BLOCK,
            {"old_name": old_name, "current_name": current_name},
            "".join((b.text for b in body)),
        )
    ]


date_regex = re.compile(".+ con efficacia a far data dal ([0-9]+ [a-z]+ [0-9]+)")


def text_filter_merges(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    text = pdf_blks[0].content
    md = {**pdf_blks[0].metadata}
    if MatchFund(md["current_name"]) not in funds:
        return []
    m = date_regex.match(text)
    md["date"] = m.group(1)
    return [TextBlock(OneTextBlockType.RELEVANT_BLOCK, md, pdf_blks[0])]


def deserialize_merges(txt_blk):
    md = txt_blk.metadata
    return FundMerge(
        old_name=md["old_name"],
        current_name=md["current_name"],
        date=to_date_with_it_month(md["date"]),
    )


header_sets_merges = [
    PdfLineSelection.text("ONDO OGGETTO DI") & h_font_selection,
    PdfLineSelection.text("ONDO RICEVENTE") & h_font_selection,
]

pipelines = {
    "": Pipeline(
        pdf_extract=(
            PdfExtractPageClassifyStandard(
                header_sets=header_sets, page_type="investments"
            ),
            PdfExtractPageClassifyStandard(
                header_sets=header_sets_merges, page_type="merges"
            ),
        ),
        text_filter=TextFilterPageClassifyStandard(),
        deserialize=DeserializerPageClassifyStandard(),
    ),
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(body_set=body_set, manco_set=manco_set),
            PdfExtractFundStandard(subfund_set),
            PdfExtractCurrencyStandard(currency_set),
        )
    ),
    "merges": Pipeline(pdf_extract_merges, text_filter_merges, deserialize_merges),
}
