from pathlib import Path
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _

data = Path(__file__).parent

format_name_regexp = r".+\-[A-Z]{2}\d{2}(\.[^\.]+)?"

formats_schema = pa.DataFrameSchema(
    columns={
        "Name": pa.Column(pd.StringDtype),
        "Locale": pa.Column(pd.StringDtype),
        "Year": pa.Column(pd.Int16Dtype),
        "Version": pa.Column(pd.StringDtype, nullable=True),
    },
    # Creazione dell'indice composto durante la validazione
    coerce=True,  # Permette trasformazioni
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Format name",
        checks=[
            pa.Check(
                lambda x: x.str.match(f"^{format_name_regexp}$"),
                error="Format index not valid",
            )
        ],
        unique=True,
    ),
)


def get_formats():
    df = pd.read_csv(data / "formats.csv")
    df = df.assign(
        Format_name=lambda x: (
            x["Name"]
            + "-"
            + x["Locale"]
            + x["Year"].astype(str).str[-2:]
            + x["Version"].apply(lambda v: f".{v}" if pd.notna(v) and v != "" else "")
        )
    )
    df.rename(columns={"Format_name": "Format name"}, inplace=True)
    df.set_index("Format name", inplace=True)
    return formats_schema.validate(df)


VALID_FORMATS = get_formats().index.tolist()
url_mapping_schema = pa.DataFrameSchema(
    {"Url": pa.Column(str)},
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Format name",
        checks=[pa.Check(lambda x: x.isin(VALID_FORMATS), error=_("Invalid format"))],
    ),
)


def get_url_mapping():
    df = pd.read_csv(data / "url_mapping.csv", index_col=["Format name"])
    return url_mapping_schema.validate(df)
