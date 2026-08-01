import pandera.pandas as pa
import pandas as pd
from pathlib import Path
from freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire import (
    LINE_SET_REGEXP,
)

CONTENT_DIR = Path("content")
ALGORITHMS_DIR = CONTENT_DIR / "algorithms"
ORCHESTRATION_DIR = CONTENT_DIR / "orchestration"
TEMPLATES_DIR = CONTENT_DIR / "templates"


column_line_set = pa.Column(
    pd.StringDtype,
    checks=[
        pa.Check(lambda x: x.str.match(f"^{LINE_SET_REGEXP}$")),
    ],
    nullable=True,
)
