from enum import Enum
import yaml
ENV_PREFIX="AFINANCE_"
DATA_DIR="data"
DATA_FILE_TARGET=f"{DATA_DIR}/target.csv"
from enum import Enum

# Leggi il file YAML
data=None
with open(f'{DATA_DIR}/format_url_mapping.yaml') as f:
    data = yaml.safe_load(f)

PDF_Formats=Enum('PDF_Formats', {k: v if v is not None else [] for k, v in data.items()})
