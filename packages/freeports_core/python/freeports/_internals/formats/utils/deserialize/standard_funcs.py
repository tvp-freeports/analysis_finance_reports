"""Utils for creating deserialize routines and functions

``DeserializeSfdrArticleStandard``, ``DeserializerPageClassifyStandard``,
``DeserializerFundStandard``, ``DeserializerManagmentCompanyStandard``,
``DeserializerInvestmentsManagerFromManco``, ``DeserializerInvestmentsManagerStandard`` are now
implemented in Rust — see
``packages/freeports_engine/src/deserialize/standard_funcs.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. The original (pre-Rust-port)
``_LegacyDeserializeSfdrArticleStandard``/``_LegacyDeserializerPageClassifyStandard``/
``_LegacyDeserializerFundStandard``/``_LegacyDeserializerManagmentCompanyStandard``/
``_LegacyDeserializerInvestmentsManagerFromManco``/``_LegacyDeserializerInvestmentsManagerStandard``
dead-code class bodies this module used to keep for reference were removed during the
freeports_core -> freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``). ``TextBlock``/
``LineParseFail`` (formerly re-exported via the now-removed, alias-only ``_internals.core.classes``
shim module) are imported directly from ``freeports._native.core`` here per that same
consolidation's Decision 3.

``DeserializerInvestmentStandard``/``DeserializerAssetsStandard`` stay Python deliberately: their
``__call__`` bodies are mostly per-field ``try``/``except`` + translated logging wrapped around
calls into the already-Rust ``cast.*`` functions and ``Equity``/``Bond``/``FundAssets``
constructors — the actual computation is already Rust; what is left is OS/i18n/logging glue,
which this migration keeps in Python throughout. The four ``deserialize_block_type*`` decorator
factories also stay Python: generic higher-order functions wrapping arbitrary Python callables
(used across the whole formats repo, not just this file).
"""

from logging import getLogger
from typing import Any, Callable, TypeAlias, Optional

from freeports import _native

TextBlock = _native.core.TextBlock
LineParseFail = _native.core.LineParseFail

from freeports.output import (
    Equity,
    Bond,
    Fund,
    ManagementCompany,
    InvestmentsManager,
    FundAssets,
)
from freeports.i18n import _
from freeports._internals.core.logging import LOG_ADAPT_INVESTMENT_INFOS
from freeports.interfaces.text_blks import ResultStandardFiltering
from .cast import (
    perc_to_float,
    to_int,
    to_float,
    to_str,
    to_currency,
    to_date,
)

logger = getLogger(__name__)

DeserializeFunc: TypeAlias = Callable[[TextBlock], Equity | Bond]


DeserializeSfdrArticleStandard = _native.core.DeserializeSfdrArticleStandard
DeserializerPageClassifyStandard = _native.core.DeserializerPageClassifyStandard
DeserializerFundStandard = _native.core.DeserializerFundStandard
DeserializerManagmentCompanyStandard = _native.core.DeserializerManagmentCompanyStandard
DeserializerInvestmentsManagerFromManco = (
    _native.core.DeserializerInvestmentsManagerFromManco
)
DeserializerInvestmentsManagerStandard = (
    _native.core.DeserializerInvestmentsManagerStandard
)


def deserialize_block_type(blk_type: str) -> Callable[..., Callable[..., Any]]:
    """Decorator that restricts deserialization to a single block type."""

    def wrapper(f: Callable[..., Any]) -> Callable[..., Any]:
        def new_f(txt_blk: TextBlock) -> Any:
            if txt_blk.type_block == blk_type:
                return f(txt_blk)

        return new_f

    return wrapper


def deserialize_block_types(*blk_types: str) -> Callable[..., Callable[..., Any]]:
    """Decorator that restricts deserialization to multiple block types."""

    def wrapper(f: Callable[..., Any]) -> Callable[..., Any]:
        def new_f(txt_blk: TextBlock) -> Any:
            if any(map(lambda x: txt_blk.type_block == x, blk_types)):
                return f(txt_blk)

        return new_f

    return wrapper


def deserialize_block_type_call(blk_type: str) -> Callable[..., Callable[..., Any]]:
    """Decorator that restricts deserialization to a single block type (method variant)."""

    def wrapper(f: Callable[..., Any]) -> Callable[..., Any]:
        def new_f(self: Any, txt_blk: TextBlock) -> Any:
            if txt_blk.type_block == blk_type:
                return f(self, txt_blk)

        return new_f

    return wrapper


def deserialize_block_types_call(*blk_types: str) -> Callable[..., Callable[..., Any]]:
    """Decorator that restricts deserialization to multiple block types (method variant)."""

    def wrapper(f: Callable[..., Any]) -> Callable[..., Any]:
        def new_f(self: Any, txt_blk: TextBlock) -> Any:
            if any(map(lambda x: txt_blk.type_block == x, blk_types)):
                return f(self, txt_blk)

        return new_f

    return wrapper


class DeserializerInvestmentStandard:
    """Deserializes a text block into an Equity or Bond investment object."""

    cost_and_value_interpret_int: bool = True
    quantity_interpret_float: bool = False

    def __init__(
        self,
        cost_and_value_interpret_int: bool = True,
        quantity_interpret_float: bool = False,
    ) -> None:
        """Initialize the investment deserializer.

        Parameters
        ----------
        cost_and_value_interpret_int : bool
            Whether to interpret cost and value as integers.
        quantity_interpret_float : bool
            Whether to interpret quantity as float.
        """
        self.cost_and_value_interpret_int = cost_and_value_interpret_int
        self.quantity_interpret_float = quantity_interpret_float

    @deserialize_block_types_call(
        ResultStandardFiltering.BOND_TARGET.name,
        ResultStandardFiltering.EQUITY_TARGET.name,
    )
    def __call__(self, txt_blk: TextBlock) -> Equity | Bond:
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
                "market_value": float_cast(md["market value"]),
                "currency": to_currency(md["currency"]),
                "nominal_quantity": try_cast(md, "quantity", quantity_cast),
                "perc_net_assets": try_cast(md, "% net assets", perc_to_float),
                "acquisition_cost": try_cast(md, "acquisition cost", float_cast),
                "acquisition_currency": try_cast(
                    md, "acquisition currency", to_currency
                ),
            }
            if txt_blk.type_block == ResultStandardFiltering.EQUITY_TARGET.name:
                LOG_ADAPT_INVESTMENT_INFOS.company = None
                LOG_ADAPT_INVESTMENT_INFOS.company_match = None
                return Equity(**args)
            if txt_blk.type_block == ResultStandardFiltering.BOND_TARGET.name:
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
    """Deserializes a text block into a FundAssets object."""

    num_converter: Callable[[str], float | int]
    date_converter: Optional[Callable[[str], float | int]]

    def __init__(
        self,
        num_converter: Callable[[str], float | int],
        date_converter: Callable[..., Any] = to_date,
    ) -> None:
        """Initialize the assets deserializer.

        Parameters
        ----------
        num_converter : Callable[[str], float | int]
            Function to convert numeric strings to numbers.
        date_converter : Callable
            Function to convert date strings.
        """
        self.num_converter = num_converter
        self.date_converter = date_converter

    def __call__(self, blk: TextBlock) -> FundAssets:
        """Deserialize FundAssets from a text block.

        Parameters
        ----------
        blk : TextBlock
            The text block containing assets metadata.

        Returns
        -------
        FundAssets
            The deserialized fund assets.
        """
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
