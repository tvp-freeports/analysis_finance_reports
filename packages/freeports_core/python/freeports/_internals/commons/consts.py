"""Implementation independent constants and classes

``FinancialInstrument``, ``SfdrArticle``, and ``Currency`` are now implemented in Rust — see
``packages/freeports_engine/src/core/consts.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. The names here delegate to that
implementation. The original (pre-Rust-port) ``_LegacyFinancialInstrument``/``_LegacySfdrArticle``/
``_LegacyCurrency`` dead-code ``Enum`` bodies this module used to keep for reference were removed
during the freeports_core -> freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``).

Known, accepted behavior differences from the legacy Python `enum.Enum` versions (see the Rust
module's own docstring for the full rationale): no singleton identity (nothing in this codebase
relies on ``is`` for these types, only ``==``/hashing), and ``for e in Currency`` no longer works
(the three call sites that used it, in the now-removed ``output/files_schema.py``, were updated to
iterate ``Currency.__members__.values()`` instead before that module itself was deleted as dead
code).
"""

from freeports import _native

from .i18n import _

FinancialInstrument = _native.FinancialInstrument
SfdrArticle = _native.SfdrArticle
Currency = _native.Currency

__all__ = ["FinancialInstrument", "SfdrArticle", "Currency", "PROGRAM_DESCRIPTION"]

PROGRAM_DESCRIPTION = _(
    """Analyze finance reports searching for investing in companies
allegedly involved interantional law violations by third parties
"""
)
