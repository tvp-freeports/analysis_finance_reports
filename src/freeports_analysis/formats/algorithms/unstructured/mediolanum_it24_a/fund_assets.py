from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.pdf_extract import (
    OnePdfBlockType,
    PdfExtractCurrencyStandard,
    ResultStandardExtraction,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats.utils.text_filter import (
    OneTextBlockType,
    extract_currency_from_text,
)
from freeports_analysis.formats.utils.deserialize import to_int, to_currency
from freeports_analysis.consts import Currency
from freeports_analysis.output import Fund, FundAssets
from freeports_analysis.formats.utils.text_filter.match import MatchFund

pdf_extract_currency = PdfExtractCurrencyStandard(
    PdfLineSelection(text="(valori espressi in")
)


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    aw = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(text="TOTALE ATTIVITÀ"), (1.2, 0.0), 100.0, 1.3
    )
    tot_assets = aw.select(lines)
    aw = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(text="sottoscrittori di quote di partecipazione riscattabili"),
        (1.2, 0.0),
        100.0,
        1.3,
    )
    liabilities = aw.select(lines)
    aw = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(text="RISCATTABILI"), (1.2, 0.0), 100.0, 1.3
    )
    net_assets = aw.select(lines)

    tot_assets, liabilities, net_assets = zip(
        *tuple(
            (tot_assets[i].text, liabilities[i].text, net_assets[i].text)
            for i in range(0, len(tot_assets), 2)
        )
    )
    aw = PdfLineSelection.area_from_movewindow(
        PdfLineSelection(text="(valori espressi in"), (1.2, -0.1), 100.0, 2.3
    )
    funds = aw.select(lines)
    _, cols = zip(*get_table_coordinates(funds))
    n_cols = max(cols) + 1
    funds = [
        " ".join((f.text.strip() for c, f in zip(cols, funds) if c == col))
        for col in range(n_cols)
    ]
    return [
        PdfBlock(
            OnePdfBlockType.RELEVANT_BLOCK,
            {"fund": f, "tot_assets": t, "liabilities": l, "net_assets": n},
            "",
        )
        for f, t, l, n in zip(funds, tot_assets, liabilities, net_assets)
    ]


def text_filter(blks, filter_data):
    filter_funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    fund_currency = None
    net_assets_md = []
    for b in blks:
        if b.type_block == ResultStandardExtraction.CURRENCY_STATEMENT:
            if fund_currency is not None:
                raise Exception("Found two different currency in same page")
            fund_currency = extract_currency_from_text(b.content)
        else:
            net_assets_md.append(b.metadata)

    return [
        TextBlock.from_content(
            OneTextBlockType.RELEVANT_BLOCK, {**md, "currency": fund_currency}, ""
        )
        for md in net_assets_md
        if MatchFund(name=md["fund"]) in filter_funds
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
