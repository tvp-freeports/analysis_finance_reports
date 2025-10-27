"""Utils for creating deserialize routines and functions"""

from logging import getLogger
from typing import Callable, TypeAlias
from datetime import date, datetime
import re
from freeports_analysis.formats import TextBlock, LineParseFail
from freeports_analysis.consts import Currency, Promise
from freeports_analysis.output import Equity, Bond
from freeports_analysis.i18n import _
from freeports_analysis.logging import LOG_ADAPT_INVESTMENT_INFOS
from .text_extract import EquityBondTextBlockType
from . import normalize_word, overwrite_if_implemented, normalize_string

logger = getLogger(__name__)

DeserializeFunc: TypeAlias = Callable[[TextBlock], Equity | Bond]


def perc_to_float(perc: str, norm: bool = True) -> float:
    """Convert a percentage string to float value.

    Handles various percentage string formats by:
    - Normalizing the string (removing spaces, converting commas to dots)
    - Removing percentage signs (if percentage sign is present,
      the number gets normalized dividing by 100)
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

    Examples
    --------
    >>> perc_to_float("5.5%")
    0.055
    >>> perc_to_float("25,5", norm=False)
    25.5
    >>> perc_to_float("10 %")
    0.1
    """
    perc = normalize_word(perc)

    # Handle percentage sign
    if "%" in perc:
        perc = perc.replace("%", "")
        perc = normalize_word(perc)
        if not norm:
            logger.warning(
                _(
                    "Found percentage symbol '%' but `norm` is False - forcing normalization"
                )
            )
        norm = True

    try:
        f = to_float(perc)
        return f / 100.0 if norm else f
    except ValueError as e:
        raise ValueError(
            _("Failed to convert percentage string '{}' to float").format(perc)
        ) from e


def _force_numeric(data: str) -> str:
    reg_num = r"^\d+([\.,]\d+)*$"
    data = normalize_word(data)
    if not re.match(reg_num, data):
        logger.warning(
            _("Trying to cast to number but found '%s' - forcing cast"), data
        )
        data = re.sub(r"[^a-zA-Z.,0-9]+", "", data)
    return data


def to_float(data: str) -> float:
    """Cast to float in a more loose way than the standard python `float`
    namely it removes dots or commas and spaces around the string and handles
    thousand separators.

    Parameters
    ----------
    data : str
        number written in string form

    Returns
    -------
    float
        casted result

    Raises
    ------
    ValueError
        the resulting processed string cannot be casted to `float`

    Notes
    -----
    This function handles various numeric formats including:
    - Thousand separators (e.g., "1.000.000" -> 1000000.0)
    - Decimal separators (both '.' and ',')
    - Mixed separators (e.g., "1,000.50" -> 1000.5)
    - Whitespace around numbers
    """
    data = normalize_word(data)
    data = _force_numeric(data)
    pos_dot = data.find(".")
    pos_com = data.find(",")
    if pos_dot != -1 and pos_com != -1:
        first_pos = min(pos_dot, pos_com)
        data = data.replace(data[first_pos], "")

    data = data.replace(",", ".")
    int_reg = r"^[1-9]\d{0,2}\.\d{3}(\.\d{3})+$"
    if re.match(int_reg, data):
        data = data.replace(".", "")
    return float(data)


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

    Notes
    -----
    This function handles integer formats including:
    - Thousand separators (e.g., "1.000" -> 1000)
    - Whitespace around numbers
    - Decimal points with zero mantissa (e.g., "100.0" -> 100)

    Raises ValueError if the number has a non-zero mantissa.
    """
    data = normalize_word(data)
    data = _force_numeric(data)
    pos_dot = data.find(".")
    pos_com = data.find(",")
    if pos_dot != -1 and pos_com != -1:
        first_pos = min(pos_dot, pos_com)
        data = data.replace(data[first_pos], "")

    data = data.replace(",", ".")
    int_reg = r"^[1-9]\d{0,2}(\.\d{3})+$"
    if re.match(int_reg, data):
        data = data.replace(".", "")

    pos_dot = data.find(".")
    if pos_dot != -1:
        mantissa = int(data[pos_dot + 1 :])
        if mantissa != 0:
            raise ValueError(
                _("Number {} has a mantissa different form 0").format(data)
            )
        data = data[:pos_dot]
    return int(data)


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

    return normalize_string(data, lower=False)


def to_currency(data: str) -> Currency:
    """Convert a string to a Currency enum value.

    Parameters
    ----------
    data : str
        The currency string to convert (e.g. "USD", "EUR")

    Returns
    -------
    Currency
        The corresponding Currency enum value

    Raises
    ------
    KeyError
        If the string doesn't match any Currency enum member

    Notes
    -----
    The input string is normalized to uppercase before matching against
    the Currency enum members. Both 3-letter ISO codes and common names
    are supported (e.g., "EUR" and "EURO" both map to Currency.EUR).
    """
    if isinstance(data, Currency):
        return data
    data = normalize_word(data)

    data = data.upper()
    try:
        return Currency[data]
    except KeyError as e:
        raise ValueError from e


def to_date(data: str) -> date:
    """Convert a date string to a date object using multiple possible formats.

    Parameters
    ----------
    data : str
        The date string to parse

    Returns
    -------
    date
        The parsed date object

    Raises
    ------
    ValueError
        If the string doesn't match any of the supported date formats

    Notes
    -----
    The function tries multiple date formats in order:
    - ISO format (YYYY-MM-DD, YYYY/MM/DD)
    - European format (DD/MM/YYYY, DD.MM.YYYY)
    - US format (MM-DD-YYYY)
    - Short formats (DD/MM/YY, MM/YY)

    The first matching format is used for parsing.
    """
    data = normalize_word(data)
    formats = [
        "%Y-%m-%d",  # 2025-07-02
        "%Y/%m/%d",  # 2025/07/02
        "%d/%m/%Y",  # 02/07/2025
        "%d.%m.%Y",  # 02.07.2025
        "%d.%m.%y",  # 02.07.25
        "%d/%m/%y",  # 02/07/25
        "%m-%d-%Y",  # 07-02-2025
        "%d-%m-%y",  # 01-05-25
        "%m/%y",  # 05-25
    ]
    for fmt in formats:
        try:
            return datetime.strptime(data, fmt).date()
        except ValueError:
            continue
    raise ValueError(_("Date string '{}' is not in a recognized format.").format(data))


def standard_deserialization(
    cost_and_value_interpret_int: bool = True, quantity_interpret_float: bool = False
) -> Callable[[DeserializeFunc], DeserializeFunc]:
    """Decorator factory that creates a deserializer function for TextBlock metadata.

    This decorator factory creates a standardized deserialization pipeline that
    transforms TextBlock metadata into structured financial data objects (Equity or Bond).
    It handles the complete conversion process including type validation, error handling,
    and financial instrument type detection.

    Parameters
    ----------
    cost_and_value_interpret_int : bool, optional
        If True, interpret market value and acquisition cost as integers before
        casting to float. This is useful for handling numbers with thousand separators.
        Default is True.
    quantity_interpret_float : bool, optional
        If True, interpret quantity column as float before casting to int.
        This handles decimal quantities that should be rounded to integers.
        Default is False.

    Returns
    -------
    Callable[[DeserializeFunc], DeserializeFunc]
        A decorator that wraps deserializer functions with standardized processing

    Notes
    -----
    This decorator provides a comprehensive deserialization pipeline that:
    - Converts text metadata to appropriate Python types
    - Handles international number formats (commas, dots, spaces)
    - Supports multiple date formats
    - Detects financial instrument type (Equity vs Bond)
    - Provides graceful error handling with detailed logging
    - Supports Promise objects for deferred value resolution
    - Validates company names against target lists

    The deserialization process extracts the following fields:
    - company: Normalized company name
    - company_match: Original matched text
    - subfund: Subfund identifier
    - market_value: Numeric market value
    - currency: Currency enum
    - nominal_quantity: Quantity as integer
    - perc_net_assets: Percentage as float
    - acquisition_cost: Acquisition cost as float
    - acquisition_currency: Acquisition currency enum
    - maturity: For bonds only
    - interest_rate: For bonds only

    Examples
    --------
    >>> @standard_deserialization()
    >>> def my_deserializer(blk: TextBlock) -> Equity | Bond:
    >>>     # Custom deserialization logic can be added here
    >>>     # The standard processing is applied automatically
    >>>     pass
    """

    def wrapper(f):
        @overwrite_if_implemented(f)
        def default_other_txt_blk_deserializer(blk: TextBlock) -> Bond | Equity:
            raise ValueError(
                _("Expected bond or equity text blocks, found {}").format(
                    blk.type_block.__name__
                )
            )

        def deserialize(blk: TextBlock) -> Bond | Equity:
            """Transform TextBlock metadata into a typed dictionary.

            Parameters
            ----------
            blk : TextBlock
                The text block containing metadata to deserialize
            targets : List[str]
                List of target companies used as validation when initializing
                the financial data object

            Returns
            -------
                Finantial data deserialized from text block
            """
            md = blk.metadata

            def float_cast(x):
                if cost_and_value_interpret_int:
                    return float(to_int(x))
                return to_float(x)

            def quantity_cast(x):
                if quantity_interpret_float:
                    return to_float(x)
                return float(to_int(x))

            def try_cast(md, key, cast_func):
                LOG_ADAPT_INVESTMENT_INFOS.field = key
                if key not in md or md[key] is None:
                    return None
                try:
                    return cast_func(md[key])
                except ValueError:
                    logger.error(
                        _("Error casting, found: %s"),
                        str(md[key]).replace("\n", "\\n"),
                    )
                    logger.warning(_("Skipping field"))
                    logger.debug(str(md))
                    return None

            LOG_ADAPT_INVESTMENT_INFOS.company = md["company"]
            LOG_ADAPT_INVESTMENT_INFOS.company_match = md["company match"]
            try:
                args = {
                    "company": to_str(md["company"]),
                    "company_match": to_str(md["company match"]),
                    "subfund": to_str(md["subfund"]).upper()
                    if not isinstance(md["subfund"], Promise)
                    else md["subfund"],
                    "market_value": float_cast(md["market value"]),
                    "currency": to_currency(md["currency"]),
                    "nominal_quantity": try_cast(md, "quantity", quantity_cast),
                    "perc_net_assets": try_cast(md, "% net assets", perc_to_float),
                    "acquisition_cost": try_cast(md, "acquisition cost", float_cast),
                    "acquisition_currency": try_cast(
                        md, "acquisition currency", to_currency
                    ),
                }
                if blk.type_block == EquityBondTextBlockType.EQUITY_TARGET:
                    return Equity(**args)
                if blk.type_block == EquityBondTextBlockType.BOND_TARGET:
                    return Bond(
                        **args,
                        maturity=to_date(md["maturity"]) if "maturity" in md else None,
                        interest_rate=perc_to_float(md["interest rate"])
                        if "interest rate" in md
                        else None,
                    )
                return default_other_txt_blk_deserializer(blk)
            except ValueError as e:
                logger.error(_("Cast error"))
                raise LineParseFail(e) from e

        return deserialize

    return wrapper
