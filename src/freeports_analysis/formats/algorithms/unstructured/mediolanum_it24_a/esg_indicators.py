from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
    ColumnConfig,
    SplittingState,
    RowConfig,
    TableConfig,
    TablePosAlgorithm,
)


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    l = PdfLineSelection.text("PAI").select(lines)[0].bbox[0]
    r = PdfLineSelection.text("1° T.").select(lines)[0].bbox[0]
    first_column = ColumnConfig(limits=(l, r), splitting=SplittingState.ALLOW_DOWN)
    table_lines = (
        PdfLineSelection.area_from_bounds(
            130,
            PdfLineSelection.text("riferimento per il"),
            1e6,
            PdfLineSelection(text="rispetto ai periodi precedenti"),
        )
        / (
            PdfLineSelection.text("^ $")
            | PdfLineSelection.text("^  $")
            | PdfLineSelection.area(0.0, 780, 1e6, 1e6)
        )
    ).select(lines)

    rows, cols = zip(
        *get_table_coordinates(
            table_lines,
            algorithm_flags=TablePosAlgorithm.USE_RULER_AREA,
            table_cfg=TableConfig(
                cols=[
                    first_column,
                    ColumnConfig(),
                    ColumnConfig(),
                    ColumnConfig(),
                    ColumnConfig(),
                    ColumnConfig(),
                ]
            ),
            tolerance=0.0,
            collapse=True,
        )
    )

    res = []
    for row in sorted(set(rows)):
        key = " ".join(
            (
                table_lines.text
                for r, c, table_lines in zip(rows, cols, table_lines)
                if row == r and c == 0
            )
        ).strip()
        value = " ".join(
            (
                table_lines.text
                for r, c, table_lines in zip(rows, cols, table_lines)
                if row == r and c == 1
            )
        ).strip()
        res.append((key, value))
