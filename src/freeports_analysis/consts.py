from enum import Enum
import yaml
from importlib_resources import files
ENV_PREFIX="AFINANCE_"
from enum import Enum
import freeports_analysis.data

# Leggi il file YAML
YAML_DATA=None
with files(freeports_analysis.data).joinpath("format_url_mapping.yaml").open('r') as f:
    YAML_DATA = yaml.safe_load(f)

PDF_Formats=Enum('PDF_Formats', {k: v if v is not None else [] for k, v in YAML_DATA.items()})
