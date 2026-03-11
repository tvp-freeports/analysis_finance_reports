import pandera.pandas as pa
import pandas as pd
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import LINE_SET_REGEXP


column_line_set = pa.Column(
    pd.StringDtype,
    checks=[
        pa.Check(lambda x: x.str.match(f"^{LINE_SET_REGEXP}$")),
    ],
    nullable=True,
)
