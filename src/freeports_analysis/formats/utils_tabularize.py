from logging import getLogger
from .utils_commons import normalize_word

logger = getLogger(__name__)


def perc_to_float(perc: str, norm: bool = True) -> float:
    perc = normalize_word(perc)
    if "." not in perc:
        perc = perc.replace(",", ".")
    if "%" in perc:
        perc = perc.replace("%", "")
        if not norm:
            logger.warning(
                "Found percentage symbol '%' and `norm` set to `False`, overwriting with `True`..."
            )
        norm = True

    f = float(perc)
    return f / 100.0 if norm else f


def to_int(data: str) -> int:
    data = normalize_word(data)
    data = data.replace(",", "")
    data = data.replace(".", "")
    return int(data)
