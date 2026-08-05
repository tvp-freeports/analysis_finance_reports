"""Implementation independent constants and classes"""

from enum import Enum, auto
from .i18n import _

PROGRAM_DESCRIPTION = _(
    """Analyze finance reports searching for investing in companies
allegedly involved interantional law violations by third parties
"""
)


class FinancialInstrument(Enum):
    """Enumeration of financial instrument types."""

    EQUITY = auto()
    BOND = auto()


class SfdrArticle(Enum):
    """Enumeration of SFDR article classifications."""

    ART_6 = auto()
    ART_8 = auto()
    ART_9 = auto()


class Currency(Enum):
    """Enumeration of supported currency codes.

    Contains standard 3-letter ISO currency codes for major world currencies.
    """

    USD = "USD"
    EUR = "EUR"
    EURO = "EUR"
    GBP = "GBP"
    JPY = "JPY"
    CNY = "CNY"
    AUD = "AUD"
    CAD = "CAD"
    CHF = "CHF"
    CNH = "CNH"
    SEK = "SEK"
    NOK = "NOK"
    DKK = "DKK"
    SGD = "SGD"
    HKD = "HKD"
    KRW = "KRW"
    INR = "INR"
    BRL = "BRL"
    MXN = "MXN"
    RUB = "RUB"
    ZAR = "ZAR"
    TRY = "TRY"
    PLN = "PLN"
    THB = "THB"
    IDR = "IDR"
    MYR = "MYR"
    PHP = "PHP"
    ILS = "ILS"
    AED = "AED"
    SAR = "SAR"
    QAR = "QAR"
    KWD = "KWD"
    CLP = "CLP"
    COP = "COP"
    PEN = "PEN"
    ARS = "ARS"
    VND = "VND"
    UAH = "UAH"
    CZK = "CZK"
    HUF = "HUF"
    RON = "RON"
    HRK = "HRK"
    BGN = "BGN"
    ISK = "ISK"
    NZD = "NZD"
    EGP = "EGP"
    TWD = "TWD"

    @property
    def symbol(self) -> str:
        """Get the currency symbol for this currency.

        Returns
        -------
        str
            The currency symbol
        """
        return {
            "USD": "$",
            "EUR": "€",
            "GBP": "£",
            "JPY": "¥",
            "CNY": "¥",
            "CNH": "¥",
            "AUD": "$",
            "CAD": "$",
            "CHF": "CHF",
            "SEK": "kr",
            "NOK": "kr",
            "DKK": "kr",
            "SGD": "$",
            "HKD": "$",
            "KRW": "₩",
            "INR": "₹",
            "BRL": "R$",
            "MXN": "$",
            "RUB": "₽",
            "ZAR": "R",
            "TRY": "₺",
            "PLN": "zł",
            "THB": "฿",
            "IDR": "Rp",
            "MYR": "RM",
            "PHP": "₱",
            "ILS": "₪",
            "AED": "د.إ",
            "SAR": "﷼",
            "QAR": "ر.ق",
            "KWD": "د.ك",
            "EGP": "ج.م",
            "CLP": "$",
            "COP": "$",
            "PEN": "S/.",
            "ARS": "$",
            "VND": "₫",
            "UAH": "₴",
            "CZK": "Kč",
            "HUF": "Ft",
            "RON": "lei",
            "HRK": "kn",
            "BGN": "лв",
            "ISK": "kr",
            "NZD": "$",
            "TWD": "$",
        }[self.value]
