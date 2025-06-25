from logging import getLogger
from freeports_analysis.formats import TextBlock
from .utils_commons import normalize_word

logger = getLogger(__name__)


def perc_to_float(perc: str, norm: bool = True) -> float:
    perc = normalize_word(perc)
    if "." not in perc:
        perc = perc.replace(",", ".")
    if "%" in perc:
        perc = perc.replace("%", "")
        perc = normalize_word(perc)
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
    try:
        return int(data)
    except ValueError:
        logger.warning("Data %s raise a value error when converting to `int`", data)
    return None


def to_str(data: str) -> str:
    data = data.strip()
    return data


def standard_tabularizer(mapping: dict, tabularize_enanched_types: bool = True) -> dict:
    def wrapper(_):
        def tabularize(blk: TextBlock) -> dict:
            row = dict()
            for k, cast in mapping.items():
                enanched_cast = cast
                if tabularize_enanched_types:
                    if cast is int:
                        enanched_cast = to_int
                    elif cast is str:
                        enanched_cast = to_str
                row[k if k != "match" else "company"] = enanched_cast(blk.metadata[k])
            return row

        return tabularize

    return wrapper
