from enum import Enum
import yaml
from importlib_resources import files
import logging as log

logger = log.getLogger(__name__)


ENV_PREFIX = "AFINANCE_"


from enum import Enum
from freeports_analysis import data

# Leggi il file YAML
YAML_DATA = None
with files(data).joinpath("format_url_mapping.yaml").open("r") as f:
    YAML_DATA = yaml.safe_load(f)

logger.debug(YAML_DATA)
PDF_Formats = Enum(
    "PDF_Formats", {k: v if v is not None else [] for k, v in YAML_DATA.items()}
)
