from freeports_analysis.formats.algorithms import PdfBlock
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.algorithms import TextBlock
from freeports_analysis.formats.utils.deserialize import to_int, to_currency
from freeports_analysis import output
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType
from freeports_analysis.formats.utils.text_filter import OneTextBlockType
from freeports_analysis.formats.utils.text_filter import match
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
    TablePosAlgorithm,
)


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)

    funds = (
        PdfLineSelection.area_from_bounds(
            x0=PdfLineSelection(
                font="arialnarrow-bold", font_size=(8.9, 9.1), text="Notes"
            ),
            y0=PdfLineSelection(
                font="arialnarrow-bold",
                font_size=(13.9, 14.1),
                text="Statement of Net Assets",
            ),
            x1=1e6,
            y1=PdfLineSelection(
                font="arialnarrow-bold", font_size=(8.9, 9.1), text="^ASSETS"
            ),
        )
        & PdfLineSelection.font("arialnarrow-bold")
    ).select(lines)

    tot_assets = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="arialnarrow-bold", font_size=(8.9, 9.1), text="LIABILITIES"
        ),
        (1.2, -3.5),
        100.0,
        3.0,
    ).select(lines)

    liabilities = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="arialnarrow-bold", font_size=(8.9, 9.1), text="TOTAL NET ASSETS"
        ),
        (1.2, -3.5),
        100.0,
        3.5,
    ).select(lines)

    net_assets = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(
            font="arialnarrow-bold", font_size=(8.9, 9.1), text="TOTAL NET ASSETS"
        ),
        (1.2, 0.0),
        100.0,
        2.2,
    ).select(lines)

    _, cols = zip(
        *get_table_coordinates(
            funds,
            algorithm_flags=TablePosAlgorithm.BIG_CELL_RULE
            | TablePosAlgorithm.USE_RULER_AREA,
        )
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
    filter_funds = set(
        map(
            lambda x: match.MatchFund(x.name),
            filter(lambda x: isinstance(x, output.Fund), filter_data),
        )
    )
    return [
        TextBlock.from_content(OneTextBlockType.RELEVANT_BLOCK, blk.metadata, "")
        for blk in blks
        if match.MatchFund(name=blk.metadata["fund"]) in filter_funds
    ]


def deserialize(blk):
    md = blk.metadata
    return output.FundAssets(
        fund=md["fund"],
        currency=to_currency(md["currency"]),
        tot_assets=float(to_int(md["tot_assets"])),
        net_assets=float(to_int(md["net_assets"])),
        liabilities=float(to_int(md["liabilities"])),
    )
