"""Provide basic constant and types used by all submodules,
should facilitate avoiding circular imports
"""

from enum import Enum
import logging as log

import yaml
from importlib_resources import files
from freeports_analysis import data


logger = log.getLogger(__name__)


ENV_PREFIX = "AFINANCE_"


# Leggi il file YAML
YAML_DATA = None
with files(data).joinpath("format_url_mapping.yaml").open("r") as f:
    YAML_DATA = yaml.safe_load(f)

logger.debug(YAML_DATA)
PDF_Formats = Enum(
    "PDF_Formats", {k: v if v is not None else [] for k, v in YAML_DATA.items()}
)
