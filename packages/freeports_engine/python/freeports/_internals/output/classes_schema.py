"""This module contains the schema of the output classes.

Here are defining the object that will be outputted in the csv files.

Every class in this module is now implemented in Rust — see
``packages/freeports_engine/src/core/{investment,assets_manager,fund,fund_sfdr_classification,
fund_esg_indicator,fund_assets,fund_change_name}.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. None of the concrete classes
(``Equity``/``Bond``, ``ManagementCompany``/``InvestmentsManager``, ``Fund``,
``FundSfdrClassification``, ``FundEsgIndicator``, ``FundAssets``, ``FundRename``/``FundMerge``)
is ever subclassed anywhere in the formats repo, and the two abstract bases (``Investment``,
``AssetsManager``) are never directly instantiated — only used for ``isinstance()`` checks — so
they're replaced with plain tuples of their concrete Rust types (a drop-in replacement for
``isinstance(x, Investment)``, not a new abstraction).

The original (pre-Rust-port) Python bodies, and the whole ``PromisableDict``/``Promised*``/
``MatchFund``-based machinery they depended on, were removed during the freeports_core ->
freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``, Part 2b) — a
workspace-wide grep confirmed every one of those symbols was by then referenced only from this
module's own dead ``_Legacy*`` classes, so nothing here still needs them.
"""

import freeports_engine

Equity = freeports_engine.Equity
Bond = freeports_engine.Bond
ManagementCompany = freeports_engine.ManagementCompany
InvestmentsManager = freeports_engine.InvestmentsManager
Fund = freeports_engine.Fund
FundSfdrClassification = freeports_engine.FundSfdrClassification
FundEsgIndicator = freeports_engine.FundEsgIndicator
FundAssets = freeports_engine.FundAssets
FundRename = freeports_engine.FundRename
FundMerge = freeports_engine.FundMerge

# `Investment`/`AssetsManager`/`FundChangeName` used to be the common Pydantic base classes of
# their respective concrete subclasses; now that those are independent Rust pyclasses (see module
# docstring), there's no single base type. Every real usage of these three names is either an
# `isinstance()` check or a plain import, both of which work identically against a tuple of
# types — a drop-in replacement, not a new abstraction.
Investment = (Equity, Bond)
AssetsManager = (ManagementCompany, InvestmentsManager)
FundChangeName = (FundRename, FundMerge)
