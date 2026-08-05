"""This module contains the schema of the output classes.

Here are defining the object that will be outputted in the csv files.
"""

from abc import ABC
from typing import Optional, Set
from datetime import datetime

from pydantic import BaseModel, Field, model_validator, PositiveFloat, NonNegativeFloat

from freeports._internals.core.match import MatchFund
from freeports._internals.core.promises import (
    Promise,
    PromisableDict,
    PromisedAcquisitionCost,
    PromisedAcquisitionCurrency,
    PromisedCurrency,
    PromisedDate,
    PromisedFundName,
    PromisedInterestRate,
    PromisedMarketValue,
    PromisedPercNetAsstes,
    PromisedSfdrArticle,
)
from freeports.i18n import _


class Investment(BaseModel, ABC, PromisableDict):
    """Abstract base class representing a financial investment.

    This class serves as the foundation for different types of financial
    instruments, providing common attributes and validation logic.

    Attributes
    ----------
    company : Company
        Validated company name from predefined list
    company_match : str
        Original company name as matched in the source document
    nominal_quantity : Optional[PositiveFloat]
        Number of units/shares held
    market_value : PromisedMarketValue
        Current market value of the investment
    currency : PromisedCurrency
        Currency of the market value
    perc_net_assets : Optional[PromisedPercNetAsstes]
        Percentage of total net assets represented by this investment
    acquisition_cost : Optional[PromisedAcquisitionCost]
        Original acquisition cost
    acquisition_currency : Optional[PromisedAcquisitionCurrency]
        Currency of the acquisition cost

    Notes
    -----
    This class uses Pydantic for data validation and supports Promise objects
    for deferred value resolution. All currency values are validated against
    the Currency enum, and company names are validated against the predefined
    companies list.
    """

    # company: Company = Field(serialization_alias="Investee")
    company: str = Field(serialization_alias="Investee")
    company_match: str = Field(serialization_alias="Triggering text")
    fund: PromisedFundName = Field(exclude=True)
    nominal_quantity: Optional[PositiveFloat] = Field(
        default=None, serialization_alias="Nominal/Quantity"
    )
    market_value: PromisedMarketValue = Field(serialization_alias="Market value")
    currency: PromisedCurrency = Field(serialization_alias="Currency")
    perc_net_assets: Optional[PromisedPercNetAsstes] = Field(
        default=None, serialization_alias="% net assets"
    )
    acquisition_cost: Optional[PromisedAcquisitionCost] = Field(
        default=None, serialization_alias="Acquisition cost"
    )
    acquisition_currency: Optional[PromisedAcquisitionCurrency] = Field(
        default=None, serialization_alias="Acquisition currency"
    )

    def __str__(self) -> str:
        """Generate a formatted string representation of the investment.

        Returns
        -------
        str
            Formatted multi-line string with investment details
        """
        string = f"{self.__class__.__name__}:\n"
        translated_field = _("Company")
        string += f"\t{translated_field}:\t{self.company_match}\t[{self.company}]\n"
        translated_field = _("Currency")
        curr_name = (
            self.currency if isinstance(self.currency, Promise) else self.currency.name  # pylint: disable=no-member
        )
        string += f"\t{translated_field}:\t{curr_name}\n"
        translated_field = _("Market value")
        symbol = "" if isinstance(self.currency, Promise) else self.currency.symbol
        string += f"\t{translated_field}:\t{self.market_value:.2f}{symbol}"
        if self.perc_net_assets is not None:
            translated_field = _("of net assets")
            string += f"\t({self.perc_net_assets:.3%} {translated_field})"
        string += "\n"
        if self.nominal_quantity is not None:
            translated_field = _("Quantity")
            string += f"\t{translated_field}:\t{self.nominal_quantity}\n"
        if self.acquisition_cost is not None:
            translated_field = _("Acquisition cost")
            string += f"\t{translated_field}:\t{self.acquisition_cost:.2f}"
        if self.acquisition_currency is not None:
            symbol = (
                ""
                if isinstance(self.acquisition_currency, Promise)
                else self.acquisition_currency.symbol
            )
            string += f"{symbol}\n"
            translated_field = _("Acquisition currency")
            curr_name = (
                self.acquisition_currency
                if isinstance(self.acquisition_currency, Promise)
                else self.acquisition_currency.name
            )
            string += f"\t{translated_field}:\t{curr_name}"
        string += "\n"
        return string

    def __hash__(self) -> int:
        """Return a hash based on the serialized model fields."""
        return hash(frozenset(self.model_dump(mode="json", by_alias=True).items()))


class Equity(Investment):
    """Represents an equity investment (stocks, shares)."""


class Bond(Investment):
    """Represents a bond investment with maturity and interest rate.

    Attributes
    ----------
    maturity : Optional[datetime.date]
        Bond maturity date when principal is repaid
    interest_rate : Optional[PromisedInterestRate]
        Annual interest rate as a decimal value (e.g., 0.05 for 5%)

    Notes
    -----
    Bond investments represent debt securities that pay periodic interest
    and return the principal at maturity. The interest rate is stored as
    a decimal value (e.g., 0.05 represents 5% annual interest).
    """

    maturity: Optional[datetime.date] = Field(default=None)
    interest_rate: Optional[PromisedInterestRate] = Field(default=None)

    def __str__(self) -> str:
        """Generate a formatted string representation of the bond investment.

        Returns
        -------
        str
            Formatted multi-line string with bond details including maturity and interest rate
        """
        add_infos = False
        string = super().__str__()
        translated_field = _("Additional infos")
        string += f"\t{translated_field}: {{"
        if self.maturity is not None:
            add_infos = True
            translated_field = _("Maturity")
            string += f"\n\t\t{translated_field}:\t{self.maturity}"
        if self.interest_rate is not None:
            add_infos = True
            translated_field = _("Interest rate")
            interest_rate = (
                f"{self.interest_rate}"
                if isinstance(self.interest_rate, Promise)
                else f"{self.interest_rate:.3%}"
            )
            string += f"\n\t\t{translated_field}:\t{interest_rate}"
        string += "\n\t}\n" if add_infos else "\t}\n"
        return string


class AssetsManager(BaseModel, ABC, PromisableDict):
    """Abstract base class for entities that manage investment funds.

    Parameters
    ----------
    name : str
        Name of the asset manager.
    managed_funds : Set[str]
        Set of fund names managed by this entity.
    """

    name: str = Field(serialization_alias="Name")
    managed_funds: Set[str] = Field(exclude=True)

    def __repr__(self) -> str:
        """Return a string representation of the asset manager."""
        return f'{self.__class__.__name__}("{self.name}")'

    def __hash__(self) -> int:
        """Return a hash based on name and managed funds."""
        return hash((self.name, frozenset(self.managed_funds)))


class ManagementCompany(AssetsManager):
    """Rappresent the manager"""


class InvestmentsManager(AssetsManager):
    """Rappresent the InvestmentsManager"""


class Fund(BaseModel, MatchFund, PromisableDict):
    """Represents an investment fund identified by its name.

    Supports both resolved and promised (deferred) fund names.
    """

    name: PromisedFundName = Field(serialization_alias="Name")

    def __init__(self, name: PromisedFundName) -> None:
        """Initialize a Fund with a name, resolving it if not a Promise.

        Parameters
        ----------
        name : PromisedFundName
            Fund name, either a string or a Promise that resolves to one.
        """
        BaseModel.__init__(self, name=name)
        if not isinstance(name, Promise):
            MatchFund.__init__(self, name)

    def __hash__(self) -> int:
        """Return a hash based on the fund name."""
        if isinstance(self.name, Promise):
            return hash(self.name)
        return MatchFund.__hash__(self)

    def __eq__(self, other: object) -> bool:
        """Compare two Fund instances by name.

        Parameters
        ----------
        other : object
            Another Fund instance to compare with.

        Returns
        -------
        bool
            True if the funds have the same name.
        """
        if isinstance(self.name, Promise) or isinstance(other.name, Promise):
            return isinstance(self.name, type(other.name)) and self.name == other.name
        return MatchFund.__eq__(self, other)


class FundSfdrClassification(BaseModel, PromisableDict):
    """SFDR (Sustainable Finance Disclosure Regulation) classification for a fund.

    Parameters
    ----------
    fund : str
        Name of the fund.
    article : PromisedSfdrArticle
        SFDR article classification (Art. 6, 8, or 9).
    """

    fund: str = Field(exclude=True)
    article: PromisedSfdrArticle = Field(exclude=True)

    def __hash__(self) -> int:
        """Return a hash based on fund name and article."""
        return hash((self.fund, self.article))


class FundEsgIndicator(BaseModel, PromisableDict):
    """ESG (Environmental, Social, Governance) indicator for a fund.

    Parameters
    ----------
    fund : PromisedFundName
        Name of the fund this indicator belongs to.
    name : str
        Name of the ESG indicator.
    value : str
        Value of the ESG indicator.
    """

    fund: PromisedFundName = Field(exclude=True)
    name: str = Field(serialization_alias="Indicator")
    value: str = Field(serialization_alias="Value")


class FundAssets(BaseModel, PromisableDict):
    """Fund balance sheet data including total assets, liabilities, and net assets.

    Parameters
    ----------
    fund : str
        Name of the fund.
    date : Optional[PromisedDate]
        Date of the balance sheet snapshot.
    tot_assets : NonNegativeFloat
        Total assets value.
    liabilities : NonNegativeFloat
        Total liabilities value.
    net_assets : NonNegativeFloat
        Total net assets value.
    currency : PromisedCurrency
        Currency of the monetary values.
    """

    fund: str = Field(exclude=True)
    date: Optional[PromisedDate] = Field(default=None, serialization_alias="Date")
    tot_assets: NonNegativeFloat = Field(serialization_alias="Total assets")
    liabilities: NonNegativeFloat = Field(serialization_alias="Total liabilities")
    net_assets: NonNegativeFloat = Field(serialization_alias="Total net assets")
    currency: PromisedCurrency = Field(serialization_alias="Currency")

    @model_validator(mode="after")
    def validate_assets_equation(self) -> "FundAssets":
        """Validate that liabilities + net assets equals total assets.

        Returns
        -------
        FundAssets
            The validated instance.

        Raises
        ------
        ValueError
            If the accounting equation is not balanced.
        """
        if (
            abs(self.liabilities + self.net_assets - self.tot_assets) > 1e-4
        ):  # Tolerance for float comparison
            raise ValueError(
                f"liabilities ({self.liabilities}) + net_assets ({self.net_assets}) "
                f"must equal tot_assets ({self.tot_assets})"
            )
        return self

    def __repr__(self) -> str:
        """Return a string representation of the fund assets."""
        return (
            f'{self.__class__.__name__}(fund="{self.fund}",'
            f"tot_assets={self.tot_assets},liabilities={self.liabilities},"
            f"net_assets={self.net_assets},currency={self.currency})"
        )

    def __hash__(self) -> int:
        """Return a hash based on fund name and balance sheet values."""
        return hash(
            (
                self.tot_assets,
                self.liabilities,
                self.net_assets,
                self.currency,
                self.fund,
            )
        )


class FundChangeName(BaseModel, PromisableDict):
    """Represents a fund name change or merge event.

    Parameters
    ----------
    old_name : str
        Previous name of the fund.
    current_name : str
        Current (new) name of the fund.
    date : PromisedDate
        Date when the name change took effect.
    """

    old_name: str = Field(serialization_alias="Old name")
    current_name: str = Field(exclude=True)
    date: PromisedDate = Field(serialization_alias="From")

    def __hash__(self) -> int:
        """Return a hash based on old name, current name, and date."""
        return hash((self.old_name, self.current_name, self.date))


class FundRename(FundChangeName):
    """Fund change name (new name doesn't exsists before)"""


class FundMerge(FundChangeName):
    """Fund get merged into another (current fund name exsists before)"""
