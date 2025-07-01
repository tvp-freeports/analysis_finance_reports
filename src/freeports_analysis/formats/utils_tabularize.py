from logging import getLogger
from freeports_analysis.formats import TextBlock
from .utils_commons import normalize_word

logger = getLogger(__name__)


def perc_to_float(perc: str, norm: bool = True) -> float:
    """Convert a percentage string to float value.

    Handles various percentage string formats by:
    - Normalizing the string (removing spaces, converting commas to dots)
    - Removing percentage signs
    - Optionally converting to decimal form (dividing by 100)

    Parameters
    ----------
    perc : str
        The percentage string to convert (may contain '%', ',', or '.')
    norm : bool, optional
        Whether to normalize the result by dividing by 100 (default True)
        If False, returns the raw numeric value from the string

    Returns
    -------
    float
        The converted float value

    Raises
    ------
    ValueError
        If the string cannot be converted to a float after processing
    """
    perc = normalize_word(perc)

    # Handle decimal separator
    if "." not in perc:
        perc = perc.replace(",", ".")

    # Handle percentage sign
    if "%" in perc:
        perc = perc.replace("%", "")
        perc = normalize_word(perc)
        if not norm:
            logger.warning(
                "Found percentage symbol '%' but `norm` is False - forcing normalization"
            )
        norm = True

    try:
        f = float(perc)
        return f / 100.0 if norm else f
    except ValueError as e:
        logger.error("Failed to convert percentage string '%f' to float", perc)
        raise ValueError(f"Could not convert '{perc}' to float") from e


def to_int(data: str) -> int:
    """Cast to int in a more loose way than the standard python `int`
    namely it remove dots or commas and spaces around the string

    Parameters
    ----------
    data : str
        number written in string form

    Returns
    -------
    int
        casted result

    Raises
    ------
    ValueError
        the resulting processed string cannot be casted to `int`
    """
    data = normalize_word(data)
    data = data.replace(",", "")
    data = data.replace(".", "")
    try:
        return int(data)
    except ValueError:
        logger.warning("Data %s raise a value error when converting to `int`", data)
    return None


def to_float(data: str) -> float:
    data = normalize_word(data)
    data = data.replace(".", "")
    data = data.replace(",", ".")
    try:
        return float(data)
    except ValueError:
        logger.warning("Data %s raise a value error when converting to `float`", data)
    return None


def to_str(data: str) -> str:
    """Normalize a string by stripping whitespace from both ends.

    Parameters
    ----------
    data : str
        The input string to be normalized
    Returns
    -------
    str
        The stripped string
    """
    return data.strip()


def standard_tabularizer(mapping: dict, tabularize_enanched_types: bool = True) -> dict:
    """Decorator factory that creates a tabularizer function for TextBlock metadata.

    The resulting decorator transforms a TextBlock's metadata into a dictionary
    with values cast according to the provided mapping. Special handling is done
    for 'match' key which is renamed to 'company'.

    Parameters
    ----------
    mapping : dict
        Dictionary mapping output keys to type casting functions
    tabularize_enanched_types : bool, optional
        Whether to use enhanced type casting for int and str (default True)
        When True, replaces int with to_int and str with to_str

    Returns
    -------
    callable
        A decorator that takes a function and returns a tabularizer function
    """

    def wrapper(_):
        def tabularize(blk: TextBlock) -> dict:
            """Transform TextBlock metadata into a typed dictionary.

            Parameters
            ----------
            blk : TextBlock
                The text block containing metadata to tabularize

            Returns
            -------
            dict
                Dictionary with keys from mapping and casted values
            """
            row = dict()
            for k, cast in mapping.items():
                enanched_cast = cast
                if tabularize_enanched_types:
                    if cast is int:
                        enanched_cast = to_int
                    elif cast is str:
                        enanched_cast = to_str
                    elif cast is float:
                        enanched_cast = to_float
                row[k if k != "match" else "company"] = enanched_cast(blk.metadata[k])
            return row

        return tabularize

    return wrapper
