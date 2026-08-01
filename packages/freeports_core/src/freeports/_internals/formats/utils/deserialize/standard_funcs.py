"""Utils for creating deserialize routines and functions"""

from logging import getLogger
from typing import Callable, TypeAlias, Optional
from datetime import date, datetime
import re
from freeports._internals.core.classes import TextBlock, LineParseFail
from freeports.consts import Currency
from freeports._internals.core.promises import Promise
from freeports.output import (
    Equity,
    Bond,
    Fund,
    ManagementCompany,
    InvestmentsManager,
    FundAssets,
    FundSfdrClassification,
)
from freeports.i18n import _
from freeports._internals.core.logging import LOG_ADAPT_INVESTMENT_INFOS
from freeports.interfaces.text_blks import ResultStandardFiltering
from freeports._internals.core.normalization import normalize_word, normalize_string
from .cast import (
    perc_to_float,
    to_int,
    to_float,
    to_str,
    to_currency,
    to_date,
    to_int_en_month,
    to_date_with_en_month,
    to_int_it_month,
    to_date_with_it_month,
)

logger = getLogger(__name__)

DeserializeFunc: TypeAlias = Callable[[TextBlock], Equity | Bond]


class DeserializeSfdrArticleStandard:
    def __call__(self, txt_blk):
        return FundSfdrClassification(
            fund=txt_blk.content, article=txt_blk.metadata["article"]
        )


class DeserializerPageClassifyStandard:
    def __call__(self, txt_blk):
        return txt_blk.metadata["page_type"]


def deserialize_block_type(blk_type):
    def wrapper(f):
        def new_f(txt_blk):
            if txt_blk.type_block == blk_type:
                return f(txt_blk)

        return new_f

    return wrapper


def deserialize_block_types(*blk_types):
    def wrapper(f):
        def new_f(txt_blk):
            if any(map(lambda x: txt_blk.type_block == x, blk_types)):
                return f(txt_blk)

        return new_f

    return wrapper


def deserialize_block_type_call(blk_type):
    def wrapper(f):
        def new_f(self, txt_blk):
            if txt_blk.type_block == blk_type:
                return f(self, txt_blk)

        return new_f

    return wrapper


def deserialize_block_types_call(*blk_types):
    def wrapper(f):
        def new_f(self, txt_blk):
            if any(map(lambda x: txt_blk.type_block == x, blk_types)):
                return f(self, txt_blk)

        return new_f

    return wrapper


class DeserializerFundStandard:
    @deserialize_block_type_call(ResultStandardFiltering.FUND)
    def __call__(self, txt_blk):
        return Fund(name=txt_blk.content)


class DeserializerManagmentCompanyStandard:
    @deserialize_block_type_call(ResultStandardFiltering.MANAGEMENT_COMPANY)
    def __call__(self, txt_blk):
        return ManagementCompany(
            name=" ".join(txt_blk.content.strip().split()),
            managed_funds=set(txt_blk.metadata["managed_funds"]),
        )


class DeserializerInvestmentsManagerFromManco:
    @deserialize_block_type_call(ResultStandardFiltering.MANAGEMENT_COMPANY)
    def __call__(self, txt_blk):
        return InvestmentsManager(
            name=" ".join(txt_blk.content.strip().split()),
            managed_funds=set(txt_blk.metadata["managed_funds"]),
        )


class DeserializerInvestmentsManagerStandard:
    @deserialize_block_type_call(ResultStandardFiltering.INVESTMENTS_MANAGER)
    def __call__(self, txt_blk):
        return InvestmentsManager(
            name=" ".join(txt_blk.content.strip().split()),
            managed_funds=set(txt_blk.metadata["managed_funds"]),
        )


class DeserializerInvestmentStandard:
    cost_and_value_interpret_int: bool = True
    quantity_interpret_float: bool = False

    def __init__(
        self,
        cost_and_value_interpret_int: bool = True,
        quantity_interpret_float: bool = False,
    ):
        self.cost_and_value_interpret_int = cost_and_value_interpret_int
        self.quantity_interpret_float = quantity_interpret_float

    @deserialize_block_types_call(
        ResultStandardFiltering.BOND_TARGET, ResultStandardFiltering.EQUITY_TARGET
    )
    def __call__(self, txt_blk):
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
        md = txt_blk.metadata
        LOG_ADAPT_INVESTMENT_INFOS.company = md["company"]
        LOG_ADAPT_INVESTMENT_INFOS.company_match = md["company match"]

        def float_cast(x):
            if self.cost_and_value_interpret_int:
                return float(to_int(x))
            return to_float(x)

        def quantity_cast(x):
            if self.quantity_interpret_float:
                return to_float(x)
            return float(to_int(x))

        def try_cast(md, key, cast_func):
            LOG_ADAPT_INVESTMENT_INFOS.field = key
            if key not in md or md[key] is None:
                LOG_ADAPT_INVESTMENT_INFOS.field = None
                return None
            try:
                tmp = cast_func(md[key])
                LOG_ADAPT_INVESTMENT_INFOS.field = None
                return tmp
            except ValueError:
                logger.error(
                    _("Error casting, found: %s"),
                    str(md[key]).replace("\n", "\\n"),
                )
                logger.warning(_("Skipping field"))
                logger.debug(str(md))
                LOG_ADAPT_INVESTMENT_INFOS.field = None
                return None

        try:
            args = {
                "company": to_str(md["company"]),
                "company_match": to_str(md["company match"]),
                "fund": md["fund"],
                "manco": to_str(md["manco"]) if md.get("manco") else None,
                "market_value": float_cast(md["market value"]),
                "currency": to_currency(md["currency"]),
                "nominal_quantity": try_cast(md, "quantity", quantity_cast),
                "perc_net_assets": try_cast(md, "% net assets", perc_to_float),
                "acquisition_cost": try_cast(md, "acquisition cost", float_cast),
                "acquisition_currency": try_cast(
                    md, "acquisition currency", to_currency
                ),
            }
            if txt_blk.type_block == ResultStandardFiltering.EQUITY_TARGET:
                LOG_ADAPT_INVESTMENT_INFOS.company = None
                LOG_ADAPT_INVESTMENT_INFOS.company_match = None
                return Equity(**args)
            if txt_blk.type_block == ResultStandardFiltering.BOND_TARGET:
                LOG_ADAPT_INVESTMENT_INFOS.company = None
                LOG_ADAPT_INVESTMENT_INFOS.company_match = None
                return Bond(
                    **args,
                    maturity=to_date(md["maturity"]) if "maturity" in md else None,
                    interest_rate=perc_to_float(md["interest rate"])
                    if "interest rate" in md
                    else None,
                )

        except ValueError as e:
            logger.error(_("Cast error"))
            LOG_ADAPT_INVESTMENT_INFOS.company = None
            LOG_ADAPT_INVESTMENT_INFOS.company_match = None
            raise LineParseFail(e) from e


class DeserializerAssetsStandard:
    num_converter: Callable[[str], float | int]
    date_converter: Optional[Callable[[str], float | int]]

    def __init__(self, num_converter, date_converter=to_date):
        self.num_converter = num_converter
        self.date_converter = date_converter

    def __call__(self, blk):
        md = {**blk.metadata}
        return FundAssets(
            fund=md["fund"],
            currency=to_currency(md["currency"]),
            tot_assets=float(self.num_converter(md["tot_assets"])),
            net_assets=float(self.num_converter(md["net_assets"])),
            liabilities=float(
                self.num_converter(md["liabilities"].replace("(", "").replace(")", ""))
            ),
            date=None
            if "date" not in md or md["date"] is None
            else self.date_converter(md["date"]),
        )
