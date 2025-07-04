"""Provide basic constant and types used by all submodules,
should facilitate avoiding circular imports
"""

from abc import ABC, abstractmethod
import datetime
from enum import Enum, auto
from typing import List
import logging as log
import importlib

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
PdfFormats = Enum(
    "PdfFormats", {k: v if v is not None else [] for k, v in YAML_DATA.items()}
)


class FinancialInstrument(Enum):
    """Enumeration of financial instrument types."""

    EQUITY = auto()
    BOND = auto()


class Currency(Enum):
    """Enumeration of supported currency codes.

    Contains standard 3-letter ISO currency codes for major world currencies.
    """

    USD = "$"
    EUR = "€"
    GBP = "£"
    JPY = "¥"
    CNY = "¥"
    AUD = "$"
    CAD = "$"
    CHF = "CHF"
    SEK = "kr"
    NOK = "kr"
    DKK = "kr"
    SGD = "$"
    HKD = "$"
    KRW = "₩"
    INR = "₹"
    BRL = "R$"
    MXN = "$"
    RUB = "₽"
    ZAR = "R"
    TRY = "₺"
    PLN = "zł"
    THB = "฿"
    IDR = "Rp"
    MYR = "RM"
    PHP = "₱"
    ILS = "₪"
    AED = "د.إ"
    SAR = "﷼"
    QAR = "ر.ق"
    KWD = "د.ك"
    CLP = "$"
    COP = "$"
    PEN = "S/."
    ARS = "$"
    VND = "₫"
    UAH = "₴"
    CZK = "Kč"
    HUF = "Ft"
    RON = "lei"
    HRK = "kn"
    BGN = "лв"
    ISK = "kr"
    NZD = "$"


class FinancialData(ABC):
    """Abstract base class representing financial data.

    This class serves as the foundation for specific financial instrument types,
    providing common attributes and validation.

    Attributes
    ----------
    page : int
        The page number where the financial data appears (must be positive).
    targets: List[str]
        The list of companies to search for, used as company validation
    company : str
        The name of the company or issuer.
    market_value : float
        The current market value of the instrument.
    currency : Currency
        The currency in which the value is denominated.
    perc_net_assets : float
        Percentage of net assets (must be between 0 and 1).
    subfund : str
        The subfund to which this instrument belongs.
    nominal_quantity : int, optional
        The nominal quantity of the instrument, if applicable.
    acquisition_cost : float, optional
        The original acquisition cost of the instrument.

    Raises
    ------
    ValueError
        If perc_net_assets is not between 0 and 1.
        If page is not a positive number.
    """

    def __init__(
        self,
        page: int,
        targets: List[str],
        company: str,
        subfund: str,
        nominal_quantity: int,
        market_value: float,
        currency: Currency,
        perc_net_assets: float,
        acquisition_cost: float = None,
    ):
        if not 0.0 <= perc_net_assets <= 1.0:
            raise ValueError(
                f"perc_net_assets must be between 0 and 1, not {perc_net_assets}"
            )
        if not page > 0:
            raise ValueError(f"page should be a positive number, not {page}")
        if company not in targets:
            raise ValueError(f"company should be between targets, not {company}")
        self._company = company
        self._page = page
        self._market_value = market_value
        self._currency = currency
        self._perc_net_assets = perc_net_assets
        self._subfund = subfund
        self._nominal_quantity = nominal_quantity
        self._acquisition_cost = acquisition_cost

    @property
    @abstractmethod
    def instrument(self) -> FinancialInstrument:
        """Abstract property to identify the financial instrument type.
        Returns
        -------
        FinancialInstrument
            The type of financial instrument (EQUITY, BOND, etc.)
        """

    @property
    def page(self) -> int:
        """int: The page number where the financial data appears."""
        return self._page

    @property
    def perc_net_assets(self) -> float:
        """float: Percentage of net assets (between 0 and 1)."""
        return self._perc_net_assets

    @property
    def company(self) -> str:
        """str: The name of the company or issuer."""
        return self._company

    @property
    def market_value(self) -> float:
        """float: The current market value of the instrument."""
        return self._market_value

    @property
    def currency(self) -> Currency:
        """Currency: The currency in which the value is denominated."""
        return self._currency

    @property
    def subfund(self) -> str:
        """str: The subfund to which this instrument belongs."""
        return self._subfund

    @property
    def nominal_quantity(self) -> int:
        """int or None: The nominal quantity of the instrument, if applicable."""
        return self._nominal_quantity

    @property
    def acquisition_cost(self) -> float:
        """float or None: The original acquisition cost of the instrument."""
        return self._acquisition_cost

    def to_dict(self) -> dict:
        return {
            "Page report": self.page,
            "Company": self.company,
            "Financial instrument": self.instrument.name,
            "Sub-fund": self.subfund,
            "Nominal/Quantity": self.nominal_quantity,
            "Market value": self.market_value,
            "Currency": self.currency.name,
            "% Net Assets": self.perc_net_assets,
            "Acquisition cost": self.acquisition_cost,
            "Maturity": None,
            "Interest rate": None,
        }

    def _str_additional_infos(self) -> str:
        string = ""
        if self.acquisition_cost is not None:
            string += f"\t\tAquisition cost:\t{self.acquisition_cost:.2f}{self.currency.value}\n"
        return string

    def __str__(self) -> str:
        string = f"{self.__class__.__name__}:\n"
        string += f"\tType match:\t{self.instrument.name}\t(pag. {self.page})\n"
        string += f"\tSubfund:\t{self.subfund}\n"
        string += f"\tCompany:\t{self.company}\n"
        string += f"\tCurrency:\t{self.currency.name}\n"
        string += f"\tMarket value:\t{self.market_value:.2f}{self.currency.value}\t({self.perc_net_assets:.3%} of net assets)\n"
        string += f"\tQuantity:\t{self.nominal_quantity}\n"
        string += "\tAdditional infos: {"
        add_string = self._str_additional_infos()
        if add_string != "":
            string += "\n" + add_string + "\t"
        string += "}\n"
        return string

    def __eq__(self, other) -> bool:
        eq = True
        eq = eq and self.instrument == other.instrument
        eq = eq and self.page == other.page
        eq = eq and self.subfund == other.subfund
        eq = eq and self.currency == other.currency
        eq = eq and self.market_value == other.market_value
        eq = eq and self.perc_net_assets == other.perc_net_assets
        eq = eq and self.nominal_quantity == other.nominal_quantity
        eq = eq and self.acquisition_cost == other.acquisition_cost
        eq = eq and self.company == other.company
        return eq


class Equity(FinancialData):
    """Concrete class representing equity financial instruments.

    Inherits from FinancialData and implements the instrument property
    to identify as EQUITY type.
    """

    @property
    def instrument(self) -> FinancialInstrument:
        """FinancialInstrument: Identifies this instrument as EQUITY type.

        Returns
        -------
        FinancialInstrument
            Always returns FinancialInstrument.EQUITY
        """
        return FinancialInstrument.EQUITY


class Bond(FinancialData):
    """Concrete class representing bond financial instruments.

    Extends FinancialData with bond-specific attributes including maturity date
    and interest rate.

    Attributes
    ----------
    maturity : datetime.date, optional
        The maturity date of the bond.
    interest_rate : float, optional
        The interest rate of the bond (should be between 0 and 1).
    """

    def __init__(
        self,
        page: int,
        targets: List[str],
        company: str,
        subfund: str,
        nominal_quantity: int,
        market_value: float,
        currency: Currency,
        perc_net_assets: float,
        acquisition_cost: float = None,
        maturity: datetime.date = None,
        interest_rate: float = None,
    ) -> None:
        """Initialize a Bond financial instrument.

        Parameters
        ----------
        page : int
            The page number where the bond appears.
        targets: List[str]
            The list of companies to search for, used as company validation
        company : str
            The issuer of the bond.
        market_value : float
            Current market value of the bond.
        currency : Currency
            Currency denomination.
        perc_net_assets : float
            Percentage of net assets (0-1).
        subfund : str
            Associated subfund.
        nominal_quantity : int, optional
            Nominal quantity of bonds.
        acquisition_cost : float, optional
            Original acquisition cost.
        maturity : datetime.date, optional
            Bond maturity date.
        interest_rate : float, optional
            Bond interest rate (0-1).
        """
        super().__init__(
            page,
            targets,
            company,
            subfund,
            nominal_quantity,
            market_value,
            currency,
            perc_net_assets,
            acquisition_cost,
        )
        self._maturity = maturity
        if interest_rate is not None and not 0.0 <= interest_rate <= 1.0:
            logger.warning(
                "Interest rate of bond in not between 0 and 1, maybe should be normalized?"
            )
        self._interest_rate = interest_rate

    @property
    def maturity(self) -> datetime.date:
        """datetime.date or None: The maturity date of the bond."""
        return self._maturity

    @property
    def interest_rate(self) -> float:
        """float or None: The interest rate of the bond.

        Returns
        -------
        float
            The interest rate value

        Notes
        -----
        Logs a warning if interest rate is not normalized (0-1 range).
        """
        return self._interest_rate

    @property
    def instrument(self) -> FinancialInstrument:
        """FinancialInstrument: Identifies this instrument as BOND type.

        Returns
        -------
        FinancialInstrument
            Always returns FinancialInstrument.BOND
        """
        return FinancialInstrument.BOND

    def to_dict(self):
        row = super().to_dict()
        row["Maturity"] = self.maturity
        row["Interest rate"] = self.interest_rate
        return row

    def _str_additional_infos(self) -> str:
        string = super()._str_additional_infos()
        if self.maturity is not None and self.interest_rate is not None:
            string += f"\t\tMaturity:\t\t{self.maturity} +{self.interest_rate:.3%}\n"
        elif self.maturity is not None:
            string += f"\t\tMaturity:\t\t{self.maturity}\n"
        elif self.interest_rate is not None:
            string += f"\t\tInterest rate:\t{self.interest_rate:.3%}\n"
        return string

    def __eq__(self, other):
        eq = super().__eq__(other)
        eq = eq and self.maturity == other.maturity
        eq = eq and self.interest_rate == other._interest_rate
        return eq


def _get_module(module_name: str):
    try:
        module = importlib.import_module(
            f"freeports_analysis.formats.{module_name}", package=__package__
        )
    except ImportError:
        print(f"Errore: modulo {module_name} non trovato")
        raise
    return module
