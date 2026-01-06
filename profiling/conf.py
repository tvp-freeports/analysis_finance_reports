from pathlib import Path
import shutil
from freeports_analysis.conf_parse import (
    OutStructureNormalMode,
    OutFlagsNormalMode,
    FreeportsFileConfig,
)

OUT_PATH = Path(".output")
OUT_PATH.mkdir(exist_ok=True)
shutil.rmtree(OUT_PATH)
OUT_PATH.mkdir()


long_documents = [
    "AMUNDI-EN24",
    "AMUNDI-IT24",
    "ANIMA_SGR-IT24.A",
    "ANIMA-EN23",
    "FINECO-EN23@IR",
    "MEDIOLANUM-IT24.A",
    "MEDIOLANUM-IT24.C",
    "EURIZON-EN23.A",
    "EURIZON-EN23.B",
    "DANSKEINVEST-EN24",
]
short_documents = [
    "ANIMA_SGR-IT24.B",
    "ANIMA_SICAV-EN24",
    "ARCA-IT24",
    "ASTERIA-EN23",
    "ASTERIA-EN24",
    "CARNE-EN23",
    "FIDEURAM-EN23",
    "FINECO-EN23@LUX",
    "KAIROS-EN23",
    "MEDIOLANUM-ES24.A",
    "MEDIOLANUM-IT24.B",
    "UBS-EN23",
    "EURIZON-EN21",
    "EURIZON-IT24",
    "MEDIOLANUM-ES24.B",
]

freeports_conf = {
    "VERBOSITY": 2,
    "N_WORKERS": 1,
    "BATCH_FILE": None,
    "OUT_PATH": None,
    "SAVE_PDF": False,
    "URL": None,
    "PDF": None,
    "FORMAT": None,
    "CONFIG_FILE": FreeportsFileConfig.find_config(),
    "PREFIX_OUT": None,
    "TARGET_LISTS": ["TEST"],
    "OUT_PROFILE": OutStructureNormalMode.REGULAR,
    "OUT_FLAGS": OutFlagsNormalMode(0),
}
