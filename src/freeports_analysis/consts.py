"""Provide basic constant and types used by all submodules,
should facilitate avoiding circular imports
"""

from abc import ABC, abstractmethod
import datetime
from enum import Enum, auto
from typing import List
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

    USD = auto()
    EUR = auto()
    GBP = auto()
    JPY = auto()
    CNY = auto()
    AUD = auto()
    CAD = auto()
    CHF = auto()
    SEK = auto()
    NOK = auto()
    DKK = auto()
    SGD = auto()
    HKD = auto()
    KRW = auto()
    INR = auto()
    BRL = auto()
    MXN = auto()
    RUB = auto()
    ZAR = auto()
    TRY = auto()
    PLN = auto()
    THB = auto()
    IDR = auto()
    MYR = auto()
    PHP = auto()
    ILS = auto()
    AED = auto()
    SAR = auto()
    QAR = auto()
    KWD = auto()
    CLP = auto()
    COP = auto()
    PEN = auto()
    ARS = auto()
    VND = auto()
    UAH = auto()
    CZK = auto()
    HUF = auto()
    RON = auto()
    HRK = auto()
    BGN = auto()
    ISK = auto()
    NZD = auto()


class FinancialData(ABC):
    """Abstract base class representing financial data.

    This class serves as the foundation for specific financial instrument types,
    providing common attributes and validation.

    Attributes
    ----------
    page : int
        The page number where the financial data appears (must be positive).
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
        market_value: float,
        currency: Currency,
        perc_net_assets: float,
        subfund: str,
        nominal_quantity: int = None,
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

    def subfund(self) -> str:
        """str: The subfund to which this instrument belongs."""
        return self._subfund

    def nominal_quantity(self) -> int:
        """int or None: The nominal quantity of the instrument, if applicable."""
        return self._nominal_quantity

    def acquisition_cost(self) -> float:
        """float or None: The original acquisition cost of the instrument."""
        return self._acquisition_cost


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
        market_value: float,
        currency: Currency,
        perc_net_assets: float,
        subfund: str,
        nominal_quantity: int = None,
        acquisition_cost: float = None,
        maturity: datetime.date = None,
        interest_rate: float = None,
    ) -> None:
        """Initialize a Bond financial instrument.

        Parameters
        ----------
        page : int
            The page number where the bond appears.
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
            market_value,
            currency,
            perc_net_assets,
            subfund,
            nominal_quantity,
            acquisition_cost,
        )
        self._maturity = maturity
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
        if not 0.0 <= self._interest_rate <= 1.0:
            logger.warning(
                "Interest rate of bond in not between 0 and 1, maybe should be normalized?"
            )
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
