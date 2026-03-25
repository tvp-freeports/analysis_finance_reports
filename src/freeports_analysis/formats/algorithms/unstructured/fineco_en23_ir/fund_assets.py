from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
    TablePosAlgorithm,
)
from freeports_analysis.formats.utils.pdf_extract import (
    OnePdfBlockType,
    ResultStandardExtraction,
    PdfExtractFundStandard,
)
from freeports_analysis.formats.utils.text_filter import OneTextBlockType
from freeports_analysis.formats.utils.deserialize import to_currency, to_int
from freeports_analysis.consts import Currency
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.output import FundAssets, Fund


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)

    funds = PdfLineSelection.area_from_bounds(
        x0=PdfLineSelection(
            font="timesnewromanbold", font_size=(8.9, 9.1), text="Notes"
        ),
        y0=0.0,
        x1=1e6,
        y1=PdfLineSelection(
            font="timesnewromanbold", font_size=(8.9, 9.1), text="^Assets"
        ),
    ).select(lines)
    tot_assets = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="timesnewromanbold", font_size=(8.9, 9.1), text="Total assets"
        ),
        (1.2, 0.0),
        100.0,
        1.2,
    ).select(lines)
    liabilities = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="timesnewromanbold", font_size=(8.9, 9.1), text="Total liabilities"
        ),
        (1.2, 0.0),
        100.0,
        2.2,
    ).select(lines)
    net_assets = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="timesnewromanbold", font_size=(8.9, 9.1), text="Net assets"
        ),
        (1.2, 0.0),
        100.0,
        2.2,
    ).select(lines)
    _, cols = zip(
        *get_table_coordinates(funds, algorithm_flags=TablePosAlgorithm.BIG_CELL_RULE)
    )
    n_cols = max(cols) + 1
    funds = [
        " ".join((f.text.strip() for c, f in zip(cols, funds) if c == col))
        for col in range(n_cols)
    ]
    funds, currencies = zip(*((" ".join(f.split()[:-1]), f.split()[-1]) for f in funds))
    tot_assets = [t.text for t in tot_assets]
    liabilities = [l.text for l in liabilities]
    net_assets = [n.text for n in net_assets]
    return [
        PdfBlock(
            OnePdfBlockType.RELEVANT_BLOCK,
            {
                "fund": f,
                "currency": c,
                "tot_assets": t,
                "liabilities": l,
                "net_assets": n,
            },
            "",
        )
        for f, c, t, l, n in zip(funds, currencies, tot_assets, liabilities, net_assets)
    ]


def text_filter(blks, filter_data):
    filter_funds = set(filter(lambda x: isinstance(x, Fund), filter_data))
    return [
        TextBlock.from_content(OneTextBlockType.RELEVANT_BLOCK, blk.metadata, "")
        for blk in blks
        if Fund(name=blk.metadata["fund"]) in filter_funds
    ]


def deserialize(blk):
    md = blk.metadata
    return FundAssets(
        fund=md["fund"],
        currency=to_currency(md["currency"]),
        tot_assets=float(to_int(md["tot_assets"])),
        net_assets=float(to_int(md["net_assets"])),
        liabilities=float(to_int(md["liabilities"])),
    )
